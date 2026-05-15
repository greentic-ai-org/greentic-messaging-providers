//! Adaptive Card → Telegram HTML + inline keyboard converter.
//!
//! This file owns the public entry point [`ac_to_telegram`] plus the per-element
//! walker [`ac_element_to_html`]. Input-element handling lives in
//! [`super::ac_inputs`] and shared helpers (escaping, truncation, action
//! collection, inline keyboard builder) live in [`super::ac_helpers`] so that
//! every file stays under the 500 LOC limit required by the project coding
//! standards.

use serde::{Deserialize, Serialize};
use serde_json::Value;

use super::ac_helpers::{collect_actions, collect_select_action, html_escape, truncate_html};
use super::ac_inputs::handle_ac_input;

/// Extracted Telegram content from an Adaptive Card.
pub(crate) struct TelegramAcContent {
    pub(crate) html: String,
    pub(crate) actions: Vec<Value>,
    pub(crate) images: Vec<String>,
    /// Input fields (Input.Text, Input.ChoiceSet, Input.Toggle) for conversational prompting.
    pub(crate) inputs: Vec<AcInput>,
}

/// A single AC input field mapped for Telegram conversational flow.
#[derive(Clone, Serialize, Deserialize)]
pub(crate) struct AcInput {
    pub(crate) id: String,
    pub(crate) label: String,
    pub(crate) placeholder: String,
    pub(crate) kind: AcInputKind,
    /// For ChoiceSet: the available choices.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub(crate) choices: Vec<AcChoice>,
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum AcInputKind {
    Text,
    Choice,
    Toggle,
}

#[derive(Clone, Serialize, Deserialize)]
pub(crate) struct AcChoice {
    pub(crate) title: String,
    pub(crate) value: String,
}

/// Convert an Adaptive Card JSON string into rich Telegram HTML + actions + images.
///
/// Maps every AC element to its best Telegram-native representation:
/// - TextBlock → `<b>` for bold/heading, plain for normal, `<i>` for subtle
/// - RichTextBlock → inline formatting (`<b>`, `<i>`, `<s>`, `<code>`)
/// - Image/ImageSet → collected for sendPhoto/sendMediaGroup
/// - FactSet → `<b>key:</b> value` lines
/// - ColumnSet → columns separated by ` │ `
/// - Container → recursive processing
/// - ActionSet + top-level actions → inline keyboard buttons
/// - Table → `<pre>` formatted table
pub(crate) fn ac_to_telegram(ac_raw: &str) -> Option<TelegramAcContent> {
    let ac: Value = serde_json::from_str(ac_raw).ok()?;
    let body = ac.get("body").and_then(Value::as_array);
    let top_actions = ac.get("actions").and_then(Value::as_array);

    let mut html_parts: Vec<String> = Vec::new();
    let mut actions: Vec<Value> = Vec::new();
    let mut images: Vec<String> = Vec::new();
    let mut inputs: Vec<AcInput> = Vec::new();

    if let Some(body) = body {
        for element in body {
            ac_element_to_html(
                element,
                &mut html_parts,
                &mut actions,
                &mut images,
                &mut inputs,
            );
        }
    }
    if let Some(top_actions) = top_actions {
        collect_actions(top_actions, &mut actions);
    }

    let html = html_parts.join("\n");
    if html.trim().is_empty() {
        return None;
    }

    // Telegram sendMessage max 4096 chars, sendPhoto caption max 1024 chars.
    // Truncate to 4096 for sendMessage; handle_send will further truncate for caption.
    let html = truncate_html(&html, 4096);

    Some(TelegramAcContent {
        html,
        actions,
        images,
        inputs,
    })
}

/// Recursively convert a single AC body element to Telegram HTML.
fn ac_element_to_html(
    element: &Value,
    parts: &mut Vec<String>,
    actions: &mut Vec<Value>,
    images: &mut Vec<String>,
    inputs: &mut Vec<AcInput>,
) {
    let etype = element
        .get("type")
        .and_then(Value::as_str)
        .unwrap_or_default();

    // Input.* elements are delegated to the `ac_inputs` submodule so that this
    // file stays under the project-wide 500 LOC limit.
    if handle_ac_input(etype, element, parts, actions, inputs) {
        return;
    }

    match etype {
        "TextBlock" => render_text_block(element, parts),
        "RichTextBlock" => render_rich_text_block(element, parts),
        "Image" => {
            if let Some(url) = element.get("url").and_then(Value::as_str) {
                images.push(url.to_string());
            }
        }
        "ImageSet" => {
            if let Some(imgs) = element.get("images").and_then(Value::as_array) {
                for img in imgs {
                    if let Some(url) = img.get("url").and_then(Value::as_str) {
                        images.push(url.to_string());
                    }
                }
            }
        }
        "FactSet" => render_fact_set(element, parts),
        "ColumnSet" => {
            if let Some(columns) = element.get("columns").and_then(Value::as_array) {
                let mut col_texts: Vec<String> = Vec::new();
                for col in columns {
                    if let Some(items) = col.get("items").and_then(Value::as_array) {
                        let mut col_parts: Vec<String> = Vec::new();
                        for item in items {
                            ac_element_to_html(item, &mut col_parts, actions, images, inputs);
                        }
                        if !col_parts.is_empty() {
                            col_texts.push(col_parts.join("\n"));
                        }
                    }
                    // Column-level selectAction → inline keyboard button.
                    collect_select_action(col, actions);
                    // Nested Container selectAction inside column.
                    if let Some(items) = col.get("items").and_then(Value::as_array) {
                        for item in items {
                            if item.get("type").and_then(Value::as_str) == Some("Container") {
                                collect_select_action(item, actions);
                            }
                        }
                    }
                }
                if !col_texts.is_empty() {
                    parts.push(col_texts.join(" │ "));
                }
                // ColumnSet-level selectAction.
                collect_select_action(element, actions);
            }
        }
        "Container" => {
            if let Some(items) = element.get("items").and_then(Value::as_array) {
                for item in items {
                    ac_element_to_html(item, parts, actions, images, inputs);
                }
            }
            // Container-level selectAction → inline keyboard button.
            collect_select_action(element, actions);
        }
        "ActionSet" => {
            if let Some(action_list) = element.get("actions").and_then(Value::as_array) {
                collect_actions(action_list, actions);
            }
        }
        "Table" => render_table(element, parts),
        _ => {
            // Unknown element type — ignore gracefully.
        }
    }
}

fn render_text_block(element: &Value, parts: &mut Vec<String>) {
    let text = element
        .get("text")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .trim();
    if text.is_empty() {
        return;
    }
    let escaped = html_escape(text);
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
    let is_subtle = element
        .get("isSubtle")
        .and_then(Value::as_bool)
        .unwrap_or(false);

    let html = if is_bold || is_heading || size == "large" || size == "extralarge" {
        format!("<b>{escaped}</b>")
    } else if size == "small" || is_subtle {
        format!("<i>{escaped}</i>")
    } else {
        escaped
    };
    parts.push(html);
}

fn render_rich_text_block(element: &Value, parts: &mut Vec<String>) {
    let inlines = element.get("inlines").and_then(Value::as_array);
    if let Some(inlines) = inlines {
        let mut rich = String::new();
        for inline in inlines {
            let text = inline
                .get("text")
                .and_then(Value::as_str)
                .or_else(|| inline.as_str())
                .unwrap_or_default();
            if text.is_empty() {
                continue;
            }
            let mut s = html_escape(text);
            if inline
                .get("fontWeight")
                .and_then(Value::as_str)
                .is_some_and(|w| w.eq_ignore_ascii_case("bolder"))
            {
                s = format!("<b>{s}</b>");
            }
            if inline
                .get("italic")
                .and_then(Value::as_bool)
                .unwrap_or(false)
            {
                s = format!("<i>{s}</i>");
            }
            if inline
                .get("strikethrough")
                .and_then(Value::as_bool)
                .unwrap_or(false)
            {
                s = format!("<s>{s}</s>");
            }
            if inline
                .get("fontType")
                .and_then(Value::as_str)
                .is_some_and(|f| f.eq_ignore_ascii_case("monospace"))
            {
                s = format!("<code>{s}</code>");
            }
            if inline
                .get("underline")
                .and_then(Value::as_bool)
                .unwrap_or(false)
            {
                s = format!("<u>{s}</u>");
            }
            // Check for hyperlink on TextRun
            if let Some(url) = inline.get("selectAction").and_then(|a| {
                if a.get("type").and_then(Value::as_str) == Some("Action.OpenUrl") {
                    a.get("url").and_then(Value::as_str)
                } else {
                    None
                }
            }) {
                s = format!("<a href=\"{}\">{s}</a>", html_escape(url));
            }
            rich.push_str(&s);
        }
        if !rich.is_empty() {
            parts.push(rich);
        }
    }
}

fn render_fact_set(element: &Value, parts: &mut Vec<String>) {
    if let Some(facts) = element.get("facts").and_then(Value::as_array) {
        let mut lines = Vec::new();
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
                lines.push(format!(
                    "<b>{}:</b> {}",
                    html_escape(title),
                    html_escape(value)
                ));
            }
        }
        if !lines.is_empty() {
            parts.push(lines.join("\n"));
        }
    }
}

