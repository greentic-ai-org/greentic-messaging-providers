//! Inbound Slack events (`ingest_http`).
//!
//! Handles:
//! - Slack URL verification (responds with the challenge string)
//! - Interactive payloads (`block_actions`) — either trigger a modal or
//!   produce a channel envelope
//! - `view_submission` (modal results) — delegates to [`super::modal`]
//! - Slack Events API (`event_callback`) and flat fallback payloads
//! - Drops Slack retries (`X-Slack-Retry-Num` header) to avoid duplicates
//! - Skips bot-authored messages to prevent echo loops

use base64::{Engine as _, engine::general_purpose::STANDARD};
use greentic_types::messaging::universal_dto::{HttpInV1, HttpOutV1};
use provider_common::http_compat::{http_out_error, http_out_v1_bytes, parse_operator_http_in};
use serde_json::{Value, json};

use super::build_slack_envelope;
use super::modal::{handle_view_submission, open_slack_modal};
use crate::bindings::greentic::http::http_client as client;
use crate::config::{ProviderConfig, load_config, resolve_bot_token};

pub(crate) fn ingest_http(input_json: &[u8]) -> Vec<u8> {
    // Try native greentic-types format first, fall back to operator format
    let request = match serde_json::from_slice::<HttpInV1>(input_json) {
        Ok(req) => req,
        Err(_) => match parse_operator_http_in(input_json) {
            Ok(req) => req,
            Err(err) => return http_out_error(400, &format!("invalid http input: {err}")),
        },
    };
    let body_bytes = match STANDARD.decode(&request.body_b64) {
        Ok(bytes) => bytes,
        Err(err) => return http_out_error(400, &format!("invalid body encoding: {err}")),
    };
    let body_val: Value = serde_json::from_slice(&body_bytes).unwrap_or(Value::Null);

    // Slack URL verification challenge — must respond with the challenge value.
    // Sent when setting Event Subscriptions or Interactivity Request URL.
    if body_val.get("type").and_then(Value::as_str) == Some("url_verification") {
        let challenge = body_val
            .get("challenge")
            .and_then(Value::as_str)
            .unwrap_or_default();
        let out = HttpOutV1 {
            status: 200,
            headers: vec![],
            body_b64: STANDARD.encode(challenge.as_bytes()),
            events: vec![],
        };
        return http_out_v1_bytes(&out);
    }

    // Slack interactive payloads (button clicks) come as URL-encoded `payload=<json>`
    // or directly as JSON with `type: "block_actions"`.
    // Also check for URL-encoded form body.
    let interactive_payload =
        if body_val.get("type").and_then(Value::as_str) == Some("block_actions") {
            Some(body_val.clone())
        } else {
            // Try URL-encoded: body may be "payload=%7B..." raw text.
            let body_str = String::from_utf8(body_bytes.clone()).unwrap_or_default();
            if let Some(payload) = body_str.strip_prefix("payload=") {
                let decoded = urldecode(payload);
                serde_json::from_str::<Value>(&decoded)
                    .ok()
                    .filter(|v| v.get("type").and_then(Value::as_str) == Some("block_actions"))
            } else {
                None
            }
        };

    // Handle view_submission — Slack modal form submitted.
    let view_submission_payload =
        if body_val.get("type").and_then(Value::as_str) == Some("view_submission") {
            Some(body_val.clone())
        } else {
            let body_str = String::from_utf8(body_bytes.clone()).unwrap_or_default();
            if let Some(payload) = body_str.strip_prefix("payload=") {
                let decoded = urldecode(payload);
                serde_json::from_str::<Value>(&decoded)
                    .ok()
                    .filter(|v| v.get("type").and_then(Value::as_str) == Some("view_submission"))
            } else {
                None
            }
        };

    if let Some(submission) = view_submission_payload {
        return handle_view_submission(&submission);
    }

    if let Some(interactive) = interactive_payload {
        return handle_block_actions(&interactive);
    }

    // Drop Slack retries — only process the first delivery.
    if is_slack_retry(&request) {
        let out = HttpOutV1 {
            status: 200,
            headers: Vec::new(),
            body_b64: STANDARD.encode(b"ok"),
            events: vec![],
        };
        return http_out_v1_bytes(&out);
    }

    // Slack Events API: {"type":"event_callback","event":{...}}
    // Legacy/generic:   {"body":{...}}
    // Flat:             {"text":"...","channel":"..."}
    let payload = body_val
        .get("event")
        .or_else(|| body_val.get("body"))
        .cloned()
        .unwrap_or_else(|| body_val.clone());

    // Skip bot messages to prevent echo loops.
    if is_bot_message(&payload) {
        let out = HttpOutV1 {
            status: 200,
            headers: Vec::new(),
            body_b64: STANDARD.encode(b"ok"),
            events: vec![],
        };
        return http_out_v1_bytes(&out);
    }

    let text = payload
        .get("text")
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_string();
    let channel = payload
        .get("channel")
        .and_then(Value::as_str)
        .map(|s| s.to_string());
    let sender = payload
        .get("user")
        .or_else(|| payload.get("user_id"))
        .and_then(Value::as_str)
        .map(|s| s.to_string());
    // Fetch user locale from Slack API (users.info) for i18n card translation.
    let slack_cfg = load_config(&body_val).ok();
    let user_locale = sender.as_deref().and_then(|uid| {
        slack_cfg
            .as_ref()
            .and_then(|c| fetch_slack_user_locale(c, uid))
    });
    let mut envelope = build_slack_envelope(text, channel.clone(), sender);
    if let Some(locale) = user_locale {
        envelope.metadata.insert("locale".to_string(), locale);
    }
    let normalized = json!({
        "ok": true,
        "event": body_val,
        "channel": channel,
    });
    let normalized_bytes = serde_json::to_vec(&normalized).unwrap_or_else(|_| b"{}".to_vec());
    let out = HttpOutV1 {
        status: 200,
        headers: Vec::new(),
        body_b64: STANDARD.encode(&normalized_bytes),
        events: vec![envelope],
    };
    http_out_v1_bytes(&out)
}

