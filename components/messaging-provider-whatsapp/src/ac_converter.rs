//! Adaptive Card → WhatsApp content conversion.
//!
//! Implements [`provider_common::AdaptiveCardConverter`] for the WhatsApp
//! provider and exposes the lower-level [`ac_to_whatsapp`] helper used by
//! `ops::encode_op` on the happy-path (it keeps returning `Option` so the
//! call-site can fall back to plain-text downsampling without threading a
//! `Result` through).

use serde_json::{Value, json};

/// Extracted WhatsApp content from an Adaptive Card.
#[derive(Debug)]
pub(crate) struct WhatsAppAcContent {
    pub(crate) header: Option<String>,
    pub(crate) body: String,
    pub(crate) buttons: Vec<Value>,
    pub(crate) image_url: Option<String>,
}

/// Convert an Adaptive Card JSON string into WhatsApp-native content.
///
/// WhatsApp supports:
/// - Interactive messages with reply buttons (max 3, title max 20 chars)
/// - Image messages with caption (max 1024 chars)
/// - Plain text (max 4096 chars)
pub(crate) fn ac_to_whatsapp(ac_raw: &str) -> Option<WhatsAppAcContent> {
    let ac: Value = serde_json::from_str(ac_raw).ok()?;
    let body_elements = ac.get("body").and_then(Value::as_array);
    let top_actions = ac.get("actions").and_then(Value::as_array);

    let mut header: Option<String> = None;
    let mut lines: Vec<String> = Vec::new();
    let mut buttons: Vec<Value> = Vec::new();
    let mut image_url: Option<String> = None;

    if let Some(elements) = body_elements {
        for element in elements {
            wa_extract_element(
                element,
                &mut header,
                &mut lines,
                &mut buttons,
                &mut image_url,
            );
        }
    }
    if let Some(actions) = top_actions {
        wa_collect_buttons(actions, &mut buttons);
    }

    let body = lines.join("\n");
    if body.trim().is_empty() {
        return None;
    }
    // WhatsApp body max 4096 for text, 1024 for interactive.
    let max = if buttons.is_empty() { 4096 } else { 1024 };
    let body: String = body.chars().take(max).collect();

    Some(WhatsAppAcContent {
        header,
        body,
        buttons,
        image_url,
    })
}