/// Marker type implementing [`provider_common::AdaptiveCardConverter`] for the
/// Telegram HTML + inline-keyboard renderer.
///
/// The trait gives every provider a uniform "Adaptive Card → native payload"
/// entry point. For Telegram the conversion is delegated to the existing pure
/// [`ac_to_telegram`] function, which already handles TextBlock/RichTextBlock,
/// images, ColumnSet, Container, Table, and `Input.*` elements. Call sites in
/// `ops::encode` continue to use [`ac_to_telegram`] directly — this impl is
/// meant for generic pipelines that want to be polymorphic over converters.
#[allow(dead_code)]
pub(crate) struct TelegramHtmlConverter;

impl provider_common::AdaptiveCardConverter for TelegramHtmlConverter {
    type Output = TelegramAcContent;

    fn convert(
        &self,
        adaptive_card: &Value,
        _caps: &provider_common::render::PlannerCapabilities,
    ) -> Result<Self::Output, provider_common::ProviderError> {
        // The existing pure entry point takes a JSON string — re-serialise the
        // caller's Value so that we share a single parsing code path and don't
        // duplicate the element walker here.
        let ac_raw = serde_json::to_string(adaptive_card).map_err(|err| {
            provider_common::ProviderError::validation(format!(
                "telegram ac_converter: failed to serialise adaptive card: {err}"
            ))
        })?;
        ac_to_telegram(&ac_raw).ok_or_else(|| {
            provider_common::ProviderError::validation(
                "telegram ac_converter: adaptive card produced no renderable content",
            )
        })
    }