/// Process a Slack `block_actions` interaction (button click).
///
/// If the button is flagged as a modal trigger we open a Slack modal via
/// `views.open`; otherwise we produce a channel envelope carrying the
/// action's metadata for downstream routing.
fn handle_block_actions(interactive: &Value) -> Vec<u8> {
    let actions = interactive
        .get("actions")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    let channel = interactive
        .get("channel")
        .and_then(|v| v.get("id"))
        .and_then(Value::as_str)
        .map(|s| s.to_string());
    let sender = interactive
        .get("user")
        .and_then(|v| v.get("id"))
        .and_then(Value::as_str)
        .map(|s| s.to_string());

    // Extract routing info from first action's value.
    let first_action = actions.first().cloned().unwrap_or(Value::Null);
    let action_value_str = first_action
        .get("value")
        .and_then(Value::as_str)
        .unwrap_or_default();
    let action_id = first_action
        .get("action_id")
        .and_then(Value::as_str)
        .unwrap_or_default();

    // Check if this button triggers a modal (AC input fields present).
    let parsed_action_val = serde_json::from_str::<Value>(action_value_str).ok();
    let is_modal = parsed_action_val
        .as_ref()
        .and_then(|v| v.get("ac_modal"))
        .and_then(Value::as_bool)
        .unwrap_or(false);

    if is_modal {
        let trigger_id = interactive
            .get("trigger_id")
            .and_then(Value::as_str)
            .unwrap_or_default();
        if !trigger_id.is_empty() {
            // Retrieve input specs from message metadata (not button value).
            let msg_metadata_inputs = interactive
                .get("message")
                .and_then(|m| m.get("metadata"))
                .and_then(|m| m.get("event_payload"))
                .and_then(|p| p.get("inputs"))
                .cloned()
                .unwrap_or(json!([]));
            // Merge: action data + input specs for the modal builder.
            let mut modal_data = parsed_action_val.clone().unwrap_or(json!({}));
            if let Some(obj) = modal_data.as_object_mut() {
                obj.insert("ac_modal_inputs".into(), msg_metadata_inputs);
            }
            return open_slack_modal(trigger_id, &modal_data, channel.as_deref());
        }
    }

    // Try to parse action value as JSON for routeToCardId.
    let (_route_to_card, _card_id, action_text) =
        if let Ok(val) = serde_json::from_str::<Value>(action_value_str) {
            let rtc = val
                .get("routeToCardId")
                .and_then(|v| v.as_str())
                .unwrap_or_default()
                .to_string();
            let cid = val
                .get("cardId")
                .and_then(|v| v.as_str())
                .unwrap_or_default()
                .to_string();
            let text = if !rtc.is_empty() {
                format!("[card:{rtc}]")
            } else if !cid.is_empty() {
                format!("[action:{cid}]")
            } else {
                format!("[action:{action_id}]")
            };
            (rtc, cid, text)
        } else {
            (
                String::new(),
                String::new(),
                format!("[action:{action_id}]"),
            )
        };

    let mut envelope = build_slack_envelope(action_text, channel.clone(), sender);
    // Forward ALL Action.Submit data fields to metadata for MCP routing.
    if let Ok(val) = serde_json::from_str::<Value>(action_value_str)
        && let Some(obj) = val.as_object()
    {
        for (k, v) in obj {
            let s = match v {
                Value::String(s) => s.clone(),
                _ => v.to_string(),
            };
            envelope.metadata.insert(k.clone(), s);
        }
    }
    envelope
        .metadata
        .insert("slack.action_id".into(), action_id.to_string());
    envelope
        .metadata
        .insert("slack.action_value".into(), action_value_str.to_string());

    let normalized = json!({
        "ok": true,
        "event": interactive,
        "channel": channel,
    });
    let normalized_bytes = serde_json::to_vec(&normalized).unwrap_or_else(|_| b"{}".to_vec());
    let out = HttpOutV1 {
        status: 200,
        headers: Vec::new(),
        body_b64: STANDARD.encode(&normalized_bytes),
        events: vec![envelope],
    };
    http_out_v1_bytes(&out)
}