/// Extract content from a single AC element for WhatsApp.
fn wa_extract_element(
    element: &Value,
    header: &mut Option<String>,
    lines: &mut Vec<String>,
    buttons: &mut Vec<Value>,
    image_url: &mut Option<String>,
) {
    let etype = element
        .get("type")
        .and_then(Value::as_str)
        .unwrap_or_default();

    match etype {
        "TextBlock" => {
            let text = element
                .get("text")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .trim();
            if text.is_empty() {
                return;
            }
            let is_bold = element
                .get("weight")
                .and_then(Value::as_str)
                .is_some_and(|w| w.eq_ignore_ascii_case("bolder"));
            let size = element
                .get("size")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_ascii_lowercase();
            let is_heading = element
                .get("style")
                .and_then(Value::as_str)
                .is_some_and(|s| s.eq_ignore_ascii_case("heading"));

            if (is_bold || is_heading || size == "large" || size == "extralarge")
                && header.is_none()
            {
                *header = Some(text.to_string());
            } else {
                let formatted = if is_bold {
                    format!("*{text}*")
                } else {
                    text.to_string()
                };
                lines.push(formatted);
            }
        }

        "RichTextBlock" => {
            if let Some(inlines) = element.get("inlines").and_then(Value::as_array) {
                let mut line = String::new();
                for inline in inlines {
                    let text = inline
                        .get("text")
                        .and_then(Value::as_str)
                        .or_else(|| inline.as_str())
                        .unwrap_or_default();
                    if !text.is_empty() {
                        let mut s = text.to_string();
                        if inline
                            .get("fontWeight")
                            .and_then(Value::as_str)
                            .is_some_and(|w| w.eq_ignore_ascii_case("bolder"))
                        {
                            s = format!("*{s}*");
                        }
                        if inline
                            .get("italic")
                            .and_then(Value::as_bool)
                            .unwrap_or(false)
                        {
                            s = format!("_{s}_");
                        }
                        if inline
                            .get("strikethrough")
                            .and_then(Value::as_bool)
                            .unwrap_or(false)
                        {
                            s = format!("~{s}~");
                        }
                        if inline
                            .get("fontType")
                            .and_then(Value::as_str)
                            .is_some_and(|f| f.eq_ignore_ascii_case("monospace"))
                        {
                            s = format!("`{s}`");
                        }
                        line.push_str(&s);
                    }
                }
                if !line.is_empty() {
                    lines.push(line);
                }
            }
        }

        "Image" => {
            if image_url.is_none()
                && let Some(url) = element.get("url").and_then(Value::as_str)
            {
                *image_url = Some(url.to_string());
            }
        }

        "ImageSet" => {
            if image_url.is_none()
                && let Some(imgs) = element.get("images").and_then(Value::as_array)
                && let Some(url) = imgs
                    .first()
                    .and_then(|i| i.get("url"))
                    .and_then(Value::as_str)
            {
                *image_url = Some(url.to_string());
            }
        }

        "FactSet" => {
            if let Some(facts) = element.get("facts").and_then(Value::as_array) {
                for fact in facts {
                    let title = fact
                        .get("title")
                        .and_then(Value::as_str)
                        .unwrap_or_default();
                    let value = fact
                        .get("value")
                        .and_then(Value::as_str)
                        .unwrap_or_default();
                    if !title.is_empty() || !value.is_empty() {
                        lines.push(format!("*{title}:* {value}"));
                    }
                }
            }
        }

        "ColumnSet" => {
            if let Some(columns) = element.get("columns").and_then(Value::as_array) {
                let mut col_texts: Vec<String> = Vec::new();
                for col in columns {
                    if let Some(items) = col.get("items").and_then(Value::as_array) {
                        let text: Vec<String> = items
                            .iter()
                            .filter_map(|i| {
                                i.get("text").and_then(Value::as_str).map(|s| s.to_string())
                            })
                            .collect();
                        if !text.is_empty() {
                            col_texts.push(text.join(" "));
                        }
                    }
                }
                if !col_texts.is_empty() {
                    lines.push(col_texts.join(" | "));
                }
            }
        }

        "Container" => {
            if let Some(items) = element.get("items").and_then(Value::as_array) {
                for item in items {
                    wa_extract_element(item, header, lines, buttons, image_url);
                }
            }
        }

        "ActionSet" => {
            if let Some(action_list) = element.get("actions").and_then(Value::as_array) {
                wa_collect_buttons(action_list, buttons);
            }
        }

        "Table" => {
            let rows = element.get("rows").and_then(Value::as_array);
            if let Some(rows) = rows {
                for row in rows {
                    if let Some(cells) = row.get("cells").and_then(Value::as_array) {
                        let cell_texts: Vec<String> = cells
                            .iter()
                            .map(|cell| {
                                cell.get("items")
                                    .and_then(Value::as_array)
                                    .map(|items| {
                                        items
                                            .iter()
                                            .filter_map(|i| i.get("text").and_then(Value::as_str))
                                            .collect::<Vec<_>>()
                                            .join(" ")
                                    })
                                    .unwrap_or_default()
                            })
                            .collect();
                        lines.push(cell_texts.join(" | "));
                    }
                }
            }
        }

        _ => {}
    }
}

/// Collect AC actions into WhatsApp button format (max 3 reply buttons).
fn wa_collect_buttons(action_list: &[Value], buttons: &mut Vec<Value>) {
    for action in action_list {
        if buttons.len() >= 3 {
            break;
        }
        let title = action
            .get("title")
            .and_then(Value::as_str)
            .unwrap_or_default();
        if title.is_empty() {
            continue;
        }
        buttons.push(json!({ "title": title }));
    }
}

// ─── AdaptiveCardConverter trait impl ───────────────────────────────────

/// Marker type implementing [`provider_common::AdaptiveCardConverter`] for
/// the WhatsApp provider. Produces header/body/buttons/image content
/// extracted from an Adaptive Card for WhatsApp Cloud API messages.
///
/// Currently only exercised by unit tests; `ops::encode_op` still calls
/// [`ac_to_whatsapp`] directly because it needs an `Option`-based fallback
/// path. The trait impl provides a uniform entry point for future generic
/// pipelines.
#[cfg_attr(not(test), allow(dead_code))]
pub(crate) struct WhatsAppConverter;

impl provider_common::AdaptiveCardConverter for WhatsAppConverter {
    type Output = WhatsAppAcContent;

