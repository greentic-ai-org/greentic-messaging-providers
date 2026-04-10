//! Webex provider operations.
//!
//! The egress pipeline and supporting logic is split into focused submodules
//! so each file stays under the project's 500-line cap:
//!
//! - `render`  — `render_plan` (step 1 of the 3-step egress pipeline)
//! - `encode`  — `encode_op`   (step 2)
//! - `send`    — `send_payload` + legacy `handle_send` / `handle_reply`
//! - `ingest`  — `ingest_http` + webhook event normalisation
//! - `webhook` — `setup_webhook` helper (manages Webex webhook registrations)
//!
//! Shared low-level helpers (body construction, error formatting, card text
//! summarisation) live directly in this file because they are used by both the
//! `send` and `ingest` paths.

mod encode;
mod ingest;
mod ingest_helpers;
mod render;
mod send;
mod send_payload;
mod webhook;

pub(crate) use encode::encode_op;
pub(crate) use ingest::ingest_http;
pub(crate) use render::render_plan;
pub(crate) use send::{handle_reply, handle_send};
pub(crate) use send_payload::send_payload;
pub(crate) use webhook::setup_webhook;

use serde_json::Value;

/// Extract a human-readable summary from an Adaptive Card payload.
///
/// Used by both the legacy `handle_send` path and the `send_payload` egress
/// step to derive `markdown` text when the envelope only carries a card.
pub(super) fn summarize_card_text(card: &Value) -> Option<String> {
    if let Some(text) = card
        .get("text")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|t| !t.is_empty())
    {
        return Some(text.to_string());
    }

    if let Some(body_array) = card.get("body").and_then(Value::as_array) {
        let mut segments = Vec::new();
        for block in body_array {
            if let Some(text) = block.get("text").and_then(Value::as_str) {
                let trimmed = text.trim();
                if !trimmed.is_empty() {
                    segments.push(trimmed.to_string());
                }
            }
        }
        if !segments.is_empty() {
            return Some(segments.join(" "));
        }
    }

    None
}

/// Build the JSON body for a `POST /messages` Webex API call.
///
/// When an Adaptive Card is supplied it is attached (capping the card version
/// at 1.3, which is the highest version Webex currently supports). Otherwise
/// the raw `text` field is included. `markdown` is always set so that Webex
/// renders the fallback text consistently.
pub(super) fn build_webex_body(
    card_payload: Option<&Value>,
    text_value: Option<&String>,
    markdown: &str,
) -> serde_json::Map<String, Value> {
    let mut map = serde_json::Map::new();
    if let Some(card) = card_payload {
        // Webex supports AC up to v1.3 — cap the version.
        let mut card = card.clone();
        if let Some(obj) = card.as_object_mut() {
            let ver = obj.get("version").and_then(Value::as_str).unwrap_or("1.0");
            if ver != "1.0" && ver != "1.1" && ver != "1.2" && ver != "1.3" {
                obj.insert("version".into(), Value::String("1.3".to_string()));
            }
        }
        let attachment = serde_json::json!({
            "contentType": "application/vnd.microsoft.card.adaptive",
            "content": card,
        });
        map.insert("attachments".into(), Value::Array(vec![attachment]));
    } else if let Some(text_val) = text_value {
        map.insert("text".into(), Value::String(text_val.clone()));
    }
    map.insert("markdown".into(), Value::String(markdown.to_string()));
    map
}

/// Format a non-2xx Webex API response into a diagnostic error string.
pub(super) fn format_webex_error(status: u16, body: &[u8]) -> String {
    let trimmed = String::from_utf8_lossy(body).trim().to_string();
    if trimmed.is_empty() {
        format!("webex returned status {}", status)
    } else {
        format!("webex returned status {} body={}", status, trimmed)
    }
}
