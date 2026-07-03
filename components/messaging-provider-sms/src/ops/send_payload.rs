use provider_common::helpers::send_payload_error;

/// The Twilio Messages API send (`https://api.twilio.com/.../Messages.json`)
/// lands in a later task; Task 1 only wires the op through as a stub.
pub(crate) fn send_payload(_input_json: &[u8]) -> Vec<u8> {
    send_payload_error(
        "messaging-provider-sms send_payload not yet implemented",
        false,
    )
}