/// Fetch user locale from Slack `users.info` API.
fn fetch_slack_user_locale(cfg: &ProviderConfig, user_id: &str) -> Option<String> {
    let token = resolve_bot_token(cfg);
    if token.is_empty() {
        return None;
    }
    let api_base = cfg
        .api_base_url
        .as_deref()
        .unwrap_or("https://slack.com/api");
    let url = format!("{api_base}/users.info?user={user_id}");
    let request = client::Request {
        method: "GET".into(),
        url,
        headers: vec![("Authorization".into(), format!("Bearer {token}"))],
        body: None,
    };
    let resp = client::send(&request, None, None).ok()?;
    if resp.status < 200 || resp.status >= 300 {
        return None;
    }
    let body: Value = serde_json::from_slice(&resp.body.unwrap_or_default()).ok()?;
    body.get("user")
        .and_then(|u| u.get("locale"))
        .and_then(Value::as_str)
        .filter(|s| !s.is_empty())
        .map(|s| s.to_string())
}

/// Check if the request is a Slack retry (X-Slack-Retry-Num header present).
fn is_slack_retry(request: &HttpInV1) -> bool {
    request.headers.iter().any(|h| {
        h.name.eq_ignore_ascii_case("x-slack-retry-num")
            || h.name.eq_ignore_ascii_case("X-Slack-Retry-Num")
    })
}

/// Check if the event payload is from a bot (prevents echo loops).
fn is_bot_message(payload: &Value) -> bool {
    // bot_id field present on bot-authored messages
    if payload.get("bot_id").is_some_and(|v| !v.is_null()) {
        return true;
    }
    // subtype "bot_message" is another indicator
    if payload.get("subtype").and_then(Value::as_str) == Some("bot_message") {
        return true;
    }
    false
}

/// Simple percent-decode for URL-encoded strings.
pub(super) fn urldecode(input: &str) -> String {
    let mut result = String::with_capacity(input.len());
    let mut chars = input.chars();
    while let Some(ch) = chars.next() {
        if ch == '%' {
            let hex: String = chars.by_ref().take(2).collect();
            if let Ok(byte) = u8::from_str_radix(&hex, 16) {
                result.push(byte as char);
            } else {
                result.push('%');
                result.push_str(&hex);
            }
        } else if ch == '+' {
            result.push(' ');
        } else {
            result.push(ch);
        }
    }
    result
}
