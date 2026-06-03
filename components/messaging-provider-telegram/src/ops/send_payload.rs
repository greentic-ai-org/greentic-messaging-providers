//! `send_payload` and `reply` ops for the Telegram provider.
//!
//! [`send_payload`] implements the universal egress step 3 (the runner decodes
//! the previously-encoded payload and asks the provider to push it to the
//! external service). It simply forwards the decoded JSON to
//! [`super::send::handle_send`].
//!
//! [`handle_reply`] handles the legacy `reply` op, which writes a plain
//! `sendMessage` with `reply_to_message_id` set.

use base64::{Engine, engine::general_purpose::STANDARD};
use greentic_types::messaging::universal_dto::SendPayloadInV1;
use provider_common::helpers::{
    check_media_results, json_bytes, send_payload_error, send_payload_success,
};
use serde_json::{Value, json};

use crate::bindings::greentic::http::http_client as client;
use crate::config::{get_bot_token, load_config};
use crate::{DEFAULT_API_BASE, PROVIDER_TYPE};

use super::ingest::extract_ids;
use super::send::handle_send;

pub(crate) fn send_payload(input_json: &[u8]) -> Vec<u8> {
    let send_in = match serde_json::from_slice::<SendPayloadInV1>(input_json) {
        Ok(value) => value,
        Err(err) => {
            return send_payload_error(&format!("invalid send_payload input: {err}"), false);
        }
    };
    if send_in.provider_type != PROVIDER_TYPE {
        return send_payload_error("provider type mismatch", false);
    }
    let payload_bytes = match STANDARD.decode(&send_in.payload.body_b64) {
        Ok(bytes) => bytes,
        Err(err) => {
            return send_payload_error(&format!("payload decode failed: {err}"), false);
        }
    };
    let payload: Value = serde_json::from_slice(&payload_bytes).unwrap_or(Value::Null);
    match forward_send_payload(&payload) {
        Ok(_) => send_payload_success(),
        Err(err) => send_payload_error(&err, false),
    }
}

pub(crate) fn forward_send_payload(payload: &Value) -> Result<(), String> {
    let payload_bytes =
        serde_json::to_vec(payload).map_err(|err| format!("serialize failed: {err}"))?;
    let result = handle_send(&payload_bytes);
    let result_value: Value =
        serde_json::from_slice(&result).map_err(|err| format!("parse send result: {err}"))?;
    let ok = result_value
        .get("ok")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    if !ok {
        let message = result_value
            .get("error")
            .and_then(Value::as_str)
            .map(|s| s.to_string())
            .unwrap_or_else(|| "send_payload failed".to_string());
        return Err(message);
    }
    // `handle_send` returns `ok: true` for the final text/photo/group send
    // even when separately-sent pre-send media (sendVideo/sendDocument/etc.)
    // failed. The new-model host egress wouldn't retry — downgrade to a
    // retriable failure.
    check_media_results(&result_value, "media")
}

