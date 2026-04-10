//! Recursive AC element → Slack block dispatcher.
//!
//! This is the heart of the Adaptive Card → Slack Block Kit converter.
//! Each AC element type is mapped to the nearest Slack Block Kit construct;
//! unsupported element types are silently skipped.

use serde_json::{Value, json};

use super::actions::{collect_select_action, collect_slack_actions};
use super::markdown::{ac_markdown_to_slack, extract_texts_from_items};

/// Recursively convert an AC body element to Slack Block Kit blocks.
/// When `has_modal` is true, input fields (Input.Text, Input.ChoiceSet) are skipped
/// because they will be rendered inside a Slack modal instead.
pub(super) fn ac_element_to_blocks(
    element: &Value,
    blocks: &mut Vec<Value>,
    actions: &mut Vec<Value>,
    has_modal: bool,
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
            let is_subtle = element
                .get("isSubtle")
                .and_then(Value::as_bool)
                .unwrap_or(false);

            let converted = ac_markdown_to_slack(text);

            if is_heading || size == "extralarge" {
                // Slack header block: plain_text, max 150 chars.
                // Strip mrkdwn chars for plain_text header.
                let plain: String = converted.replace('*', "").chars().take(150).collect();
                blocks.push(json!({
                    "type": "header",
                    "text": { "type": "plain_text", "text": plain, "emoji": true }
                }));
            } else if is_subtle || size == "small" {
                // Context block for subtle/small text — appears smaller and grayed.
                blocks.push(json!({
                    "type": "context",
                    "elements": [{ "type": "mrkdwn", "text": converted }]
                }));
            } else if is_bold || size == "large" {
                // Bold section.
                let bold = if converted.starts_with('*') {
                    converted
                } else {
                    format!("*{converted}*")
                };
                blocks.push(json!({
                    "type": "section",
                    "text": { "type": "mrkdwn", "text": bold }
                }));
            } else {
                blocks.push(json!({
                    "type": "section",
                    "text": { "type": "mrkdwn", "text": converted }
                }));
            }
        }

        "RichTextBlock" => {
            let inlines = element.get("inlines").and_then(Value::as_array);
            if let Some(inlines) = inlines {
                let mut mrkdwn = String::new();
                for inline in inlines {
                    let text = inline
                        .get("text")
                        .and_then(Value::as_str)
                        .or_else(|| inline.as_str())
                        .unwrap_or_default();
                    if text.is_empty() {
                        continue;
                    }
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
                    // Hyperlink
                    if let Some(url) = inline.get("selectAction").and_then(|a| {
                        if a.get("type").and_then(Value::as_str) == Some("Action.OpenUrl") {
                            a.get("url").and_then(Value::as_str)
                        } else {
                            None
                        }
                    }) {
                        s = format!("<{url}|{s}>");
                    }
                    mrkdwn.push_str(&s);
                }
                if !mrkdwn.is_empty() {
                    blocks.push(json!({
                        "type": "section",
                        "text": { "type": "mrkdwn", "text": mrkdwn }
                    }));
                }
            }
        }

        "Image" => {
            if let Some(url) = element.get("url").and_then(Value::as_str) {
                let alt = element
                    .get("altText")
                    .and_then(Value::as_str)
                    .unwrap_or("image");
                blocks.push(json!({
                    "type": "image",
                    "image_url": url,
                    "alt_text": alt
                }));
            }
        }

        "ImageSet" => {
            if let Some(imgs) = element.get("images").and_then(Value::as_array) {
                for img in imgs {
                    if let Some(url) = img.get("url").and_then(Value::as_str) {
                        let alt = img
                            .get("altText")
                            .and_then(Value::as_str)
                            .unwrap_or("image");
                        blocks.push(json!({
                            "type": "image",
                            "image_url": url,
                            "alt_text": alt
                        }));
                    }
                }
            }
        }

        "FactSet" => {
            if let Some(facts) = element.get("facts").and_then(Value::as_array) {
                // Slack section fields: max 10 fields, each max 2000 chars.
                let fields: Vec<Value> = facts
                    .iter()
                    .take(10)
                    .filter_map(|fact| {
                        let title = fact
                            .get("title")
                            .and_then(Value::as_str)
                            .unwrap_or_default();
                        let value = fact
                            .get("value")
                            .and_then(Value::as_str)
                            .unwrap_or_default();
                        if title.is_empty() && value.is_empty() {
                            return None;
                        }
                        Some(json!({
                            "type": "mrkdwn",
                            "text": format!("*{title}*\n{value}")
                        }))
                    })
                    .collect();
                if !fields.is_empty() {
                    blocks.push(json!({
                        "type": "section",
                        "fields": fields
                    }));
                }
            }
        }

        "ColumnSet" => {
            if let Some(columns) = element.get("columns").and_then(Value::as_array) {
                // Try to merge icon+text columns into a single mrkdwn line.
                // Pattern: [auto-width emoji col] [stretch text col] → "emoji *title*\ndesc"
                let col_texts: Vec<Vec<String>> = columns
                    .iter()
                    .map(|col| {
                        col.get("items")
                            .and_then(Value::as_array)
                            .map(|items| extract_texts_from_items(items))
                            .unwrap_or_default()
                    })
                    .collect();

                if col_texts.len() == 2
                    && col_texts[0].len() == 1
                    && col_texts[0][0].chars().count() <= 3
                {
                    // Icon + text pattern — merge into single section.
                    let icon = &col_texts[0][0];
                    let text_parts = col_texts[1].join("\n");
                    let merged = format!("{icon}  {text_parts}");
                    blocks.push(json!({
                        "type": "section",
                        "text": { "type": "mrkdwn", "text": merged }
                    }));
                } else {
                    // General columns → section fields.
                    let mut fields: Vec<Value> = Vec::new();
                    for texts in &col_texts {
                        if !texts.is_empty() {
                            fields.push(json!({
                                "type": "mrkdwn",
                                "text": texts.join("\n")
                            }));
                        }
                    }
                    if !fields.is_empty() {
                        fields.truncate(10);
                        blocks.push(json!({
                            "type": "section",
                            "fields": fields
                        }));
                    }
                }

                // Convert Column selectAction / nested Container selectAction to Slack buttons.
                for col in columns {
                    collect_select_action(col, actions);
                    // Also check Container items inside each Column.
                    if let Some(items) = col.get("items").and_then(Value::as_array) {
                        for item in items {
                            if item.get("type").and_then(Value::as_str) == Some("Container") {
                                collect_select_action(item, actions);
                            }
                        }
                    }
                }
            }

            // ColumnSet-level selectAction.
            collect_select_action(element, actions);
        }

        "Container" => {
            // In AC, isVisible:false containers are toggled by Action.ToggleVisibility.
            // Slack has no toggle concept, so render hidden containers anyway — the user
            // needs to see the content that would normally be revealed by a toggle.

            let has_style = element
                .get("style")
                .and_then(Value::as_str)
                .is_some_and(|s| {
                    s == "accent"
                        || s == "emphasis"
                        || s == "good"
                        || s == "attention"
                        || s == "warning"
                });

            // Add divider before styled containers for visual separation.
            if has_style && !blocks.is_empty() {
                blocks.push(json!({"type": "divider"}));
            }

            if let Some(items) = element.get("items").and_then(Value::as_array) {
                for item in items {
                    ac_element_to_blocks(item, blocks, actions, has_modal);
                }
            }

            // Convert Container selectAction to Slack button.
            collect_select_action(element, actions);
        }

        "ActionSet" => {
            if let Some(action_list) = element.get("actions").and_then(Value::as_array) {
                collect_slack_actions(action_list, actions);
            }
        }

        "Input.Text" | "Input.ChoiceSet" => {
            // When modal is active, skip inline rendering — these will be in the modal.
            // Otherwise fall back to inline rendering.
            if has_modal {
                return;
            }
            if etype == "Input.Text" {
                let label = element
                    .get("label")
                    .and_then(Value::as_str)
                    .or_else(|| element.get("placeholder").and_then(Value::as_str));
                if let Some(label) = label {
                    blocks.push(json!({
                        "type": "context",
                        "elements": [{ "type": "mrkdwn", "text": format!("_{label}_") }]
                    }));
                }
            } else {
                // Input.ChoiceSet inline fallback (no modal).
                if let Some(choices) = element.get("choices").and_then(Value::as_array) {
                    let options: Vec<Value> = choices
                        .iter()
                        .take(100)
                        .filter_map(|c| {
                            let title = c.get("title").and_then(Value::as_str)?;
                            let value = c.get("value").and_then(Value::as_str).unwrap_or(title);
                            Some(json!({
                                "text": { "type": "plain_text", "text": title.chars().take(75).collect::<String>() },
                                "value": value.chars().take(75).collect::<String>()
                            }))
                        })
                        .collect();
                    if !options.is_empty() {
                        let input_id = element
                            .get("id")
                            .and_then(Value::as_str)
                            .unwrap_or("choice");
                        let placeholder = element
                            .get("placeholder")
                            .and_then(Value::as_str)
                            .unwrap_or("Select");
                        let is_multi = element
                            .get("isMultiSelect")
                            .and_then(Value::as_bool)
                            .unwrap_or(false);
                        let select_type = if is_multi {
                            "multi_static_select"
                        } else {
                            "static_select"
                        };
                        let select = json!({
                            "type": select_type,
                            "action_id": format!("ac_input_{input_id}"),
                            "placeholder": { "type": "plain_text", "text": placeholder.chars().take(150).collect::<String>() },
                            "options": options
                        });
                        let label = element
                            .get("label")
                            .and_then(Value::as_str)
                            .unwrap_or(placeholder);
                        blocks.push(json!({
                            "type": "section",
                            "text": { "type": "mrkdwn", "text": format!("*{label}*") },
                            "accessory": select
                        }));
                    }
                }
            }
        }

        "Table" => {
            let rows = element.get("rows").and_then(Value::as_array);
            let columns = element.get("columns").and_then(Value::as_array);
            if let Some(rows) = rows {
                let mut lines = Vec::new();
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
                        lines.push(
                            headers
                                .iter()
                                .map(|h| format!("*{h}*"))
                                .collect::<Vec<_>>()
                                .join(" | "),
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
                        lines.push(cell_texts.join(" | "));
                    }
                }
                if !lines.is_empty() {
                    blocks.push(json!({
                        "type": "section",
                        "text": {
                            "type": "mrkdwn",
                            "text": format!("```\n{}\n```", lines.join("\n"))
                        }
                    }));
                }
            }
        }

        _ => {}
    }
}
