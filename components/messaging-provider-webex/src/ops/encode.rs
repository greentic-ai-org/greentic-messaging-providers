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
