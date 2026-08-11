//! Approval rail delivery for Slack.
//!
//! `approval_request` turns a `greentic.approval.request.v1` body into a Slack
//! API call; the `block_actions` half of `ingest_http` turns the click back
//! into a `greentic.approval.response.v1` body, carried on the emitted
//! envelope's extensions for the caller to publish.
//!
//! Contract: `greentic-designer/docs/approval-rail-contract-v2.md`.

mod blocks;

use base64::{Engine as _, engine::general_purpose::STANDARD};
use greentic_types::ChannelMessageEnvelope;
use greentic_types::messaging::universal_dto::{Header, HttpOutV1, ProviderPayloadV1};
use provider_common::approval::{
    ApprovalRequest, ApprovalResponse, CORRELATION_ID_HEADER, Decision, DecisionToken,
    RESPONSE_SUBJECT,
};
use provider_common::helpers::{encode_error, json_bytes};
use provider_common::http_compat::http_out_v1_bytes;
use serde_json::{Value, json};
use std::collections::BTreeMap;

use super::build_slack_envelope;
use crate::DEFAULT_API_BASE;
use crate::bindings::greentic::http::http_client as client;
use crate::config::{load_config, parse_destination, resolve_bot_token};

use blocks::{ACTION_ID_APPROVE, ACTION_ID_DENY, approval_blocks, decision_ack, message_metadata};

/// Envelope extension key carrying the response the caller must publish.
pub(crate) const RESPONSE_EXTENSION_KEY: &str = "greentic.approval.response";

/// Build the Slack call that delivers (or updates) an approval request.
///
/// Input: `{correlation_id?, request, channel|to, message_ts?}`. A
/// `message_ts` means this correlation id has already been delivered, so the
/// existing message is updated with the new token rather than a second
/// approval being posted.
pub(crate) fn approval_request_op(input_json: &[u8]) -> Vec<u8> {
    let parsed: Value = match serde_json::from_slice(input_json) {
        Ok(value) => value,
        Err(err) => return encode_error(&format!("invalid json: {err}")),
    };

    let request_value = parsed.get("request").cloned().unwrap_or(parsed.clone());
    let request = ApprovalRequest::from_value(&request_value);
    if request.operation != "request" {
        return encode_error("approval request required");
    }

    let correlation_id = parsed
        .get("correlation_id")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or(request.target.as_str())
        .to_string();
    if correlation_id.is_empty() {
        return encode_error("correlation id required");
    }

    let cfg = load_config(&parsed).ok();
    let channel = parsed
        .get("channel")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
        .or_else(|| parse_destination(&parsed).map(|destination| destination.id))
        .or_else(|| cfg.as_ref().and_then(|cfg| cfg.default_channel.clone()));
    let channel = match channel {
        Some(channel) => channel,
        None => return encode_error("destination (to) required"),
    };

    let message_ts = parsed
        .get("message_ts")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty());
    let api_base = cfg
        .as_ref()
        .and_then(|cfg| cfg.api_base_url.clone())
        .unwrap_or_else(|| DEFAULT_API_BASE.to_string());

    let mut body = json!({
        "channel": channel,
        "text": notification_text(&request),
        "blocks": approval_blocks(&request, &correlation_id),
        "metadata": message_metadata(&correlation_id),
    });
    if let Some(ts) = message_ts
        && let Some(object) = body.as_object_mut()
    {
        object.insert("ts".into(), json!(ts));
    }

    let method = if message_ts.is_some() {
        "chat.update"
    } else {
        "chat.postMessage"
    };
    let mut metadata = BTreeMap::new();
    metadata.insert("url".to_string(), json!(format!("{api_base}/{method}")));
    metadata.insert("method".to_string(), json!("POST"));
    metadata.insert("channel".to_string(), json!(channel));

    let payload = ProviderPayloadV1 {
        content_type: "application/json".to_string(),
        body_b64: STANDARD.encode(serde_json::to_vec(&body).unwrap_or_else(|_| b"{}".to_vec())),
        metadata,
    };
    json_bytes(&json!({
        "ok": true,
        "update": message_ts.is_some(),
        "correlation_id": correlation_id,
        "payload": payload,
    }))
}

/// Is this `block_actions` payload an approval decision?
pub(crate) fn is_approval_interaction(interactive: &Value) -> bool {
    approval_action(interactive).is_some()
}

/// Answer the click: emit the response body on the envelope and replace the
/// request message so its token stops sitting in the channel.
pub(crate) fn handle_approval_interaction(interactive: &Value, host_input: &Value) -> Vec<u8> {
    let resolved_by = resolve_approver_email(interactive, host_input);
    match build_interaction(interactive, resolved_by) {
        Ok(interaction) => interaction.into_http_out(),
        Err(message) => {
            let out = HttpOutV1 {
                status: 200,
                headers: Vec::new(),
                body_b64: STANDARD.encode(message.as_bytes()),
                events: Vec::new(),
            };
            http_out_v1_bytes(&out)
        }
    }
}

struct Interaction {
    envelope: ChannelMessageEnvelope,
    ack: Value,
}

