use base64::{Engine as _, engine::general_purpose};
use greentic_types::messaging::universal_dto::ProviderPayloadV1;
use provider_common::helpers::{
    decode_encode_message, encode_error, extract_ac_summary, json_bytes,
};
use serde_json::{Value, json};
use std::collections::BTreeMap;

pub(crate) fn encode_op(input_json: &[u8]) -> Vec<u8> {
    let encode_message = match decode_encode_message(input_json) {
        Ok(value) => value,
        Err(err) => return encode_error(&err),
    };

    // If the reply carries an Adaptive Card, downsample it to plain text —
    // SMS is text-only (no AC support).
    let ac_raw_str = provider_common::helpers::resolve_adaptive_card(&encode_message)
        .map(|v| serde_json::to_string(&v).unwrap_or_default());
    let caps = provider_common::render::capabilities_for("sms")
        .expect("sms capabilities must be registered");
    let text = ac_raw_str
        .as_deref()
        .and_then(|ac_raw| extract_ac_summary(ac_raw, &caps))
        .or_else(|| encode_message.text.clone().filter(|t| !t.trim().is_empty()))
        .unwrap_or_else(|| "sms message".to_string());

    // Reply destination: the sender of the inbound message (ingest sets
    // `metadata["from"]`), falling back to the envelope's first `to` entry
    // for messages built outside the inbound-reply path.
    let to_id = encode_message
        .metadata
        .get("from")
        .cloned()
        .or_else(|| encode_message.to.first().map(|d| d.id.clone()))
        .map(|id| id.trim().to_string())
        .filter(|id| !id.is_empty());
    let to_id = match to_id {
        Some(id) => id,
        None => return encode_error("destination phone number required"),
    };

    let payload_body = json!({
        "to": to_id,
        "body": text,
    });
    let body_bytes = serde_json::to_vec(&payload_body).unwrap_or_else(|_| b"{}".to_vec());
    let mut metadata = BTreeMap::new();
    metadata.insert("method".to_string(), Value::String("POST".to_string()));
    let payload = ProviderPayloadV1 {
        content_type: "application/json".to_string(),
        body_b64: general_purpose::STANDARD.encode(&body_bytes),
        metadata,
    };
    json_bytes(&json!({"ok": true, "payload": payload}))
}

#[cfg(test)]
mod tests {
    use super::*;
    use greentic_types::{
        ChannelMessageEnvelope, Destination, EnvId, MessageMetadata, TenantCtx, TenantId,
    };

    fn envelope(text: Option<&str>, to: Vec<Destination>, from_meta: Option<&str>) -> Vec<u8> {
        let env = EnvId::try_from("default").expect("env id");
        let tenant = TenantId::try_from("default").expect("tenant id");
        let mut metadata = MessageMetadata::new();
        if let Some(from) = from_meta {
            metadata.insert("from".to_string(), from.to_string());
        }
        let envelope = ChannelMessageEnvelope {
            id: "sms-reply-1".to_string(),
            tenant: TenantCtx::new(env, tenant),
            channel: "sms".to_string(),
            session_id: "+15551230001".to_string(),
            reply_scope: None,
            from: None,
            to,
            correlation_id: None,
            text: text.map(|s| s.to_string()),
            attachments: Vec::new(),
            metadata,
            extensions: Default::default(),
        };
        serde_json::to_vec(&json!({"message": envelope})).expect("serialize envelope")
    }

    fn decode_payload_body(bytes: &[u8]) -> Value {
        let out: Value = serde_json::from_slice(bytes).expect("json result");
        assert_eq!(out["ok"], true);
        let body_b64 = out["payload"]["body_b64"].as_str().expect("body_b64");
        let body_bytes = general_purpose::STANDARD
            .decode(body_b64)
            .expect("base64 decode");
        serde_json::from_slice(&body_bytes).expect("payload json")
    }

    #[test]
    fn encodes_reply_text_and_destination_from_metadata() {
        let input = envelope(
            Some("thanks for reaching out"),
            vec![],
            Some("+15551230001"),
        );
        let out = encode_op(&input);
        let body = decode_payload_body(&out);
        assert_eq!(body["to"], "+15551230001");
        assert_eq!(body["body"], "thanks for reaching out");
    }

    #[test]
    fn falls_back_to_first_destination_when_no_from_metadata() {
        let input = envelope(
            Some("hello"),
            vec![Destination {
                id: "+15559990000".to_string(),
                kind: Some("phone".to_string()),
            }],
            None,
        );
        let out = encode_op(&input);
        let body = decode_payload_body(&out);
        assert_eq!(body["to"], "+15559990000");
    }

    #[test]
    fn missing_destination_returns_encode_error() {
        let input = envelope(Some("hello"), vec![], None);
        let out = encode_op(&input);
        let value: Value = serde_json::from_slice(&out).expect("json result");
        assert_eq!(value["ok"], false);
        assert!(
            value["error"]
                .as_str()
                .unwrap_or_default()
                .contains("destination phone number required")
        );
    }

    #[test]
    fn empty_text_falls_back_to_default_summary() {
        let input = envelope(Some("   "), vec![], Some("+15551230001"));
        let out = encode_op(&input);
        let body = decode_payload_body(&out);
        assert_eq!(body["body"], "sms message");
    }

    #[test]
    fn invalid_input_returns_encode_error() {
        let out = encode_op(b"not json");
        let value: Value = serde_json::from_slice(&out).expect("json result");
        assert_eq!(value["ok"], false);
    }
}
