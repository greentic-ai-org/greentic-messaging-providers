//! Step 1 of the 3-step egress pipeline: render planning.
//!
//! Teams is a Tier A provider (native Adaptive Card). The capability matrix is
//! centralized in `greentic-messaging-renderer` via `capabilities_for("teams")`
//! — do not duplicate or override tier logic here.

use provider_common::helpers::{RenderPlanConfig, render_plan_common};

pub(crate) fn render_plan(input_json: &[u8]) -> Vec<u8> {
    // Capability matrix is centralized in greentic-messaging-renderer.
    // See: greentic_messaging_renderer::capabilities_for
    let capabilities = greentic_messaging_renderer::capabilities_for("teams")
        .expect("teams capabilities must be registered");
    render_plan_common(
        input_json,
        &RenderPlanConfig {
            capabilities,
            default_summary: "teams message",
        },
    )
}
