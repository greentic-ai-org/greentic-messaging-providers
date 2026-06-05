//! `identify-instance` export — extracts the receiving Webex bot's
//! `appId` from a webhook event so the host can route the inbound to
//! the right `MessagingEndpoint` when multiple Webex bots share one
//! runtime.

use provider_common::identify::extract_from_wrapper;
use serde_json::Value;

/// JSON-encoded `IdentifyInstanceHint` returned from
/// `describe-identify-instance` — declares that the discriminator lives
/// at body path `/appId`, so the host can scope per-provider header
/// allowlists (Webex needs no inbound headers for identification).
pub(crate) const IDENTIFY_HINT_JSON: &[u8] =
    br#"{"version":1,"sources":[{"body_path":{"json_pointer":"/appId"}}]}"#;

/// **Routing discriminator only — not an authentication check.**
///
/// The top-level `appId` is the receiving bot's application id (stable
/// per registered bot). The `data.appId` nested field is the actor's
/// appId on some events — the routing discriminator is always the
/// top-level one. Downstream auth — `x-spark-signature` HMAC
/// verification against the routed endpoint's webhook secret — MUST
/// run before any event is admitted.
pub(crate) fn extract_app_id(input_json: &[u8]) -> Option<String> {
    extract_from_wrapper(input_json, |value| {
        value
            .get("appId")
            .and_then(Value::as_str)
            .map(str::to_owned)
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use provider_common::identify::test_utils::assert_valid_hint;
    use serde_json::json;

    fn webhook_event(app_id: &str) -> Value {
        json!({
            "id": "Y2lzY29zcGFyazovL3VzL1dFQkhPT0sv...",
            "name": "bot-legal",
            "targetUrl": "https://example.com/webhook",
            "resource": "messages",
            "event": "created",
            "appId": app_id,
            "actorId": "Y2lzY29zcGFyazovL3VzL1BFT1BMRS8...",
            "data": {
                "id": "Y2lzY29zcGFyazovL3VzL01FU1NBR0Uv...",
                "roomId": "Y2lzY29zcGFyazovL3VzL1JPT00v..."
            }
        })
    }

    #[test]
    fn returns_app_id_from_top_level_webhook_payload() {
        let bytes = serde_json::to_vec(&webhook_event("APP_LEGAL")).unwrap();
        assert_eq!(extract_app_id(&bytes).as_deref(), Some("APP_LEGAL"));
    }

    #[test]
    fn returns_app_id_from_m1_iid4d_wrapper() {
        let wrapper = json!({
            "headers": [{ "name": "x-forwarded-for", "value": "203.0.113.42" }],
            "body": webhook_event("APP_ACCOUNTING")
        });
        let bytes = serde_json::to_vec(&wrapper).unwrap();
        assert_eq!(extract_app_id(&bytes).as_deref(), Some("APP_ACCOUNTING"));
    }

    #[test]
    fn prefers_top_level_app_id_over_data_app_id() {
        // `data.appId` is the actor/source's appId on some Webex events; the
        // bot-id discriminator lives at the top level.
        let payload = json!({
            "appId": "APP_TOP",
            "data": { "appId": "APP_NESTED" }
        });
        let bytes = serde_json::to_vec(&payload).unwrap();
        assert_eq!(extract_app_id(&bytes).as_deref(), Some("APP_TOP"));
    }

    #[test]
    fn returns_none_when_app_id_absent() {
        let payload = json!({ "resource": "messages", "event": "created" });
        let bytes = serde_json::to_vec(&payload).unwrap();
        assert!(extract_app_id(&bytes).is_none());
    }

    #[test]
    fn identify_hint_json_is_valid() {
        assert_valid_hint(IDENTIFY_HINT_JSON);
    }
}
