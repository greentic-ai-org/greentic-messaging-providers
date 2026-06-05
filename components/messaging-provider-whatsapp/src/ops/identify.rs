//! `identify-instance` export — extracts the receiving WhatsApp Cloud
//! API `phone_number_id` so the host can route the inbound to the right
//! `MessagingEndpoint` when multiple WhatsApp numbers share one runtime.

use provider_common::identify::extract_from_wrapper;
use serde_json::Value;

const PHONE_NUMBER_ID_POINTER: &str = "/entry/0/changes/0/value/metadata/phone_number_id";

/// JSON-encoded `IdentifyInstanceHint` returned from
/// `describe-identify-instance` — declares that the discriminator lives
/// at body path [`PHONE_NUMBER_ID_POINTER`], so the host can scope
/// per-provider header allowlists (WhatsApp needs no inbound headers for
/// identification).
pub(crate) const IDENTIFY_HINT_JSON: &[u8] = br#"{"version":1,"sources":[{"body_path":{"json_pointer":"/entry/0/changes/0/value/metadata/phone_number_id"}}]}"#;

/// **Routing discriminator only — not an authentication check.**
///
/// `phone_number_id` is read from the inbound WhatsApp Cloud API JSON
/// body at `entry[0].changes[0].value.metadata.phone_number_id`, which
/// is caller-controlled. Downstream auth — `X-Hub-Signature-256`
/// verification against the routed endpoint's app secret — MUST run
/// before any event is admitted.
///
/// Each WABA phone number maps 1:1 to a `MessagingEndpoint`.
pub(crate) fn extract_phone_number_id(input_json: &[u8]) -> Option<String> {
    extract_from_wrapper(input_json, |value| {
        value
            .pointer(PHONE_NUMBER_ID_POINTER)
            .and_then(Value::as_str)
            .map(str::to_owned)
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use provider_common::identify::test_utils::assert_valid_hint;
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

    #[test]
    fn returns_phone_number_id_from_top_level_cloud_api_payload() {
        let bytes = serde_json::to_vec(&cloud_api_body("PNID_LEGAL")).unwrap();
        assert_eq!(
            extract_phone_number_id(&bytes).as_deref(),
            Some("PNID_LEGAL")
        );
    }

    #[test]
    fn returns_phone_number_id_from_m1_iid4d_wrapper() {
        let wrapper = json!({
            "headers": [{ "name": "x-forwarded-for", "value": "203.0.113.42" }],
            "body": cloud_api_body("PNID_ACCOUNTING")
        });
        let bytes = serde_json::to_vec(&wrapper).unwrap();
        assert_eq!(
            extract_phone_number_id(&bytes).as_deref(),
            Some("PNID_ACCOUNTING")
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
    fn identify_hint_json_is_valid() {
        assert_valid_hint(IDENTIFY_HINT_JSON);
    }
}
