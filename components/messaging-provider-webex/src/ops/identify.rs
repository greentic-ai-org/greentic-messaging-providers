//! `identify-instance` export — extracts the receiving Webex bot's
//! `appId` from a webhook event so the host can route the inbound to
//! the right `MessagingEndpoint` when multiple Webex bots share one
//! runtime.

use serde_json::Value;

/// JSON-encoded `IdentifyInstanceHint` returned from
/// `describe-identify-instance` — declares that the discriminator lives
/// at body path `/appId`, so the host can scope per-provider header
/// allowlists (Webex needs no inbound headers for identification).
pub(crate) const IDENTIFY_HINT_JSON: &[u8] =
    br#"{"version":1,"sources":[{"body_path":{"json_pointer":"/appId"}}]}"#;

/// **Routing discriminator only — not an authentication check.**
///
/// `appId` is read from the inbound Webex webhook JSON body, which is
/// caller-controlled. Downstream auth — `x-spark-signature` HMAC
/// verification against the routed endpoint's webhook secret — MUST
/// run before any event is admitted.
///
/// The discriminator for Webex is the top-level `appId` field on the
/// webhook event (the receiving bot's application id, stable per
/// registered bot). The input is accepted in any shape the runtime
/// currently delivers:
///
/// - the raw Webex webhook JSON body at the top level (legacy bare
///   body), or
/// - an M1 IID.4d wrapper whose `body` is the webhook JSON
///   (already-decoded object, JSON array of u8 bytes, or absent with
///   base64 in `body_b64`).
pub(crate) fn extract_app_id(input_json: &[u8]) -> Option<String> {
    let value: Value = serde_json::from_slice(input_json).ok()?;
    if let Some(id) = app_id_from(&value) {
        return Some(id);
    }
    if let Some(body) = value.get("body").filter(|b| b.is_object()) {
        return app_id_from(body);
    }
    let bytes = http_body_bytes(&value)?;
    let parsed: Value = serde_json::from_slice(&bytes).ok()?;
    app_id_from(&parsed)
}

fn app_id_from(value: &Value) -> Option<String> {
    value
        .get("appId")
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

    fn b64_body(body: &Value) -> String {
        base64::engine::general_purpose::STANDARD.encode(serde_json::to_vec(body).unwrap())
    }

    #[test]
    fn returns_app_id_from_top_level_webhook_payload() {
        let bytes = serde_json::to_vec(&webhook_event("APP_LEGAL")).unwrap();
        assert_eq!(extract_app_id(&bytes).as_deref(), Some("APP_LEGAL"));
    }

    #[test]
    fn returns_app_id_from_m1_iid4d_wrapper_with_decoded_body() {
        let wrapper = json!({
            "headers": [
                { "name": "x-forwarded-for", "value": "203.0.113.42" }
            ],
            "body": webhook_event("APP_ACCOUNTING")
        });
        let bytes = serde_json::to_vec(&wrapper).unwrap();
        assert_eq!(extract_app_id(&bytes).as_deref(), Some("APP_ACCOUNTING"));
    }

    #[test]
    fn returns_app_id_from_wrapper_with_body_b64() {
        let wrapper = json!({
            "headers": [],
            "body_b64": b64_body(&webhook_event("APP_VIA_B64"))
        });
        let bytes = serde_json::to_vec(&wrapper).unwrap();
        assert_eq!(extract_app_id(&bytes).as_deref(), Some("APP_VIA_B64"));
    }

    #[test]
    fn returns_app_id_from_wrapper_with_body_byte_array() {
        let body_bytes = serde_json::to_vec(&webhook_event("APP_VIA_BYTES")).unwrap();
        let wrapper = json!({
            "headers": [],
            "body": body_bytes.iter().map(|b| *b as u64).collect::<Vec<_>>()
        });
        let input = serde_json::to_vec(&wrapper).unwrap();
        assert_eq!(extract_app_id(&input).as_deref(), Some("APP_VIA_BYTES"));
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
    fn returns_none_for_unparseable_input() {
        assert!(extract_app_id(b"not json").is_none());
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
