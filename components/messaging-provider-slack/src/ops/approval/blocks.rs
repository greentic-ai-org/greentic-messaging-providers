//! Block Kit rendering for an approval request.
//!
//! The decision token rides in the buttons' `value` — the opaque per-message
//! state Slack already provides — and nowhere else. It is never placed in a
//! URL, in the notification `text`, or in Slack message metadata.

use provider_common::approval::{ApprovalRequest, Decision, Routing, Tier};
use serde_json::{Value, json};

pub(super) const ACTION_ID_APPROVE: &str = "greentic_approval_approve";
pub(super) const ACTION_ID_DENY: &str = "greentic_approval_deny";
pub(super) const BLOCK_ID: &str = "greentic_approval";
pub(super) const METADATA_EVENT_TYPE: &str = "greentic_approval";
pub(super) const STATE_VERSION: i64 = 1;

const SECTION_TEXT_MAX: usize = 2900;
const APPROVERS_SHOWN: usize = 3;

/// Opaque per-message state carried on both buttons.
///
/// Only the correlation id and the token: the decision comes from the
/// `action_id`, so the two halves of the click cannot disagree.
pub(super) fn button_state(correlation_id: &str, token: Option<&str>) -> String {
    let mut state = json!({"v": STATE_VERSION, "cid": correlation_id});
    if let Some(token) = token
        && let Some(object) = state.as_object_mut()
    {
        object.insert("tok".into(), json!(token));
    }
    state.to_string()
}

pub(super) fn approval_blocks(request: &ApprovalRequest, correlation_id: &str) -> Vec<Value> {
    let token = request
        .decision_token()
        .map(provider_common::approval::DecisionToken::expose);
    let state = button_state(correlation_id, token);
    let title = request.title().unwrap_or("Approval required");

    let mut blocks = vec![
        json!({
            "type": "header",
            "text": {"type": "plain_text", "text": "Approval needed"}
        }),
        json!({
            "type": "section",
            "text": {"type": "plain_text", "text": truncate(title, SECTION_TEXT_MAX)}
        }),
    ];

    let fields = detail_fields(request);
    if !fields.is_empty() {
        blocks.push(json!({"type": "section", "fields": fields}));
    }
    if let Some(context) = approvers_context(request.routing.as_ref()) {
        blocks.push(context);
    }

    blocks.push(json!({
        "type": "actions",
        "block_id": BLOCK_ID,
        "elements": [
            decision_button(ACTION_ID_APPROVE, "Approve", "primary", title, &state),
            decision_button(ACTION_ID_DENY, "Reject", "danger", title, &state),
        ]
    }));
    blocks
}

/// Replaces the request message once someone has clicked, so the token stops
/// sitting in the channel and a second click has nothing to send.
pub(super) fn decision_ack(title: Option<&str>, decision: Decision, slack_user_id: &str) -> Value {
    let verb = match decision {
        Decision::Approved => "Approved",
        Decision::Denied => "Rejected",
        Decision::Timeout => "Timed out",
    };
    let subject = title.unwrap_or("Approval request");
    let attribution = if slack_user_id.is_empty() {
        String::new()
    } else {
        format!(" by <@{slack_user_id}>")
    };
    let summary = format!("{verb}{attribution}");

    json!({
        "replace_original": true,
        "text": format!("{summary}: {}", escape_text(&truncate(subject, SECTION_TEXT_MAX))),
        "blocks": [
            {
                "type": "section",
                "text": {"type": "plain_text", "text": truncate(subject, SECTION_TEXT_MAX)}
            },
            {
                "type": "context",
                "elements": [{"type": "mrkdwn", "text": summary}]
            }
        ]
    })
}

/// Message-level metadata ties a delivery back to its gate. Never the token —
/// message metadata is readable by every member of the channel.
pub(super) fn message_metadata(correlation_id: &str) -> Value {
    json!({
        "event_type": METADATA_EVENT_TYPE,
        "event_payload": {"correlation_id": correlation_id}
    })
}

