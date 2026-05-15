//! Step 1 of the egress pipeline: render planning.
//!
//! Webchat is a Tier A provider (native Adaptive Card support). The capability
//! matrix is centralized in `provider-common` — do not duplicate it
//! here.

use provider_common::helpers::{RenderPlanConfig, render_plan_common};

pub(crate) fn render_plan(input_json: &[u8]) -> Vec<u8> {
    // Capability matrix is centralized in provider-common.
    // See: provider_common::render::capabilities_for
    let capabilities = provider_common::render::capabilities_for("webchat")
        .expect("webchat capabilities must be registered");
    render_plan_common(
        input_json,
        &RenderPlanConfig {
            capabilities,
            default_summary: "webchat message",
        },
    )
}