impl Interaction {
    fn into_http_out(self) -> Vec<u8> {
        let body = serde_json::to_vec(&self.ack).unwrap_or_else(|_| b"{}".to_vec());
        let out = HttpOutV1 {
            status: 200,
            headers: vec![Header {
                name: "Content-Type".to_string(),
                value: "application/json".to_string(),
            }],
            body_b64: STANDARD.encode(&body),
            events: vec![self.envelope],
        };
        http_out_v1_bytes(&out)
    }
}

fn build_interaction(
    interactive: &Value,
    resolved_by: Option<String>,
) -> Result<Interaction, String> {
    let (action_id, state) = approval_action(interactive).ok_or("not an approval interaction")?;
    let decision = match action_id {
        ACTION_ID_APPROVE => Decision::Approved,
        _ => Decision::Denied,
    };

    let correlation_id = state
        .get("cid")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or("approval state carries no correlation id")?
        .to_string();
    let token = state
        .get("tok")
        .and_then(Value::as_str)
        .and_then(DecisionToken::new);

    let response = ApprovalResponse::new(correlation_id.clone(), decision)
        .with_resolved_by(resolved_by)
        .with_token(token);

    let channel = interactive
        .get("channel")
        .and_then(|channel| channel.get("id"))
        .and_then(Value::as_str)
        .map(str::to_string);
    let slack_user_id = interactive
        .get("user")
        .and_then(|user| user.get("id"))
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string();

    let mut envelope = build_slack_envelope(
        format!("[approval:{}]", decision.as_wire()),
        channel,
        (!slack_user_id.is_empty()).then(|| slack_user_id.clone()),
    );
    envelope.correlation_id = Some(correlation_id.clone());
    envelope
        .metadata
        .insert("greentic.approval".into(), "true".into());
    envelope.metadata.insert(
        "greentic.approval.decision".into(),
        decision.as_wire().into(),
    );
    envelope.metadata.insert(
        "greentic.approval.correlation_id".into(),
        correlation_id.clone(),
    );
    envelope.metadata.insert(
        "greentic.approval.carries_token".into(),
        response.carries_token().to_string(),
    );
    envelope
        .metadata
        .insert("slack.action_id".into(), action_id.into());
    envelope.extensions.insert(
        RESPONSE_EXTENSION_KEY.to_string(),
        json!({
            "subject": RESPONSE_SUBJECT,
            "headers": {CORRELATION_ID_HEADER: correlation_id},
            "body": response.to_value(),
        }),
    );

    let title = message_title(interactive);
    Ok(Interaction {
        ack: decision_ack(title.as_deref(), decision, &slack_user_id),
        envelope,
    })
}

fn approval_action(interactive: &Value) -> Option<(&'static str, Value)> {
    interactive
        .get("actions")
        .and_then(Value::as_array)?
        .iter()
        .find_map(|action| {
            let action_id = match action.get("action_id").and_then(Value::as_str) {
                Some(ACTION_ID_APPROVE) => ACTION_ID_APPROVE,
                Some(ACTION_ID_DENY) => ACTION_ID_DENY,
                _ => return None,
            };
            let state = action
                .get("value")
                .and_then(Value::as_str)
                .and_then(|raw| serde_json::from_str::<Value>(raw).ok())
                .unwrap_or(Value::Null);
            Some((action_id, state))
        })
}

fn message_title(interactive: &Value) -> Option<String> {
    interactive
        .get("message")
        .and_then(|message| message.get("blocks"))
        .and_then(Value::as_array)?
        .iter()
        .find(|block| block["type"] == "section")
        .and_then(|block| block["text"]["text"].as_str())
        .map(str::to_string)
}

fn notification_text(request: &ApprovalRequest) -> String {
    match request.title() {
        Some(title) => format!("Approval needed: {}", blocks::escape_text(title)),
        None => "Approval needed".to_string(),
    }
}

/// `resolved_by` is an email; Slack hands us a workspace user id. The profile
/// lookup is best effort — an unnamed vote is refused by a policy-governed
/// gate, which is a better outcome than guessing an identity.
fn resolve_approver_email(interactive: &Value, host_input: &Value) -> Option<String> {
    if let Some(email) = interactive
        .get("user")
        .and_then(|user| user.get("profile"))
        .and_then(|profile| profile.get("email"))
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        return Some(email.to_string());
    }

    let user_id = interactive
        .get("user")
        .and_then(|user| user.get("id"))
        .and_then(Value::as_str)?;
    let cfg = load_config(host_input)
        .or_else(|_| load_config(interactive))
        .ok()?;
    let token = resolve_bot_token(&cfg);
    if token.is_empty() {
        return None;
    }
    let api_base = cfg.api_base_url.as_deref().unwrap_or(DEFAULT_API_BASE);
    let request = client::Request {
        method: "GET".into(),
        url: format!("{api_base}/users.info?user={user_id}"),
        headers: vec![("Authorization".into(), format!("Bearer {token}"))],
        body: None,
    };
    let response = client::send(&request, None, None).ok()?;
    if response.status < 200 || response.status >= 300 {
        return None;
    }
    let body: Value = serde_json::from_slice(&response.body.unwrap_or_default()).ok()?;
    body.get("user")
        .and_then(|user| user.get("profile"))
        .and_then(|profile| profile.get("email"))
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
}

#[cfg(test)]
mod tests;
