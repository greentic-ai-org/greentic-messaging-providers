//! `identify-instance` export — extracts the bot's `recipient.id` from
//! a Bot Framework Activity (Direct Line) so the host can route the
//! inbound to the right `MessagingEndpoint` when multiple WebChat bots
//! share one runtime.
//!
//! Shared by `messaging-provider-webchat` and
//! `messaging-provider-webchat-gui` (the GUI crate re-uses this ops
//! module via `#[path = "../../messaging-provider-webchat/src/ops/mod.rs"]`).

use provider_common::identify::extract_from_wrapper;
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
pub(crate) fn extract_recipient_id(input_json: &[u8]) -> Option<String> {
    extract_from_wrapper(input_json, |value| {
        value
            .get("recipient")
            .and_then(|r| r.get("id"))
            .and_then(Value::as_str)
            .map(str::to_owned)
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use provider_common::identify::test_utils::assert_valid_hint;
    use serde_json::json;

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
    fn returns_recipient_id_from_m1_iid4d_wrapper() {
        let wrapper = json!({
            "headers": [{ "name": "x-forwarded-for", "value": "203.0.113.42" }],
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
    fn identify_hint_json_is_valid() {
        assert_valid_hint(IDENTIFY_HINT_JSON);
    }
}
