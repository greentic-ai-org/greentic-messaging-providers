use base64::{Engine as _, engine::general_purpose};
use greentic_types::messaging::universal_dto::SendPayloadInV1;
use provider_common::helpers::{send_payload_error, send_payload_success};
use serde_json::Value;

use crate::PROVIDER_TYPE;
use crate::ops::send::handle_send;

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
    let payload_bytes = match general_purpose::STANDARD.decode(&send_in.payload.body_b64) {
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

fn forward_send_payload(payload: &Value) -> Result<(), String> {
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
    check_media_results(&result_value)
}

// `handle_send` reports the final text/interactive send as the top-level `ok`,
// but attaches per-media outcomes in `media_results` whose failures are not
// reflected there. Flow-based callers may want that partial-success shape, but
// the new-model host egress treats `ok: true` as a successful delivery and
// will not retry — silently losing media. Downgrade to a retriable failure
// here so the host's reply-egress doesn't ack a partial send.
fn check_media_results(result_value: &Value) -> Result<(), String> {
    let Some(media) = result_value.get("media").and_then(Value::as_array) else {
        return Ok(());
    };
    let failures: Vec<String> = media
        .iter()
        .filter(|m| m.get("ok").and_then(Value::as_bool) == Some(false))
        .map(|m| {
            let kind = m.get("type").and_then(Value::as_str).unwrap_or("media");
            let detail = m
                .get("error")
                .or_else(|| m.get("detail"))
                .and_then(Value::as_str)
                .unwrap_or("unknown error");
            format!("{kind}: {detail}")
        })
        .collect();
    if failures.is_empty() {
        Ok(())
    } else {
        Err(format!(
            "media send failed ({} of {}): [{}]",
            failures.len(),
            media.len(),
            failures.join("; ")
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use base64::engine::general_purpose::STANDARD;
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
                .unwrap_or_default()
                .contains("invalid send_payload input")
        );
    }

    #[test]
    fn send_payload_rejects_wrong_provider_before_decoding_payload() {
        let input = send_payload_input("other", "not base64".to_string());
        let body = parse_result(send_payload(&serde_json::to_vec(&input).expect("input")));

        assert_eq!(body["ok"], false);
        assert_eq!(body["message"], "provider type mismatch");
    }

    #[test]
    fn send_payload_rejects_invalid_base64() {
        let input = send_payload_input(PROVIDER_TYPE, "not base64".to_string());
        let body = parse_result(send_payload(&serde_json::to_vec(&input).expect("input")));

        assert_eq!(body["ok"], false);
        assert!(
            body["message"]
                .as_str()
                .unwrap_or_default()
                .contains("payload decode failed")
        );
    }

    #[test]
    fn send_payload_returns_send_error_for_missing_config_without_network() {
        let input = send_payload_input(PROVIDER_TYPE, STANDARD.encode(br#"{"text":"hello"}"#));
        let body = parse_result(send_payload(&serde_json::to_vec(&input).expect("input")));

        assert_eq!(body["ok"], false);
        assert_eq!(body["message"], "config required");
        assert_eq!(body["retryable"], false);
    }

    #[test]
    fn check_media_results_passes_when_no_media_array() {
        let value = serde_json::json!({ "ok": true });
        assert!(check_media_results(&value).is_ok());
    }

    #[test]
    fn check_media_results_passes_when_all_media_ok() {
        let value = serde_json::json!({
            "ok": true,
            "media": [
                { "type": "image", "ok": true },
                { "type": "document", "ok": true },
            ],
        });
        assert!(check_media_results(&value).is_ok());
    }

    #[test]
    fn check_media_results_downgrades_when_any_media_failed() {
        let value = serde_json::json!({
            "ok": true,
            "media": [
                { "type": "image", "ok": true },
                { "type": "video", "ok": false, "detail": "Err(\"transport\")" },
            ],
        });
        let err = check_media_results(&value).expect_err("must downgrade");
        assert!(err.contains("media send failed"));
        assert!(err.contains("video: Err"));
    }
}
