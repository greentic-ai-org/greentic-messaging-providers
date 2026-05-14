//! Step 1 of the egress pipeline: `render_plan`.
//!
//! Slack cannot render Adaptive Cards natively, so the planner downsamples
//! to plain text (TierD) with a modest markdown allowance.

use provider_common::helpers::{RenderPlanConfig, render_plan_common};

pub(crate) fn render_plan(input_json: &[u8]) -> Vec<u8> {
    // Capability matrix is centralized in provider-common.
    // See: provider_common::render::capabilities_for
    let capabilities = provider_common::render::capabilities_for("slack")
        .expect("slack capabilities must be registered");
    render_plan_common(
        input_json,
        &RenderPlanConfig {
            capabilities,
            default_summary: "slack message",
        },
    )
}
