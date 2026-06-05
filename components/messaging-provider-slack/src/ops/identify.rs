//! `identify-instance` export — extracts the receiving Slack app's
//! `api_app_id` from an Events API payload so the host can route the
//! inbound to the right `MessagingEndpoint` when multiple Slack apps
//! share one runtime.

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
/// The discriminator for Slack is the `api_app_id` field (the Slack
/// app's unique id, set on every Events API delivery and on every
/// interactive payload). The input is accepted in any shape the runtime
/// currently delivers:
///
/// - the raw Slack JSON body at the top level (legacy bare body), or
/// - an HttpInV1 / M1 IID.4d wrapper whose `body` is the Slack JSON
///   (already-decoded object, JSON array of u8 bytes, or absent with
///   base64 in `body_b64`).
///
/// Returns `None` for payloads without `api_app_id`:
/// - `url_verification` (challenge handshake),
/// - URL-encoded `payload=<json>` interactive form bodies (the host
///   parses the body as JSON before wrapping; non-JSON bodies arrive
///   as `body: null`).
pub(crate) fn extract_api_app_id(input_json: &[u8]) -> Option<String> {
    let value: Value = serde_json::from_slice(input_json).ok()?;
    if let Some(id) = api_app_id_from(&value) {
        return Some(id);
    }
    if let Some(body) = value.get("body").filter(|b| b.is_object()) {
        return api_app_id_from(body);
    }
    let bytes = http_body_bytes(&value)?;
    let parsed: Value = serde_json::from_slice(&bytes).ok()?;
    api_app_id_from(&parsed)
}

fn api_app_id_from(value: &Value) -> Option<String> {
    value
        .get("api_app_id")
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

    fn b64_body(body: &Value) -> String {
        base64::engine::general_purpose::STANDARD.encode(serde_json::to_vec(body).unwrap())
    }

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
    fn returns_api_app_id_from_m1_iid4d_wrapper_with_decoded_body() {
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
    fn returns_api_app_id_from_wrapper_with_body_b64() {
        let body = json!({
            "team_id": "T123",
            "api_app_id": "A_VIA_B64"
        });
        let wrapper = json!({
            "headers": [],
            "body_b64": b64_body(&body)
        });
        let bytes = serde_json::to_vec(&wrapper).unwrap();
        assert_eq!(extract_api_app_id(&bytes).as_deref(), Some("A_VIA_B64"));
    }

    #[test]
    fn returns_api_app_id_from_wrapper_with_body_byte_array() {
        let body = json!({ "api_app_id": "A_VIA_BYTES" });
        let bytes = serde_json::to_vec(&body).unwrap();
        let wrapper = json!({
            "headers": [],
            "body": bytes.iter().map(|b| *b as u64).collect::<Vec<_>>()
        });
        let input = serde_json::to_vec(&wrapper).unwrap();
        assert_eq!(extract_api_app_id(&input).as_deref(), Some("A_VIA_BYTES"));
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
    fn returns_none_for_unparseable_input() {
        assert!(extract_api_app_id(b"not json").is_none());
    }

    #[test]
    fn returns_none_when_body_b64_is_garbage() {
        let wrapper = json!({
            "headers": [],
            "body_b64": "@@@not-base64@@@"
        });
        let bytes = serde_json::to_vec(&wrapper).unwrap();
        assert!(extract_api_app_id(&bytes).is_none());
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
