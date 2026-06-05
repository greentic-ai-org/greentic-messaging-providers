//! `identify-instance` export — extracts the receiving WhatsApp Cloud
//! API `phone_number_id` so the host can route the inbound to the right
//! `MessagingEndpoint` when multiple WhatsApp numbers share one runtime.

use serde_json::Value;

/// JSON-encoded `IdentifyInstanceHint` returned from
/// `describe-identify-instance` — declares that the discriminator lives
/// at body path `/entry/0/changes/0/value/metadata/phone_number_id`, so
/// the host can scope per-provider header allowlists (WhatsApp needs no
/// inbound headers for identification).
pub(crate) const IDENTIFY_HINT_JSON: &[u8] = br#"{"version":1,"sources":[{"body_path":{"json_pointer":"/entry/0/changes/0/value/metadata/phone_number_id"}}]}"#;

/// **Routing discriminator only — not an authentication check.**
///
/// `phone_number_id` is read from the inbound WhatsApp Cloud API JSON
/// body at `entry[0].changes[0].value.metadata.phone_number_id`, which
/// is caller-controlled. Downstream auth — `X-Hub-Signature-256`
/// verification against the routed endpoint's app secret — MUST run
/// before any event is admitted.
///
/// The discriminator is the WhatsApp Business Cloud API
/// `phone_number_id` — each WABA phone number maps 1:1 to a
/// `MessagingEndpoint`. The input is accepted in any shape the runtime
/// currently delivers:
///
/// - the raw Cloud API JSON body at the top level (legacy bare body), or
/// - an M1 IID.4d wrapper whose `body` is the Cloud API JSON
///   (already-decoded object, JSON array of u8 bytes, or absent with
///   base64 in `body_b64`).
pub(crate) fn extract_phone_number_id(input_json: &[u8]) -> Option<String> {
    let value: Value = serde_json::from_slice(input_json).ok()?;
    if let Some(id) = phone_number_id_from(&value) {
        return Some(id);
    }
    if let Some(body) = value.get("body").filter(|b| b.is_object()) {
        return phone_number_id_from(body);
    }
    let bytes = http_body_bytes(&value)?;
    let parsed: Value = serde_json::from_slice(&bytes).ok()?;
    phone_number_id_from(&parsed)
}

fn phone_number_id_from(value: &Value) -> Option<String> {
    value
        .get("entry")
        .and_then(Value::as_array)
        .and_then(|arr| arr.first())
        .and_then(|entry| entry.get("changes"))
        .and_then(Value::as_array)
        .and_then(|arr| arr.first())
        .and_then(|change| change.get("value"))
        .and_then(|v| v.get("metadata"))
        .and_then(|m| m.get("phone_number_id"))
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

    fn cloud_api_body(pnid: &str) -> Value {
        json!({
            "object": "whatsapp_business_account",
            "entry": [{
                "id": "WABA-1",
                "changes": [{
                    "value": {
                        "messaging_product": "whatsapp",
                        "metadata": {
                            "display_phone_number": "+15551234567",
                            "phone_number_id": pnid
                        },
                        "messages": [{ "from": "+1...", "id": "wamid-1", "text": { "body": "hi" } }]
                    },
                    "field": "messages"
                }]
            }]
        })
    }

    fn b64_body(body: &Value) -> String {
        base64::engine::general_purpose::STANDARD.encode(serde_json::to_vec(body).unwrap())
    }

    #[test]
    fn returns_phone_number_id_from_top_level_cloud_api_payload() {
        let bytes = serde_json::to_vec(&cloud_api_body("PNID_LEGAL")).unwrap();
        assert_eq!(
            extract_phone_number_id(&bytes).as_deref(),
            Some("PNID_LEGAL")
        );
    }

    #[test]
    fn returns_phone_number_id_from_m1_iid4d_wrapper_with_decoded_body() {
        let wrapper = json!({
            "headers": [
                { "name": "x-forwarded-for", "value": "203.0.113.42" }
            ],
            "body": cloud_api_body("PNID_ACCOUNTING")
        });
        let bytes = serde_json::to_vec(&wrapper).unwrap();
        assert_eq!(
            extract_phone_number_id(&bytes).as_deref(),
            Some("PNID_ACCOUNTING")
        );
    }

    #[test]
    fn returns_phone_number_id_from_wrapper_with_body_b64() {
        let wrapper = json!({
            "headers": [],
            "body_b64": b64_body(&cloud_api_body("PNID_VIA_B64"))
        });
        let bytes = serde_json::to_vec(&wrapper).unwrap();
        assert_eq!(
            extract_phone_number_id(&bytes).as_deref(),
            Some("PNID_VIA_B64")
        );
    }

    #[test]
    fn returns_phone_number_id_from_wrapper_with_body_byte_array() {
        let body_bytes = serde_json::to_vec(&cloud_api_body("PNID_VIA_BYTES")).unwrap();
        let wrapper = json!({
            "headers": [],
            "body": body_bytes.iter().map(|b| *b as u64).collect::<Vec<_>>()
        });
        let input = serde_json::to_vec(&wrapper).unwrap();
        assert_eq!(
            extract_phone_number_id(&input).as_deref(),
            Some("PNID_VIA_BYTES")
        );
    }

    #[test]
    fn returns_none_when_entry_array_is_empty() {
        let payload = json!({ "object": "whatsapp_business_account", "entry": [] });
        let bytes = serde_json::to_vec(&payload).unwrap();
        assert!(extract_phone_number_id(&bytes).is_none());
    }

    #[test]
    fn returns_none_when_metadata_absent() {
        let payload = json!({
            "entry": [{ "changes": [{ "value": { "messaging_product": "whatsapp" } }] }]
        });
        let bytes = serde_json::to_vec(&payload).unwrap();
        assert!(extract_phone_number_id(&bytes).is_none());
    }

    #[test]
    fn returns_none_for_unparseable_input() {
        assert!(extract_phone_number_id(b"not json").is_none());
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
