//! Adaptive Card → Slack Block Kit converter.
//!
//! Maps AC elements to their best Slack-native representation:
//! - TextBlock (bold/heading) → `header` block (max 150 chars)
//! - TextBlock (normal) → `section` block with mrkdwn
//! - RichTextBlock → `section` with mrkdwn formatting
//! - Image/ImageSet → `image` block
//! - FactSet → `section` with `fields` array
//! - ColumnSet → `section` with `fields`
//! - Container → recursive processing
//! - ActionSet + top-level actions → `actions` block with buttons
//! - Table → `section` with preformatted code block
//! - Input.Text → collected for Slack modal (opened on Action.Submit click)
//!
//! The heavy lifting lives in submodules:
//! - `elements`  — the big `ac_element_to_blocks` dispatcher
//! - `actions`   — Slack button collection from AC actions / selectAction
//! - `inputs`    — AC input field collection + modal metadata injection
//! - `markdown`  — AC markdown → Slack mrkdwn conversion + text extraction
//!
//! The [`SlackBlockKitConverter`] marker type exposes this converter through
//! the generic [`provider_common::AdaptiveCardConverter`] trait so the egress
//! pipeline can stay provider-agnostic in future refactors. Current call sites
//! still use the free [`ac_to_slack_blocks`] function directly; the trait
//! impl is an additive entry point.

mod actions;
mod elements;
mod inputs;
mod markdown;

use serde_json::{Value, json};

use actions::collect_slack_actions;
use elements::ac_element_to_blocks;
use inputs::{collect_ac_input_fields, inject_modal_metadata};

/// Result of converting an AC card to Slack blocks.
pub(crate) struct SlackBlocksResult {
    pub blocks: Vec<Value>,
    /// Input field specs for modal rendering (empty if no inputs).
    pub modal_inputs: Vec<Value>,
}

/// Convert an Adaptive Card JSON string into Slack Block Kit blocks.
pub(crate) fn ac_to_slack_blocks(ac_raw: &str) -> Option<SlackBlocksResult> {
    let ac: Value = serde_json::from_str(ac_raw).ok()?;
    let body = ac.get("body").and_then(Value::as_array);
    let top_actions = ac.get("actions").and_then(Value::as_array);

    let mut blocks: Vec<Value> = Vec::new();
    let mut actions: Vec<Value> = Vec::new();

    // Collect input fields (Input.Text + Input.ChoiceSet) for modal support.
    let mut input_fields: Vec<Value> = Vec::new();
    if let Some(body) = body {
        collect_ac_input_fields(body, &mut input_fields);
    }
    let has_modal = !input_fields.is_empty();

    if let Some(body) = body {
        for element in body {
            ac_element_to_blocks(element, &mut blocks, &mut actions, has_modal);
        }
    }
    if let Some(top_actions) = top_actions {
        collect_slack_actions(top_actions, &mut actions);
    }

    // If there are input fields, mark Action.Submit buttons as modal triggers.
    // Input specs are NOT embedded in button value (too large) — they go in message metadata.
    if has_modal {
        inject_modal_metadata(&mut actions);
    }

    // Add actions block if any buttons were collected.
    if !actions.is_empty() {
        // Slack max 25 elements per actions block.
        let capped: Vec<Value> = actions.into_iter().take(25).collect();
        blocks.push(json!({
            "type": "actions",
            "elements": capped
        }));
    }

    if blocks.is_empty() {
        return None;
    }

    // Slack max 50 blocks per message.
    blocks.truncate(50);
    Some(SlackBlocksResult {
        blocks,
        modal_inputs: input_fields,
    })
}

/// Marker type implementing [`provider_common::AdaptiveCardConverter`] for
/// Slack Block Kit. Delegates to the existing pure [`ac_to_slack_blocks`]
/// helper so call sites and unit tests can share a single conversion path.
///
/// This is an additive entry point — current internal call sites still invoke
/// [`ac_to_slack_blocks`] directly. Migration to `converter.convert(...)` will
/// happen in a follow-up pass once the pipeline is generic over the trait.
#[allow(dead_code)] // additive trait entry point; non-test call sites migrate later
pub(crate) struct SlackBlockKitConverter;

impl provider_common::AdaptiveCardConverter for SlackBlockKitConverter {
    type Output = SlackBlocksResult;

    fn convert(
        &self,
        adaptive_card: &Value,
        _caps: &provider_common::render::PlannerCapabilities,
    ) -> Result<Self::Output, provider_common::ProviderError> {
        // `ac_to_slack_blocks` currently takes a raw JSON string; adapt the
        // trait's `&Value` input by re-serialising. Cheap: the card is small
        // and this keeps the legacy helper untouched.
        let ac_raw = serde_json::to_string(adaptive_card).map_err(|err| {
            provider_common::ProviderError::Validation(format!(
                "adaptive card is not serialisable JSON: {err}"
            ))
        })?;
        ac_to_slack_blocks(&ac_raw).ok_or_else(|| {
            provider_common::ProviderError::Validation(
                "adaptive card produced empty Slack Block Kit payload".to_string(),
            )
        })
    }

    fn provider_name(&self) -> &'static str {
        "slack"
    }
}

#[cfg(test)]
mod converter_tests {
    use super::*;
    use provider_common::AdaptiveCardConverter;

    #[test]
    fn converter_handles_simple_card() {
        let card = serde_json::json!({
            "type": "AdaptiveCard",
            "version": "1.6",
            "body": [{"type": "TextBlock", "text": "hello"}]
        });
        let caps = provider_common::render::capabilities_for("slack")
            .expect("slack capabilities must be registered");
        let result = SlackBlockKitConverter.convert(&card, &caps);
        assert!(result.is_ok(), "converter should succeed on a simple card");
        let blocks = result.unwrap();
        assert!(
            !blocks.blocks.is_empty(),
            "simple card should produce at least one block"
        );
    }

    #[test]
    fn converter_provider_name() {
        assert_eq!(SlackBlockKitConverter.provider_name(), "slack");
    }

    #[test]
    fn converter_rejects_empty_card() {
        let card = serde_json::json!({
            "type": "AdaptiveCard",
            "version": "1.6",
            "body": []
        });
        let caps = provider_common::render::capabilities_for("slack")
            .expect("slack capabilities must be registered");
        let result = SlackBlockKitConverter.convert(&card, &caps);
        assert!(
            matches!(result, Err(provider_common::ProviderError::Validation(_))),
            "empty body should yield a validation error",
        );
    }
}
