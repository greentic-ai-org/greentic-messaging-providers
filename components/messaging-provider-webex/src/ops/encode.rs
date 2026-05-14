//! Step 2 of the egress pipeline: encoding.
//!
//! Serialises the incoming [`ChannelMessageEnvelope`] into a
//! [`ProviderPayloadV1`] whose body is JSON. Webex egress does not require any
//! provider-specific shaping at this stage — the envelope itself is what the
//! `send_payload` step consumes.

use base64::{Engine, engine::general_purpose::STANDARD};
use greentic_types::messaging::universal_dto::ProviderPayloadV1;
use provider_common::helpers::{decode_encode_message, encode_error, json_bytes};
use serde_json::json;
use std::collections::BTreeMap;

pub(crate) fn encode_op(input_json: &[u8]) -> Vec<u8> {
    let envelope = match decode_encode_message(input_json) {
        Ok(value) => value,
        Err(err) => return encode_error(&err),
    };
    let body_bytes = serde_json::to_vec(&envelope).unwrap_or_else(|_| b"{}".to_vec());
    let payload = ProviderPayloadV1 {
        content_type: "application/json".to_string(),
        body_b64: STANDARD.encode(&body_bytes),
        metadata: BTreeMap::new(),
    };
    json_bytes(&json!({"ok": true, "payload": payload}))
}

#[cfg(test)]
mod tests {
    use super::*;
    use base64::engine::general_purpose::STANDARD;
    use greentic_types::{
        ChannelMessageEnvelope, Destination, EnvId, MessageMetadata, TenantCtx, TenantId,
    };
    use serde_json::Value;

    fn envelope() -> ChannelMessageEnvelope {
        ChannelMessageEnvelope {
            id: "msg-1".to_string(),
            tenant: TenantCtx::new(
                EnvId::try_from("dev").expect("env"),
                TenantId::try_from("tenant").expect("tenant"),
            ),
            channel: "webex".to_string(),
            session_id: "session-1".to_string(),
            reply_scope: None,
            from: None,
            to: vec![Destination {
                id: "room-1".to_string(),
                kind: Some("room".to_string()),
            }],
            correlation_id: None,
            text: Some("hello".to_string()),
            attachments: Vec::new(),
            metadata: MessageMetadata::new(),
            extensions: Default::default(),
        }
    }

    #[test]
    fn encode_serializes_envelope_for_send_payload() {
        let result: Value = serde_json::from_slice(&encode_op(
            json!({ "message": envelope() }).to_string().as_bytes(),
        ))
        .expect("result");

        assert_eq!(result["ok"], true);
        assert_eq!(result["payload"]["content_type"], "application/json");
        let body_b64 = result["payload"]["body_b64"].as_str().expect("body_b64");
        let body = STANDARD.decode(body_b64).expect("payload body");
        let body: Value = serde_json::from_slice(&body).expect("body json");
        assert_eq!(body["channel"], "webex");
        assert_eq!(body["to"][0]["id"], "room-1");
        assert_eq!(body["text"], "hello");
    }

    #[test]
    fn encode_reports_invalid_input() {
        let result: Value =
            serde_json::from_slice(&encode_op(br#"{"message":{"bad":true}}"#)).expect("result");

        assert_eq!(result["ok"], false);
        assert!(
            result["error"]
                .as_str()
                .unwrap_or_default()
                .contains("invalid encode input")
        );
    }
}
