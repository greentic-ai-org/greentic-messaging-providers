//! Step 1 of the egress pipeline: render planning.
//!
//! Webex is a TierB provider (native Adaptive Card attachments with a text
//! fallback). The capability matrix is centralised in
//! `provider-common` so this module is a thin wrapper over the
//! common helper.

use provider_common::helpers::{RenderPlanConfig, render_plan_common};

pub(crate) fn render_plan(input_json: &[u8]) -> Vec<u8> {
    // Capability matrix is centralized in provider-common.
    // See: provider_common::render::capabilities_for
    let capabilities = provider_common::render::capabilities_for("webex")
        .expect("webex capabilities must be registered");
    render_plan_common(
        input_json,
        &RenderPlanConfig {
            capabilities,
            default_summary: "webex message",
        },
    )
}
