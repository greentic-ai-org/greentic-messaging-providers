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
mod identify;
mod ingest;
mod ingest_helpers;
mod render;
mod send;
mod send_payload;
mod webhook;

pub(crate) use encode::encode_op;
pub(crate) use identify::{IDENTIFY_HINT_JSON, extract_app_id};
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
        let card = normalize_adaptive_card_for_webex(card);
        let attachment = serde_json::json!({
            "contentType": "application/vnd.microsoft.card.adaptive",
            "content": card,
        });
        map.insert("attachments".into(), Value::Array(vec![attachment]));
        // Webex requires `text` as fallback when sending attachments.
        map.insert("text".into(), Value::String(markdown.to_string()));
    } else if let Some(text_val) = text_value {
        map.insert("text".into(), Value::String(text_val.clone()));
    }
    map.insert("markdown".into(), Value::String(markdown.to_string()));
    map
}

fn normalize_adaptive_card_for_webex(card: &Value) -> Value {
    let mut card = card.clone();
    if let Some(obj) = card.as_object_mut() {
        let ver = obj.get("version").and_then(Value::as_str).unwrap_or("1.0");
        if ver != "1.0" && ver != "1.1" && ver != "1.2" && ver != "1.3" {
            obj.insert("version".into(), Value::String("1.3".to_string()));
        }
        obj.remove("speak");
        obj.remove("$schema");
        obj.remove("rtl");
    }
    normalize_adaptive_card_value(&mut card);
    card
}

fn normalize_adaptive_card_value(value: &mut Value) {
    match value {
        Value::Object(obj) => {
            for key in ["isVisible", "wrap", "isSubtle", "bleed", "separator"] {
                if let Some(Value::String(text)) = obj.get(key) {
                    let normalized = match text.as_str() {
                        "true" => Some(true),
                        "false" => Some(false),
                        _ => None,
                    };
                    if let Some(boolean) = normalized {
                        obj.insert(key.to_string(), Value::Bool(boolean));
                    }
                }
            }

            for child in obj.values_mut() {
                normalize_adaptive_card_value(child);
            }
        }
        Value::Array(items) => {
            for item in items.iter_mut() {
                normalize_adaptive_card_value(item);
            }
            items.retain(|item| !is_webex_content_image(item));
        }
        _ => {}
    }
}

fn is_webex_content_image(value: &Value) -> bool {
    let Some(obj) = value.as_object() else {
        return false;
    };
    let is_image = obj.get("type").and_then(Value::as_str) == Some("Image");
    let is_webex_content = obj
        .get("url")
        .and_then(Value::as_str)
        .is_some_and(is_webex_content_url);
    is_image && is_webex_content
}

fn is_webex_content_url(url: &str) -> bool {
    url.starts_with("https://webexapis.com/v1/contents/")
        || url.starts_with("https://api.ciscospark.com/v1/contents/")
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

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn summarize_card_text_prefers_top_level_text_then_body_segments() {
        assert_eq!(
            summarize_card_text(&json!({"text": "  Summary  ", "body": [{"text": "Body"}]}))
                .as_deref(),
            Some("Summary")
        );
        assert_eq!(
            summarize_card_text(&json!({
                "body": [
                    {"text": " First "},
                    {"text": ""},
                    {"type": "Image"},
                    {"text": "Second"}
                ]
            }))
            .as_deref(),
            Some("First Second")
        );
        assert!(summarize_card_text(&json!({"body": []})).is_none());
    }

    #[test]
    fn build_webex_body_caps_card_version_and_strips_unsupported_fields() {
        let body = build_webex_body(
            Some(&json!({
                "$schema": "https://adaptivecards.io/schemas/adaptive-card.json",
                "type": "AdaptiveCard",
                "version": "1.5",
                "speak": "ignored",
                "rtl": false
            })),
            None,
            "fallback",
        );

        assert_eq!(body["text"], "fallback");
        assert_eq!(body["markdown"], "fallback");
        let content = &body["attachments"][0]["content"];
        assert_eq!(content["version"], "1.3");
        assert!(content.get("$schema").is_none());
        assert!(content.get("speak").is_none());
        assert!(content.get("rtl").is_none());
    }

    #[test]
    fn build_webex_body_coerces_adaptive_card_boolean_strings() {
        let body = build_webex_body(
            Some(&json!({
                "type": "AdaptiveCard",
                "version": "1.3",
                "body": [{
                    "type": "Container",
                    "isVisible": "true",
                    "items": [
                        {"type": "TextBlock", "text": "Wrapped", "wrap": "true"},
                        {"type": "TextBlock", "text": "Subtle", "isSubtle": "false"}
                    ]
                }]
            })),
            None,
            "fallback",
        );

        let content = &body["attachments"][0]["content"];
        assert_eq!(content["body"][0]["isVisible"], true);
        assert_eq!(content["body"][0]["items"][0]["wrap"], true);
        assert_eq!(content["body"][0]["items"][1]["isSubtle"], false);
    }

    #[test]
    fn build_webex_body_removes_webex_content_images_but_keeps_public_images() {
        let body = build_webex_body(
            Some(&json!({
                "type": "AdaptiveCard",
                "version": "1.3",
                "body": [{
                    "type": "ColumnSet",
                    "columns": [{
                        "type": "Column",
                        "items": [
                            {"type": "Image", "url": "https://webexapis.com/v1/contents/private"},
                            {"type": "Image", "url": "https://www.gstatic.com/webp/gallery/1.jpg"}
                        ]
                    }]
                }]
            })),
            None,
            "fallback",
        );

        let items = body["attachments"][0]["content"]["body"][0]["columns"][0]["items"]
            .as_array()
            .expect("column items");
        assert_eq!(items.len(), 1);
        assert_eq!(
            items[0]["url"],
            "https://www.gstatic.com/webp/gallery/1.jpg"
        );
    }

    #[test]
    fn build_webex_body_uses_text_when_no_card() {
        let text = "hello".to_string();
        let body = build_webex_body(None, Some(&text), "hello");

        assert_eq!(body["text"], "hello");
        assert_eq!(body["markdown"], "hello");
        assert!(body.get("attachments").is_none());
    }

    #[test]
    fn format_webex_error_includes_body_when_present() {
        assert_eq!(format_webex_error(500, b""), "webex returned status 500");
        assert_eq!(
            format_webex_error(400, br#"{"message":"bad room"}"#),
            r#"webex returned status 400 body={"message":"bad room"}"#
        );
    }
}
