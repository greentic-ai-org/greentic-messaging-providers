//! Step 2 of the egress pipeline: universal message → provider payload.
//!
//! Serializes a `ChannelMessageEnvelope`-derived `EncodeMessage` into a
//! `ProviderPayloadV1` whose body is a JSON blob understood by the webchat
//! send/persist path. Passes through adaptive card, tenant, and env metadata.

use base64::{Engine as _, engine::general_purpose};
use greentic_types::messaging::universal_dto::ProviderPayloadV1;
use provider_common::helpers::{decode_encode_message, encode_error, json_bytes};
use serde_json::{Value, json};
use std::collections::BTreeMap;

pub(crate) fn encode_op(input_json: &[u8]) -> Vec<u8> {
    let encode_message = match decode_encode_message(input_json) {
        Ok(value) => value,
        Err(err) => return encode_error(&err),
    };
    let has_adaptive_card = encode_message.metadata.contains_key("adaptive_card");
    let text = encode_message
        .text
        .clone()
        .filter(|t| !t.trim().is_empty())
        .unwrap_or_else(|| {
            if has_adaptive_card {
                // Don't emit fallback text when an adaptive card is present —
                // it renders natively and the text bubble is redundant.
                String::new()
            } else {
                "webchat universal payload".to_string()
            }
        });
    let metadata_route = encode_message.metadata.get("route").cloned();
    let route = metadata_route
        .clone()
        .or_else(|| Some(encode_message.session_id.clone()));
    let route_value = route.clone().unwrap_or_else(|| "webchat".to_string());
    // Pass through adaptive_card from envelope metadata (set by app flow for AC output).
    let adaptive_card = encode_message.metadata.get("adaptive_card").cloned();
    let tenant = encode_message.metadata.get("tenant").cloned();
    let env = encode_message.metadata.get("env").cloned();
    let mut payload_body = json!({
        "text": text,
        "route": route_value.clone(),
        "session_id": encode_message.session_id,
    });
    if let Some(ac) = &adaptive_card {
        payload_body["adaptive_card"] = Value::String(ac.clone());
    }
    if let Some(ref tenant) = tenant {
        payload_body["tenant"] = Value::String(tenant.clone());
    }
    if let Some(ref env) = env {
        payload_body["env"] = Value::String(env.clone());
    }
    let body_bytes = serde_json::to_vec(&payload_body).unwrap_or_else(|_| b"{}".to_vec());
    let mut metadata = BTreeMap::new();
    metadata.insert("route".to_string(), Value::String(route_value.clone()));
    metadata.insert("method".to_string(), Value::String("POST".to_string()));
    if let Some(tenant) = tenant {
        metadata.insert("tenant".to_string(), Value::String(tenant));
    }
    let payload = ProviderPayloadV1 {
        content_type: "application/json".to_string(),
        body_b64: general_purpose::STANDARD.encode(&body_bytes),
        metadata,
    };
    json_bytes(&json!({"ok": true, "payload": payload}))
}
