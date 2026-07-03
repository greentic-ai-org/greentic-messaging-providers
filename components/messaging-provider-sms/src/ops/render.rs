use provider_common::helpers::render_plan_error;

/// SMS render planning (text-only, single segment) lands in a later task;
/// Task 1 only wires the op through as a stub.
pub(crate) fn render_plan(_input_json: &[u8]) -> Vec<u8> {
    render_plan_error("messaging-provider-sms render_plan not yet implemented")
}