    fn convert(
        &self,
        adaptive_card: &Value,
        _caps: &provider_common::render::PlannerCapabilities,
    ) -> Result<Self::Output, provider_common::ProviderError> {
        let ac_str = serde_json::to_string(adaptive_card).map_err(|e| {
            provider_common::ProviderError::Validation(format!("invalid adaptive card json: {e}"))
        })?;
        ac_to_whatsapp(&ac_str).ok_or_else(|| {
            provider_common::ProviderError::Validation(
                "adaptive card produced empty whatsapp payload".to_string(),
            )
        })
    }

    fn provider_name(&self) -> &'static str {
        "whatsapp"
    }
}

#[cfg(test)]
mod converter_tests {
    use super::*;
    use provider_common::AdaptiveCardConverter;

    #[test]
    fn converter_provider_name() {
        assert_eq!(WhatsAppConverter.provider_name(), "whatsapp");
    }

    #[test]
    fn converter_empty_card_errors() {
        let caps = provider_common::render::capabilities_for("whatsapp").unwrap();
        let card = json!({"type": "AdaptiveCard", "body": []});
        let err = WhatsAppConverter.convert(&card, &caps).unwrap_err();
        assert!(matches!(err, provider_common::ProviderError::Validation(_)));
    }

    #[test]
    fn converter_extracts_text_block() {
        let caps = provider_common::render::capabilities_for("whatsapp").unwrap();
        let card = json!({
            "type": "AdaptiveCard",
            "body": [{"type": "TextBlock", "text": "hello whatsapp"}]
        });
        let content = WhatsAppConverter.convert(&card, &caps).unwrap();
        assert!(content.body.contains("hello whatsapp"));
    }

    #[test]
    fn ac_to_whatsapp_extracts_heading_image_facts_and_buttons() {
        let card = json!({
            "type": "AdaptiveCard",
            "body": [
                {"type": "TextBlock", "text": "Incident 42", "weight": "Bolder"},
                {"type": "FactSet", "facts": [
                    {"title": "Severity", "value": "High"},
                    {"title": "Owner", "value": "Ops"}
                ]},
                {"type": "Image", "url": "https://example.com/incident.png"},
                {"type": "ActionSet", "actions": [
                    {"type": "Action.Submit", "title": "Ack"},
                    {"type": "Action.OpenUrl", "title": "Open"}
                ]}
            ],
            "actions": [
                {"type": "Action.Submit", "title": "Escalate"},
                {"type": "Action.Submit", "title": "Ignored fourth"}
            ]
        });

        let content = ac_to_whatsapp(&card.to_string()).expect("whatsapp content");

        assert_eq!(content.header.as_deref(), Some("Incident 42"));
        assert!(content.body.contains("*Severity:* High"));
        assert!(content.body.contains("*Owner:* Ops"));
        assert_eq!(
            content.image_url.as_deref(),
            Some("https://example.com/incident.png")
        );
        assert_eq!(content.buttons.len(), 3);
        assert_eq!(content.buttons[0]["title"], "Ack");
        assert_eq!(content.buttons[2]["title"], "Escalate");
    }

    #[test]
    fn ac_to_whatsapp_formats_rich_text_columns_and_tables() {
        let card = json!({
            "type": "AdaptiveCard",
            "body": [
                {"type": "RichTextBlock", "inlines": [
                    {"text": "bold", "fontWeight": "Bolder"},
                    {"text": " italic", "italic": true},
                    {"text": " code", "fontType": "Monospace"}
                ]},
                {"type": "ColumnSet", "columns": [
                    {"items": [{"type": "TextBlock", "text": "left"}]},
                    {"items": [{"type": "TextBlock", "text": "right"}]}
                ]},
                {"type": "Table", "rows": [{
                    "cells": [
                        {"items": [{"type": "TextBlock", "text": "A"}]},
                        {"items": [{"type": "TextBlock", "text": "B"}]}
                    ]
                }]}
            ]
        });

        let content = ac_to_whatsapp(&card.to_string()).expect("whatsapp content");

        assert!(content.body.contains("*bold*"));
        assert!(content.body.contains("_ italic_"));
        assert!(content.body.contains("` code`"));
        assert!(content.body.contains("left | right"));
        assert!(content.body.contains("A | B"));
    }
}