fn decision_button(action_id: &str, label: &str, style: &str, title: &str, state: &str) -> Value {
    json!({
        "type": "button",
        "action_id": action_id,
        "style": style,
        "text": {"type": "plain_text", "text": label},
        "value": state,
        "confirm": {
            "title": {"type": "plain_text", "text": format!("{label} this request?")},
            "text": {"type": "plain_text", "text": truncate(title, 300)},
            "confirm": {"type": "plain_text", "text": label},
            "deny": {"type": "plain_text", "text": "Cancel"}
        }
    })
}

fn detail_fields(request: &ApprovalRequest) -> Vec<Value> {
    let mut fields = Vec::new();
    let routing = match request.routing.as_ref() {
        Some(routing) => routing,
        None => return fields,
    };
    if let Some(policy_id) = &routing.policy_id {
        fields.push(plain_field(format!("Policy: {policy_id}")));
    }
    if let Some(tier) = &routing.tier {
        fields.extend(tier_fields(tier));
    }
    if let Some(risk) = request.risk() {
        fields.push(plain_field(format!("Risk: {risk:.2}")));
    }
    if let Some(confidence) = request.confidence() {
        fields.push(plain_field(format!("Confidence: {confidence:.2}")));
    }
    fields
}

fn tier_fields(tier: &Tier) -> Vec<Value> {
    let mut fields = Vec::new();
    if let Some(chain) = tier.chain_display() {
        fields.push(plain_field(format!("Tier: {chain}")));
    }
    if let Some(min_approvals) = tier.min_approvals.filter(|value| *value > 1) {
        fields.push(plain_field(format!("Approvals needed: {min_approvals}")));
    }
    match tier.deadline_ms {
        Some(deadline_ms) => fields.push(plain_field(format!(
            "Escalates after: {}",
            humanize_ms(deadline_ms)
        ))),
        None => fields.push(plain_field("Escalates after: never".to_string())),
    }
    fields
}

fn approvers_context(routing: Option<&Routing>) -> Option<Value> {
    let approvers = routing?.approvers.as_ref()?;
    let mut parts = Vec::new();
    if let Some(role) = &approvers.role {
        parts.push(format!("Role: {role}"));
    }
    if approvers.has_explicit_list() {
        let shown = approvers
            .emails
            .iter()
            .take(APPROVERS_SHOWN)
            .cloned()
            .collect::<Vec<_>>()
            .join(", ");
        let remaining = approvers.emails.len().saturating_sub(APPROVERS_SHOWN);
        let listed = if remaining > 0 {
            format!("{shown} (+{remaining} more)")
        } else {
            shown
        };
        parts.push(format!("Approvers: {listed}"));
    }
    if parts.is_empty() {
        return None;
    }
    Some(json!({
        "type": "context",
        "elements": [{"type": "plain_text", "text": parts.join("  ·  ")}]
    }))
}

fn plain_field(text: String) -> Value {
    json!({"type": "plain_text", "text": truncate(&text, SECTION_TEXT_MAX)})
}

fn humanize_ms(ms: i64) -> String {
    if ms <= 0 {
        return "immediately".to_string();
    }
    let seconds = ms / 1000;
    match seconds {
        0..=59 => format!("{seconds}s"),
        60..=3599 => format!("{}m", seconds / 60),
        3600..=86399 => format!("{}h", seconds / 3600),
        _ => format!("{}d", seconds / 86400),
    }
}

fn truncate(text: &str, max: usize) -> String {
    text.chars().take(max).collect()
}

