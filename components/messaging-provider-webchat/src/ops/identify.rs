//! `identify-instance` export — extracts the bot's `recipient.id` from
//! a Bot Framework Activity (Direct Line) so the host can route the
//! inbound to the right `MessagingEndpoint` when multiple WebChat bots
//! share one runtime.
//!
//! Shared by `messaging-provider-webchat` and
//! `messaging-provider-webchat-gui` (the GUI crate re-uses this ops
//! module via `#[path = "../../messaging-provider-webchat/src/ops/mod.rs"]`).

use serde_json::Value;

/// JSON-encoded `IdentifyInstanceHint` returned from
/// `describe-identify-instance` — declares that the discriminator lives
/// at body path `/recipient/id`, so the host can scope per-provider
/// header allowlists (WebChat needs no inbound headers for
/// identification).
pub(crate) const IDENTIFY_HINT_JSON: &[u8] =
    br#"{"version":1,"sources":[{"body_path":{"json_pointer":"/recipient/id"}}]}"#;

/// **Routing discriminator only — not an authentication check.**
///
/// `recipient.id` is read from the inbound Bot Framework Activity body,
/// which is caller-controlled. Downstream auth — Direct Line secret /
/// conversation-token verification against the routed endpoint's
/// configured credentials — MUST run before any event is admitted.
///
/// The discriminator is the activity's `recipient.id` field — every
/// WebChat / Direct Line registration has a unique bot id that lands in
/// every inbound activity addressed to that bot. The input is accepted
/// in any shape the runtime currently delivers:
///
/// - the raw Bot Framework Activity at the top level (legacy bare
///   body), or
/// - an HttpInV1 / M1 IID.4d wrapper whose `body` is the activity
///   (already-decoded object, JSON array of u8 bytes, or absent with
///   base64 in `body_b64`).
pub(crate) fn extract_recipient_id(input_json: &[u8]) -> Option<String> {
    let value: Value = serde_json::from_slice(input_json).ok()?;
    if let Some(id) = recipient_id_from(&value) {
        return Some(id);
    }
    if let Some(body) = value.get("body").filter(|b| b.is_object()) {
        return recipient_id_from(body);
    }
    let bytes = http_body_bytes(&value)?;
    let parsed: Value = serde_json::from_slice(&bytes).ok()?;
    recipient_id_from(&parsed)
}

fn recipient_id_from(value: &Value) -> Option<String> {
    value
        .get("recipient")
        .and_then(|r| r.get("id"))
        .and_then(Value::as_str)
        .map(str::to_owned)
}

fn http_body_bytes(value: &Value) -> Option<Vec<u8>> {
    if let Some(Value::Array(arr)) = value.get("body") {
        return arr.iter().map(|v| u8::try_from(v.as_u64()?).ok()).collect();
    }
    let b64 = value.get("body_b64").and_then(Value::as_str)?;
    use base64::Engine;
    base64::engine::general_purpose::STANDARD.decode(b64).ok()
}

#[cfg(test)]
mod tests {
    use super::*;
    use base64::Engine;
    use serde_json::json;

    fn b64_body(activity: &Value) -> String {
        base64::engine::general_purpose::STANDARD.encode(serde_json::to_vec(activity).unwrap())
    }

    #[test]
    fn returns_recipient_id_from_top_level_activity() {
        let payload = json!({
            "type": "message",
            "id": "act-1",
            "from": { "id": "user-1" },
            "recipient": { "id": "bot-legal", "name": "Legal Bot" },
            "conversation": { "id": "conv-1" },
            "text": "hello"
        });
        let bytes = serde_json::to_vec(&payload).unwrap();
        assert_eq!(extract_recipient_id(&bytes).as_deref(), Some("bot-legal"));
    }

    #[test]
    fn returns_recipient_id_from_m1_iid4d_wrapper_with_decoded_body() {
        let wrapper = json!({
            "headers": [
                { "name": "x-forwarded-for", "value": "203.0.113.42" }
            ],
            "body": {
                "type": "message",
                "recipient": { "id": "bot-accounting" },
                "from": { "id": "user-2" }
            }
        });
        let bytes = serde_json::to_vec(&wrapper).unwrap();
        assert_eq!(
            extract_recipient_id(&bytes).as_deref(),
            Some("bot-accounting")
        );
    }

    #[test]
    fn returns_recipient_id_from_wrapper_with_body_b64() {
        let activity = json!({
            "type": "message",
            "recipient": { "id": "bot-via-b64" }
        });
        let wrapper = json!({
            "headers": [],
            "body_b64": b64_body(&activity)
        });
        let bytes = serde_json::to_vec(&wrapper).unwrap();
        assert_eq!(extract_recipient_id(&bytes).as_deref(), Some("bot-via-b64"));
    }

    #[test]
    fn returns_recipient_id_from_wrapper_with_body_byte_array() {
        let activity = json!({
            "type": "message",
            "recipient": { "id": "bot-via-bytes" }
        });
        let bytes = serde_json::to_vec(&activity).unwrap();
        let wrapper = json!({
            "headers": [],
            "body": bytes.iter().map(|b| *b as u64).collect::<Vec<_>>()
        });
        let input = serde_json::to_vec(&wrapper).unwrap();
        assert_eq!(
            extract_recipient_id(&input).as_deref(),
            Some("bot-via-bytes")
        );
    }

    #[test]
    fn returns_none_when_recipient_absent() {
        let payload = json!({
            "type": "message",
            "from": { "id": "user-1" },
            "text": "hi"
        });
        let bytes = serde_json::to_vec(&payload).unwrap();
        assert!(extract_recipient_id(&bytes).is_none());
    }

    #[test]
    fn returns_none_for_unparseable_input() {
        assert!(extract_recipient_id(b"not json").is_none());
    }

    #[test]
    fn identify_hint_json_parses_with_version_one_and_non_empty_sources() {
        let value: Value = serde_json::from_slice(IDENTIFY_HINT_JSON).expect("parse hint");
        assert_eq!(value.get("version").and_then(Value::as_u64), Some(1));
        let sources = value
            .get("sources")
            .and_then(Value::as_array)
            .expect("sources array");
        assert!(!sources.is_empty(), "hint sources must be non-empty");
    }
}
