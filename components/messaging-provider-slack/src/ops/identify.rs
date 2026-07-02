//! `identify-instance` export — extracts the receiving Slack app's
//! `api_app_id` from an Events API payload so the host can route the
//! inbound to the right `MessagingEndpoint` when multiple Slack apps
//! share one runtime.

use provider_common::identify::extract_from_wrapper;
use serde_json::Value;

/// JSON-encoded `IdentifyInstanceHint` returned from
/// `describe-identify-instance` — declares that the discriminator lives
/// at body path `/api_app_id`, so the host can scope per-provider header
/// allowlists (Slack needs no inbound headers for identification).
pub(crate) const IDENTIFY_HINT_JSON: &[u8] =
    br#"{"version":1,"sources":[{"body_path":{"json_pointer":"/api_app_id"}}]}"#;

/// **Routing discriminator only — not an authentication check.**
///
/// `api_app_id` is read from the inbound Slack Events API JSON body,
/// which is caller-controlled. Downstream auth — `X-Slack-Signature`
/// verification against the routed endpoint's signing secret — MUST
/// run before any event is admitted.
///
/// Returns `None` for payloads without `api_app_id`:
/// - `url_verification` (challenge handshake),
/// - URL-encoded `payload=<json>` interactive form bodies (the host
///   parses the body as JSON before wrapping; non-JSON bodies arrive
///   as `body: null`). See `project_slack_interactive_form_payload_routing_gap`
///   for the multi-app interactive-payload limitation and the host-side
///   fix needed to lift it.
pub(crate) fn extract_api_app_id(input_json: &[u8]) -> Option<String> {
    extract_from_wrapper(input_json, |value| {
        value
            .get("api_app_id")
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
    fn returns_api_app_id_from_top_level_events_api_payload() {
        let payload = json!({
            "token": "verification-token",
            "team_id": "T123",
            "api_app_id": "A_LEGAL_BOT",
            "type": "event_callback",
            "event": { "type": "message", "user": "U1", "text": "hi" }
        });
        let bytes = serde_json::to_vec(&payload).unwrap();
        assert_eq!(extract_api_app_id(&bytes).as_deref(), Some("A_LEGAL_BOT"));
    }

    #[test]
    fn returns_api_app_id_from_m1_iid4d_wrapper() {
        let wrapper = json!({
            "headers": [
                { "name": "x-forwarded-for", "value": "203.0.113.42" }
            ],
            "body": {
                "team_id": "T123",
                "api_app_id": "A_ACCOUNTING_BOT",
                "type": "event_callback"
            }
        });
        let bytes = serde_json::to_vec(&wrapper).unwrap();
        assert_eq!(
            extract_api_app_id(&bytes).as_deref(),
            Some("A_ACCOUNTING_BOT")
        );
    }

    #[test]
    fn returns_none_for_url_verification_handshake() {
        // url_verification carries no api_app_id — falls through to
        // single-instance admit fallback at the host.
        let payload = json!({
            "type": "url_verification",
            "token": "tok",
            "challenge": "abc"
        });
        let bytes = serde_json::to_vec(&payload).unwrap();
        assert!(extract_api_app_id(&bytes).is_none());
    }

    #[test]
    fn returns_none_when_api_app_id_absent() {
        let payload = json!({ "team_id": "T123", "type": "event_callback" });
        let bytes = serde_json::to_vec(&payload).unwrap();
        assert!(extract_api_app_id(&bytes).is_none());
    }

    #[test]
    fn identify_hint_json_is_valid() {
        assert_valid_hint(IDENTIFY_HINT_JSON);
    }
}
