use provider_common::helpers::encode_error;

/// Encoding the universal payload into a Twilio send request lands in a
/// later task; Task 1 only wires the op through as a stub.
pub(crate) fn encode_op(_input_json: &[u8]) -> Vec<u8> {
    encode_error("messaging-provider-sms encode not yet implemented")
}