    fn provider_name(&self) -> &'static str {
        "telegram"
    }
}

fn render_table(element: &Value, parts: &mut Vec<String>) {
    // Render table rows as pre-formatted text.
    let rows = element.get("rows").and_then(Value::as_array);
    let columns = element.get("columns").and_then(Value::as_array);
    if let Some(rows) = rows {
        let mut table_lines = Vec::new();
        // Header from column titles
        if let Some(cols) = columns {
            let headers: Vec<String> = cols
                .iter()
                .map(|c| {
                    c.get("title")
                        .or_else(|| c.get("header"))
                        .and_then(Value::as_str)
                        .unwrap_or("")
                        .to_string()
                })
                .collect();
            if headers.iter().any(|h| !h.is_empty()) {
                table_lines.push(headers.join(" │ "));
                table_lines.push(
                    headers
                        .iter()
                        .map(|h| "─".repeat(h.len().max(3)))
                        .collect::<Vec<_>>()
                        .join("─┼─"),
                );
            }
        }
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
                table_lines.push(cell_texts.join(" │ "));
            }
        }
        if !table_lines.is_empty() {
            parts.push(format!(
                "<pre>{}</pre>",
                html_escape(&table_lines.join("\n"))
            ));
        }
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
        let caps = provider_common::render::capabilities_for("telegram")
            .expect("telegram capabilities must be registered");
        let result = TelegramHtmlConverter.convert(&card, &caps);
        assert!(result.is_ok(), "expected Ok converting simple card");
        let content = result.expect("converter produced Ok content");
        assert!(content.html.contains("hello"));
    }

    #[test]
    fn converter_provider_name() {
        assert_eq!(TelegramHtmlConverter.provider_name(), "telegram");
    }

    #[test]
    fn converter_empty_card_returns_validation_error() {
        let card = serde_json::json!({
            "type": "AdaptiveCard",
            "version": "1.6",
            "body": []
        });
        let caps = provider_common::render::capabilities_for("telegram")
            .expect("telegram capabilities must be registered");
        let result = TelegramHtmlConverter.convert(&card, &caps);
        assert!(matches!(
            result,
            Err(provider_common::ProviderError::Validation(_))
        ));
    }

    #[test]
    fn ac_to_telegram_renders_core_elements_actions_images_and_inputs() {
        let card = serde_json::json!({
            "type": "AdaptiveCard",
            "body": [
                {"type": "TextBlock", "text": "Title <unsafe>", "weight": "Bolder"},
                {"type": "RichTextBlock", "inlines": [
                    {"text": "bold", "fontWeight": "Bolder"},
                    {"text": " link", "selectAction": {"type": "Action.OpenUrl", "url": "https://example.com?a=1&b=2"}}
                ]},
                {"type": "FactSet", "facts": [{"title": "Status", "value": "Open"}]},
                {"type": "ImageSet", "images": [{"url": "https://example.com/a.png"}]},
                {"type": "Input.Text", "id": "comment", "label": "Comment"}
            ],
            "actions": [{"type": "Action.Submit", "title": "Save", "data": {"cardId": "c1"}}]
        });

        let content = ac_to_telegram(&card.to_string()).expect("telegram content");

        assert!(content.html.contains("<b>Title &lt;unsafe&gt;</b>"));
        assert!(content.html.contains("<b>bold</b>"));
        assert!(
            content
                .html
                .contains("<a href=\"https://example.com?a=1&amp;b=2\"> link</a>")
        );
        assert!(content.html.contains("<b>Status:</b> Open"));
        assert_eq!(content.images, vec!["https://example.com/a.png"]);
        assert_eq!(content.actions[0]["title"], "Save");
        assert_eq!(content.inputs[0].id, "comment");
    }

    #[test]
    fn ac_to_telegram_renders_columns_containers_select_actions_and_table() {
        let card = serde_json::json!({
            "type": "AdaptiveCard",
            "body": [
                {
                    "type": "ColumnSet",
                    "columns": [
                        {"items": [{"type": "TextBlock", "text": "Left"}],
                         "selectAction": {"type": "Action.Submit", "data": {"routeToCardId": "left"}}},
                        {"items": [{"type": "Container", "items": [{"type": "TextBlock", "text": "Right", "weight": "Bolder"}],
                         "selectAction": {"type": "Action.Execute", "data": {"routeToCardId": "right"}}}]}
                    ],
                    "selectAction": {"type": "Action.Submit", "data": {"routeToCardId": "whole"}}
                },
                {
                    "type": "Table",
                    "columns": [{"title": "A"}, {"title": "B"}],
                    "rows": [{"cells": [
                        {"items": [{"type": "TextBlock", "text": "1"}]},
                        {"items": [{"type": "TextBlock", "text": "2"}]}
                    ]}]
                }
            ]
        });

        let content = ac_to_telegram(&card.to_string()).expect("telegram content");

        assert!(content.html.contains("Left │ <b>Right</b>"));
        assert!(content.html.contains("<pre>A │ B"));
        assert_eq!(content.actions.len(), 3);
        assert_eq!(content.actions[0]["title"], "Left");
        assert_eq!(content.actions[1]["title"], "Right");
        assert_eq!(content.actions[2]["title"], "Right");
    }
}