pub(crate) fn handle_reply(input_json: &[u8]) -> Vec<u8> {
    let parsed: Value = match serde_json::from_slice(input_json) {
        Ok(val) => val,
        Err(err) => {
            return json_bytes(&json!({"ok": false, "error": format!("invalid json: {err}")}));
        }
    };

    let cfg = match load_config(&parsed) {
        Ok(cfg) => cfg,
        Err(err) => return json_bytes(&json!({"ok": false, "error": err})),
    };
    if !cfg.enabled {
        return json_bytes(&json!({"ok": false, "error": "provider disabled by config"}));
    }

    let text = match parsed
        .get("text")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string())
    {
        Some(t) if !t.is_empty() => t,
        _ => return json_bytes(&json!({"ok": false, "error": "text required"})),
    };

    let chat_id = match parsed
        .get("chat_id")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string())
        .or_else(|| cfg.default_chat_id.clone())
    {
        Some(chat) if !chat.is_empty() => chat,
        _ => return json_bytes(&json!({"ok": false, "error": "chat_id required"})),
    };

    let reply_to = parsed
        .get("reply_to_id")
        .or_else(|| parsed.get("thread_id"))
        .and_then(|v| v.as_str())
        .unwrap_or("");
    if reply_to.is_empty() {
        return json_bytes(&json!({"ok": false, "error": "reply_to_id or thread_id required"}));
    }

    let token = match get_bot_token(&cfg) {
        Ok(s) => s,
        Err(err) => return json_bytes(&json!({"ok": false, "error": err})),
    };

    let api_base = cfg
        .api_base_url
        .clone()
        .unwrap_or_else(|| DEFAULT_API_BASE.to_string());
    let url = format!("{api_base}/bot{token}/sendMessage");
    let payload = json!({
        "chat_id": chat_id,
        "text": text,
        "reply_to_message_id": reply_to
    });
    let request = client::Request {
        method: "POST".to_string(),
        url,
        headers: vec![("Content-Type".to_string(), "application/json".to_string())],
        body: Some(serde_json::to_vec(&payload).unwrap_or_else(|_| b"{}".to_vec())),
    };

    let resp = match client::send(&request, None, None) {
        Ok(resp) => resp,
        Err(err) => {
            return json_bytes(&json!({
                "ok": false,
                "error": format!("transport error: {}", err.message),
            }));
        }
    };

    if resp.status < 200 || resp.status >= 300 {
        return json_bytes(&json!({
            "ok": false,
            "error": format!("telegram returned status {}", resp.status),
        }));
    }

    let body_bytes = resp.body.unwrap_or_default();
    let body_json: Value = serde_json::from_slice(&body_bytes).unwrap_or(Value::Null);
    let (message_id, provider_message_id) = extract_ids(&body_json);

    json_bytes(&json!({
        "ok": true,
        "status": "replied",
        "provider_type": PROVIDER_TYPE,
        "public_base_url": cfg.public_base_url,
        "message_id": message_id,
        "provider_message_id": provider_message_id,
        "response": body_json
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use greentic_types::messaging::universal_dto::{ProviderPayloadV1, SendPayloadInV1};
    use std::collections::BTreeMap;

    fn parse_result(bytes: Vec<u8>) -> Value {
        serde_json::from_slice(&bytes).expect("json result")
    }

    fn send_payload_input(provider_type: &str, body_b64: String) -> SendPayloadInV1 {
        SendPayloadInV1 {
            provider_type: provider_type.to_string(),
            tenant_id: None,
            auth_user: None,
            payload: ProviderPayloadV1 {
                content_type: "application/json".to_string(),
                body_b64,
                metadata: BTreeMap::new(),
            },
        }
    }

    #[test]
    fn send_payload_rejects_invalid_input_json() {
        let body = parse_result(send_payload(b"{"));

        assert_eq!(body["ok"], false);
        assert!(
            body["message"]
                .as_str()
                .unwrap_or("")
                .contains("invalid send_payload input")
        );
    }

    #[test]
    fn send_payload_rejects_provider_mismatch_before_external_call() {
        let input = send_payload_input("other", STANDARD.encode(b"{}"));
        let body = parse_result(send_payload(&serde_json::to_vec(&input).expect("input")));

        assert_eq!(body["ok"], false);
        assert_eq!(body["message"], "provider type mismatch");
    }

    #[test]
    fn send_payload_rejects_invalid_base64_before_external_call() {
        let input = send_payload_input(PROVIDER_TYPE, "not base64".to_string());
        let body = parse_result(send_payload(&serde_json::to_vec(&input).expect("input")));

        assert_eq!(body["ok"], false);
        assert!(
            body["message"]
                .as_str()
                .unwrap_or("")
                .contains("payload decode failed")
        );
    }

    #[test]
    fn handle_reply_validates_required_text_before_secret_lookup() {
        let input = json!({
            "config": {
                "enabled": true,
                "public_base_url": "https://example.com",
                "default_chat_id": "123"
            },
            "reply_to_id": "456"
        });

        let body = parse_result(handle_reply(input.to_string().as_bytes()));

        assert_eq!(body["ok"], false);
        assert_eq!(body["error"], "text required");
    }
}
