use provider_common::helpers::json_bytes;
use serde_json::json;

/// v1 is a documented no-op: operators register the inbound webhook URL in
/// the Twilio console. Auto-registration is deferred (see design spec §3.1).
pub(crate) fn setup_webhook(_input_json: &[u8]) -> Vec<u8> {
    json_bytes(&json!({
        "ok": true,
        "status": "not_supported",
        "message": "configure the inbound webhook URL in the Twilio console"
    }))
}
