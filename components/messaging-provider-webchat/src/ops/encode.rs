//! Step 2 of the egress pipeline: universal message → provider payload.
//!
//! Serializes a `ChannelMessageEnvelope`-derived `EncodeMessage` into a
//! `ProviderPayloadV1` whose body is a JSON blob understood by the webchat
//! send/persist path. Passes through adaptive card, tenant, env, and team metadata.

use base64::{Engine as _, engine::general_purpose};
use greentic_types::messaging::universal_dto::ProviderPayloadV1;
use provider_common::helpers::{decode_encode_message, encode_error, json_bytes};
use serde_json::{Value, json};
use std::collections::BTreeMap;

use super::helpers::envelope_attachments_to_directline;

pub(crate) fn encode_op(input_json: &[u8]) -> Vec<u8> {
    let encode_message = match decode_encode_message(input_json) {
        Ok(value) => value,
        Err(err) => return encode_error(&err),
    };
    let has_adaptive_card = encode_message.metadata.contains_key("adaptive_card")
        || encode_message.extensions.contains_key("adaptive_card");
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
    let team = encode_message.metadata.get("team").cloned();
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
    if let Some(ref team) = team {
        payload_body["team"] = Value::String(team.clone());
    }
    if !encode_message.extensions.is_empty() {
        payload_body["extensions"] =
            serde_json::to_value(&encode_message.extensions).unwrap_or(Value::Null);
    }
    // Forward the envelope's top-level `attachments` field, converted to the
    // DirectLine shape. These are distinct from `extensions["attachments"]` —
    // upstream components may populate either (the typed envelope field is the
    // canonical path; extensions is the escape hatch for provider-native shapes).
    if !encode_message.attachments.is_empty() {
        payload_body["attachments"] =
            envelope_attachments_to_directline(&encode_message.attachments);
    }
    let body_bytes = serde_json::to_vec(&payload_body).unwrap_or_else(|_| b"{}".to_vec());
    let mut metadata = BTreeMap::new();
    metadata.insert("route".to_string(), Value::String(route_value.clone()));
    metadata.insert("method".to_string(), Value::String("POST".to_string()));
    if let Some(tenant) = tenant {
        metadata.insert("tenant".to_string(), Value::String(tenant));
    }
    if let Some(team) = team {
        metadata.insert("team".to_string(), Value::String(team));
    }
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
    use serde_json::json;

    fn envelope_with_attachments(attachments: Value) -> Value {
        json!({
            "message": {
                "id": "msg-1",
                "tenant": {
                    "env": "demo",
                    "tenant": "demo",
                    "tenant_id": "demo",
                    "attempt": 0
                },
                "channel": "webchat",
                "session_id": "conv-1",
                "text": "see image",
                "attachments": attachments,
                "metadata": {}
            }
        })
    }

    fn decode_payload_body(response: &[u8]) -> Value {
        let resp: Value = serde_json::from_slice(response).unwrap();
        assert_eq!(resp["ok"], true, "encode_op should succeed: {resp:?}");
        let body_b64 = resp["payload"]["body_b64"].as_str().unwrap();
        let body_bytes = general_purpose::STANDARD.decode(body_b64).unwrap();
        serde_json::from_slice(&body_bytes).unwrap()
    }

    #[test]
    fn encode_op_forwards_envelope_attachments_in_directline_shape() {
        let envelope = envelope_with_attachments(json!([
            {
                "mime_type": "image/png",
                "url": "https://cdn.example.com/diagram.png",
                "name": "diagram.png"
            }
        ]));
        let input = serde_json::to_vec(&envelope).unwrap();
        let body = decode_payload_body(&encode_op(&input));

        let attachments = body["attachments"].as_array().expect("attachments array");
        assert_eq!(attachments.len(), 1);
        assert_eq!(attachments[0]["contentType"], "image/png");
        assert_eq!(
            attachments[0]["contentUrl"],
            "https://cdn.example.com/diagram.png"
        );
        assert_eq!(attachments[0]["name"], "diagram.png");
    }

    #[test]
    fn encode_op_omits_attachments_field_when_envelope_has_none() {
        let envelope = envelope_with_attachments(json!([]));
        let input = serde_json::to_vec(&envelope).unwrap();
        let body = decode_payload_body(&encode_op(&input));
        assert!(body.get("attachments").is_none());
    }
}
