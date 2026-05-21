//! `render_plan` op for the Telegram provider.
//!
//! Telegram does not render Adaptive Cards natively, so the planner advertises
//! TierD capabilities (plain text with Markdown/HTML) and lets the shared
//! `render_plan_common` helper build the plan. The public entry point wraps the
//! inner implementation in `std::panic::catch_unwind` so that a bug in the
//! planner can be logged (via `stderr`) before the panic is re-raised to the
//! host runtime — matching the previous contract of `ops.rs`.

use provider_common::helpers::{RenderPlanConfig, render_plan_common};
use provider_common::telemetry::{self, Field, Level, field};

const PROVIDER_TYPE: &str = "telegram";

pub(crate) fn render_plan(input_json: &[u8]) -> Vec<u8> {
    match std::panic::catch_unwind(|| render_plan_inner(input_json)) {
        Ok(result) => result,
        Err(err) => {
            let detail = format!("{err:?}");
            telemetry::emit(
                Level::Error,
                PROVIDER_TYPE,
                "render_plan panic",
                &[Field {
                    key: field::ERROR,
                    value: &detail,
                }],
            );
            std::panic::resume_unwind(err);
        }
    }
}

fn render_plan_inner(input_json: &[u8]) -> Vec<u8> {
    // Capability matrix is centralized in provider-common.
    // See: provider_common::render::capabilities_for
    let capabilities = provider_common::render::capabilities_for("telegram")
        .expect("telegram capabilities must be registered");
    render_plan_common(
        input_json,
        &RenderPlanConfig {
            capabilities,
            default_summary: "telegram message",
        },
    )
}
