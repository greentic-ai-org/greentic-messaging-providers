//! `identify-instance` export — extracts the bot's `recipient.id` from a
//! Bot Framework Activity so the host can route the inbound to the right
//! `MessagingEndpoint` when multiple Teams bots share one runtime.

use serde_json::Value;

/// **Routing discriminator only — not an authentication check.**
///
/// `recipient.id` is read from the inbound Bot Framework Activity
/// body, which is caller-controlled. This function is intended to
/// answer "which `MessagingEndpoint` does this payload claim to be
/// for?" — not "is this payload authentic?". The host MUST verify
/// the Bot Framework JWT (signature, `aud`, expected bot identity)
/// against the routed endpoint's configured credentials before
/// admitting the request or emitting any events. Without that auth
/// gate downstream, an attacker who knows a bot's `recipient.id`
/// could choose the target endpoint by setting it in their payload.
///
/// The runner-host already validates the eid grammar at the routing
/// boundary; the missing piece — JWT enforcement, not decode-only —
/// is a separate hardening task on the Teams provider's existing
/// ingest path.
///
/// The discriminator for Teams is the activity's `recipient.id` field —
/// every Bot Service registration has a unique MSA user id that lands in
/// every inbound activity addressed to that bot. The input is accepted in
/// any shape the runtime currently delivers:
///
/// - the raw Bot Framework Activity at the top level (when the runtime
///   has already unwrapped the HTTP envelope), or
/// - an HttpInV1 / operator-format HTTP wrapper whose body is the
///   activity (base64 in `body_b64`, a JSON array of u8 numbers in
///   `body`, or an already-decoded JSON object in `body`).
pub(crate) fn extract_recipient_id(input_json: &[u8]) -> Option<String> {
    let value: Value = serde_json::from_slice(input_json).ok()?;
    recipient_id_from(&value).or_else(|| recipient_id_from_http_envelope(&value))
}

fn recipient_id_from(value: &Value) -> Option<String> {
    value
        .get("recipient")
        .and_then(|r| r.get("id"))
        .and_then(Value::as_str)
        .map(str::to_owned)
}

fn recipient_id_from_http_envelope(value: &Value) -> Option<String> {
    let body_value = decoded_body(value)?;
    recipient_id_from(&body_value)
}

fn decoded_body(value: &Value) -> Option<Value> {
    if let Some(body) = value.get("body").and_then(Value::as_object) {
        return Some(Value::Object(body.clone()));
    }
    if let Some(arr) = value.get("body").and_then(Value::as_array) {
        let bytes: Vec<u8> = arr
            .iter()
            .map(|v| u8::try_from(v.as_u64()?).ok())
            .collect::<Option<Vec<u8>>>()?;
        return serde_json::from_slice(&bytes).ok();
    }
    let b64 = value.get("body_b64").and_then(Value::as_str)?;
    let bytes = decode_b64(b64)?;
    serde_json::from_slice(&bytes).ok()
}

fn decode_b64(input: &str) -> Option<Vec<u8>> {
    use base64::Engine;
    base64::engine::general_purpose::STANDARD.decode(input).ok()
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
            "recipient": { "id": "28:bot-legal-msa", "name": "Legal Assistant" },
            "conversation": { "id": "conv-1" },
            "text": "hello"
        });
        let bytes = serde_json::to_vec(&payload).unwrap();
        assert_eq!(
            extract_recipient_id(&bytes).as_deref(),
            Some("28:bot-legal-msa")
        );
    }

    #[test]
    fn returns_recipient_id_from_invoke_activity() {
        let payload = json!({
            "type": "invoke",
            "name": "adaptiveCard/action",
            "recipient": { "id": "28:bot-accounting-msa" },
            "value": { "action": { "type": "Action.Submit" } }
        });
        let bytes = serde_json::to_vec(&payload).unwrap();
        assert_eq!(
            extract_recipient_id(&bytes).as_deref(),
            Some("28:bot-accounting-msa")
        );
    }

    #[test]
    fn returns_recipient_id_from_http_wrapper_with_body_b64() {
        let activity = json!({
            "type": "message",
            "recipient": { "id": "28:bot-via-http" }
        });
        let wrapper = json!({
            "headers": [],
            "body_b64": b64_body(&activity)
        });
        let bytes = serde_json::to_vec(&wrapper).unwrap();
        assert_eq!(
            extract_recipient_id(&bytes).as_deref(),
            Some("28:bot-via-http")
        );
    }

    #[test]
    fn returns_recipient_id_from_http_wrapper_with_decoded_body() {
        let wrapper = json!({
            "headers": [],
            "body": {
                "type": "message",
                "recipient": { "id": "28:bot-decoded" }
            }
        });
        let bytes = serde_json::to_vec(&wrapper).unwrap();
        assert_eq!(
            extract_recipient_id(&bytes).as_deref(),
            Some("28:bot-decoded")
        );
    }

    #[test]
    fn returns_recipient_id_from_http_wrapper_with_body_byte_array() {
        let activity = json!({
            "type": "message",
            "recipient": { "id": "28:bot-via-bytes" }
        });
        let bytes = serde_json::to_vec(&activity).unwrap();
        let wrapper = json!({
            "headers": [],
            "body": bytes.iter().map(|b| *b as u64).collect::<Vec<_>>()
        });
        let input = serde_json::to_vec(&wrapper).unwrap();
        assert_eq!(
            extract_recipient_id(&input).as_deref(),
            Some("28:bot-via-bytes")
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
    fn returns_none_when_http_body_b64_is_garbage() {
        let wrapper = json!({
            "headers": [],
            "body_b64": "@@@not-base64@@@"
        });
        let bytes = serde_json::to_vec(&wrapper).unwrap();
        assert!(extract_recipient_id(&bytes).is_none());
    }
}