/// Slack parses the top-level `text` field as mrkdwn, so an author-supplied
/// title could otherwise smuggle `<!channel>` into a notification.
pub(super) fn escape_text(text: &str) -> String {
    text.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    const TOKEN: &str = "EXAMPLE-TOKEN-NOT-A-REAL-SECRET";

    fn request(routing: Value) -> ApprovalRequest {
        ApprovalRequest::from_value(&json!({
            "target": "default::run=RUN-1::node=gate",
            "operation": "request",
            "input": {"title": "Refund 1200 USD", "risk": 0.8},
            "routing": routing
        }))
    }

    fn full_request() -> ApprovalRequest {
        request(json!({
            "policy_id": "refunds",
            "tier": {"level": 1, "position": 0, "chain_len": 2, "min_approvals": 2, "deadline_ms": 3600000},
            "approvers": {"role": "admin", "emails": ["boss@acme.test"]},
            "channels": ["slack"],
            "decision_token": TOKEN
        }))
    }

    fn buttons(blocks: &[Value]) -> Vec<Value> {
        blocks
            .iter()
            .filter(|block| block["type"] == "actions")
            .flat_map(|block| {
                block["elements"]
                    .as_array()
                    .cloned()
                    .unwrap_or_default()
                    .into_iter()
            })
            .collect()
    }

    #[test]
    fn renders_the_tier_position_not_the_level() {
        let blocks = approval_blocks(&full_request(), "default::run=RUN-1::node=gate");
        let rendered = serde_json::to_string(&blocks).expect("blocks");
        assert!(rendered.contains("Tier: 1 of 2"));
        assert!(!rendered.contains("Tier: 2 of 2"));
        assert!(rendered.contains("Approvals needed: 2"));
        assert!(rendered.contains("Escalates after: 1h"));
    }

    #[test]
    fn an_unescalated_gate_renders_no_tier_line() {
        let request = request(json!({
            "tier": {"level": 1, "position": null, "chain_len": 2, "deadline_ms": null},
            "decision_token": TOKEN
        }));
        let rendered = serde_json::to_string(&approval_blocks(&request, "cid")).expect("blocks");
        assert!(!rendered.contains("Tier:"));
        assert!(rendered.contains("Escalates after: never"));
    }

    #[test]
    fn a_token_only_routing_block_still_renders_both_buttons() {
        let blocks = approval_blocks(&request(json!({"decision_token": TOKEN})), "cid");
        let buttons = buttons(&blocks);

        assert_eq!(buttons.len(), 2);
        assert_eq!(buttons[0]["action_id"], ACTION_ID_APPROVE);
        assert_eq!(buttons[1]["action_id"], ACTION_ID_DENY);
        for button in &buttons {
            assert!(button["value"].as_str().expect("state").contains(TOKEN));
        }
    }

    #[test]
    fn a_request_with_no_routing_at_all_still_renders_buttons_without_a_token() {
        let request = ApprovalRequest::from_value(&json!({
            "target": "t::run=R::node=n",
            "operation": "request",
            "input": {"title": "Legacy gate"}
        }));
        let buttons = buttons(&approval_blocks(&request, "t::run=R::node=n"));

        assert_eq!(buttons.len(), 2);
        for button in &buttons {
            let state: Value =
                serde_json::from_str(button["value"].as_str().expect("state")).expect("json");
            assert_eq!(state["cid"], "t::run=R::node=n");
            assert!(state.get("tok").is_none());
        }
    }

    #[test]
    fn the_token_is_confined_to_the_button_state() {
        let blocks = approval_blocks(&full_request(), "default::run=RUN-1::node=gate");

        for block in &blocks {
            if block["type"] == "actions" {
                continue;
            }
            assert!(
                !serde_json::to_string(block).expect("block").contains(TOKEN),
                "token leaked into a non-action block: {block}"
            );
        }
        assert!(
            !serde_json::to_string(&message_metadata("default::run=RUN-1::node=gate"))
                .expect("metadata")
                .contains(TOKEN)
        );
    }

    #[test]
    fn the_ack_carries_no_token_and_names_the_slack_clicker() {
        let ack = decision_ack(Some("Refund 1200 USD"), Decision::Approved, "U123");
        let rendered = serde_json::to_string(&ack).expect("ack");

        assert_eq!(ack["replace_original"], true);
        assert!(rendered.contains("Approved by <@U123>"));
        assert!(!rendered.contains(TOKEN));
    }

    #[test]
    fn an_author_supplied_title_cannot_smuggle_a_channel_ping() {
        let ack = decision_ack(Some("Refund <!channel> now"), Decision::Denied, "U123");
        let text = ack["text"].as_str().expect("text");

        assert!(!text.contains("<!channel>"));
        assert!(text.contains("&lt;!channel&gt;"));
        assert_eq!(escape_text("a & b < c > d"), "a &amp; b &lt; c &gt; d");
    }

    #[test]
    fn humanize_covers_each_bucket() {
        assert_eq!(humanize_ms(-1), "immediately");
        assert_eq!(humanize_ms(30_000), "30s");
        assert_eq!(humanize_ms(600_000), "10m");
        assert_eq!(humanize_ms(3_600_000), "1h");
        assert_eq!(humanize_ms(172_800_000), "2d");
    }
}
