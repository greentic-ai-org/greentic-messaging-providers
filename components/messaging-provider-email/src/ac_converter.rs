//! Adaptive Card → styled HTML email body conversion.
//!
//! Implements [`provider_common::AdaptiveCardConverter`] for the Email
//! provider and exposes the lower-level [`ac_to_email_html`] helper used by
//! `ops::encode_op` directly on the happy-path.

use serde_json::Value;

/// Convert an Adaptive Card JSON string into a styled HTML email body.
///
/// Email supports full HTML/CSS, so this produces the richest rendering:
/// - TextBlock → styled headings and paragraphs
/// - RichTextBlock → inline formatting (bold, italic, strikethrough, code, underline, links)
/// - Image/ImageSet → `<img>` tags
/// - FactSet → HTML table with bold keys
/// - ColumnSet → flexbox columns
/// - Container → `<div>` with border
/// - ActionSet + actions → styled link buttons
/// - Table → full HTML table
pub(crate) fn ac_to_email_html(ac_raw: &str) -> Option<String> {
    let ac: Value = serde_json::from_str(ac_raw).ok()?;
    let body = ac.get("body").and_then(Value::as_array);
    let top_actions = ac.get("actions").and_then(Value::as_array);

    let mut parts: Vec<String> = Vec::new();

    if let Some(body) = body {
        for element in body {
            email_element_to_html(element, &mut parts);
        }
    }
    if let Some(actions) = top_actions {
        let btns = email_action_buttons(actions);
        if !btns.is_empty() {
            parts.push(format!(
                "<div style=\"margin-top:16px;\">{}</div>",
                btns.join(" ")
            ));
        }
    }

    if parts.is_empty() {
        return None;
    }

    let inner = parts.join("\n");
    Some(format!(
        "<div style=\"font-family:Segoe UI,Helvetica,Arial,sans-serif;max-width:600px;\
         margin:0 auto;padding:20px;background:#fff;border:1px solid #e0e0e0;\
         border-radius:8px;\">\n{inner}\n</div>"
    ))
}

/// Convert a single AC body element to email HTML.
fn email_element_to_html(element: &Value, parts: &mut Vec<String>) {
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

            if is_heading || size == "extralarge" {
                parts.push(format!(
                    "<h1 style=\"margin:0 0 8px;color:#333;\">{escaped}</h1>"
                ));
            } else if is_bold || size == "large" {
                parts.push(format!(
                    "<h2 style=\"margin:0 0 8px;color:#333;\">{escaped}</h2>"
                ));
            } else if size == "medium" {
                parts.push(format!(
                    "<h3 style=\"margin:0 0 6px;color:#333;\">{escaped}</h3>"
                ));
            } else if is_subtle || size == "small" {
                parts.push(format!(
                    "<p style=\"margin:4px 0;color:#888;font-size:13px;\">{escaped}</p>"
                ));
            } else {
                parts.push(format!(
                    "<p style=\"margin:4px 0;color:#333;\">{escaped}</p>"
                ));
            }
        }

        "RichTextBlock" => {
            if let Some(inlines) = element.get("inlines").and_then(Value::as_array) {
                let mut html = String::new();
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
                        s = format!("<strong>{s}</strong>");
                    }
                    if inline
                        .get("italic")
                        .and_then(Value::as_bool)
                        .unwrap_or(false)
                    {
                        s = format!("<em>{s}</em>");
                    }
                    if inline
                        .get("strikethrough")
                        .and_then(Value::as_bool)
                        .unwrap_or(false)
                    {
                        s = format!("<del>{s}</del>");
                    }
                    if inline
                        .get("fontType")
                        .and_then(Value::as_str)
                        .is_some_and(|f| f.eq_ignore_ascii_case("monospace"))
                    {
                        s = format!(
                            "<code style=\"background:#f4f4f4;padding:2px 4px;\
                             border-radius:3px;\">{s}</code>"
                        );
                    }
                    if inline
                        .get("underline")
                        .and_then(Value::as_bool)
                        .unwrap_or(false)
                    {
                        s = format!("<u>{s}</u>");
                    }
                    if let Some(url) = inline.get("selectAction").and_then(|a| {
                        if a.get("type").and_then(Value::as_str) == Some("Action.OpenUrl") {
                            a.get("url").and_then(Value::as_str)
                        } else {
                            None
                        }
                    }) {
                        s = format!(
                            "<a href=\"{}\" style=\"color:#0078d4;\">{s}</a>",
                            html_escape(url)
                        );
                    }
                    html.push_str(&s);
                }
                if !html.is_empty() {
                    parts.push(format!("<p style=\"margin:4px 0;color:#333;\">{html}</p>"));
                }
            }
        }

        "Image" => {
            if let Some(url) = element.get("url").and_then(Value::as_str) {
                let alt = element
                    .get("altText")
                    .and_then(Value::as_str)
                    .unwrap_or("image");
                parts.push(format!(
                    "<div style=\"margin:8px 0;\"><img src=\"{}\" alt=\"{}\" \
                     style=\"max-width:100%;border-radius:4px;\" /></div>",
                    html_escape(url),
                    html_escape(alt)
                ));
            }
        }

        "ImageSet" => {
            if let Some(imgs) = element.get("images").and_then(Value::as_array) {
                let mut img_html = Vec::new();
                for img in imgs {
                    if let Some(url) = img.get("url").and_then(Value::as_str) {
                        let alt = img
                            .get("altText")
                            .and_then(Value::as_str)
                            .unwrap_or("image");
                        img_html.push(format!(
                            "<img src=\"{}\" alt=\"{}\" \
                             style=\"max-width:48%;border-radius:4px;margin:4px;\" />",
                            html_escape(url),
                            html_escape(alt)
                        ));
                    }
                }
                if !img_html.is_empty() {
                    parts.push(format!(
                        "<div style=\"margin:8px 0;\">{}</div>",
                        img_html.join("")
                    ));
                }
            }
        }

        "FactSet" => {
            if let Some(facts) = element.get("facts").and_then(Value::as_array) {
                let mut rows = Vec::new();
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
                        rows.push(format!(
                            "<tr><td style=\"padding:4px 12px 4px 0;font-weight:bold;\
                             color:#555;white-space:nowrap;\">{}</td>\
                             <td style=\"padding:4px 0;color:#333;\">{}</td></tr>",
                            html_escape(title),
                            html_escape(value)
                        ));
                    }
                }
                if !rows.is_empty() {
                    parts.push(format!(
                        "<table style=\"margin:8px 0;border-collapse:collapse;\">\
                         {}</table>",
                        rows.join("")
                    ));
                }
            }
        }

        "ColumnSet" => {
            if let Some(columns) = element.get("columns").and_then(Value::as_array) {
                let mut cols_html: Vec<String> = Vec::new();
                for col in columns {
                    if let Some(items) = col.get("items").and_then(Value::as_array) {
                        let mut col_parts: Vec<String> = Vec::new();
                        for item in items {
                            email_element_to_html(item, &mut col_parts);
                        }
                        if !col_parts.is_empty() {
                            cols_html.push(format!(
                                "<td style=\"vertical-align:top;padding:0 8px;\">{}</td>",
                                col_parts.join("")
                            ));
                        }
                    }
                }
                if !cols_html.is_empty() {
                    parts.push(format!(
                        "<table style=\"width:100%;margin:8px 0;\"><tr>{}</tr></table>",
                        cols_html.join("")
                    ));
                }
            }
        }

        "Container" => {
            if let Some(items) = element.get("items").and_then(Value::as_array) {
                let mut inner: Vec<String> = Vec::new();
                for item in items {
                    email_element_to_html(item, &mut inner);
                }
                if !inner.is_empty() {
                    parts.push(format!(
                        "<div style=\"margin:8px 0;padding:12px;border:1px solid #e8e8e8;\
                         border-radius:4px;background:#fafafa;\">{}</div>",
                        inner.join("")
                    ));
                }
            }
        }

        "ActionSet" => {
            if let Some(action_list) = element.get("actions").and_then(Value::as_array) {
                let btns = email_action_buttons(action_list);
                if !btns.is_empty() {
                    parts.push(format!(
                        "<div style=\"margin:8px 0;\">{}</div>",
                        btns.join(" ")
                    ));
                }
            }
        }

        "Table" => {
            let rows = element.get("rows").and_then(Value::as_array);
            let columns = element.get("columns").and_then(Value::as_array);
            if let Some(rows) = rows {
                let mut table_rows = Vec::new();
                // Header row
                if let Some(cols) = columns {
                    let headers: Vec<String> = cols
                        .iter()
                        .map(|c| {
                            let h = c
                                .get("title")
                                .or_else(|| c.get("header"))
                                .and_then(Value::as_str)
                                .unwrap_or("");
                            format!(
                                "<th style=\"padding:6px 12px;text-align:left;\
                                 border-bottom:2px solid #ddd;color:#555;\">{}</th>",
                                html_escape(h)
                            )
                        })
                        .collect();
                    if headers.iter().any(|h| !h.contains(">&lt;")) {
                        table_rows.push(format!("<tr>{}</tr>", headers.join("")));
                    }
                }
                for row in rows {
                    if let Some(cells) = row.get("cells").and_then(Value::as_array) {
                        let cell_html: Vec<String> = cells
                            .iter()
                            .map(|cell| {
                                let text = cell
                                    .get("items")
                                    .and_then(Value::as_array)
                                    .map(|items| {
                                        items
                                            .iter()
                                            .filter_map(|i| i.get("text").and_then(Value::as_str))
                                            .collect::<Vec<_>>()
                                            .join(" ")
                                    })
                                    .unwrap_or_default();
                                format!(
                                    "<td style=\"padding:6px 12px;\
                                     border-bottom:1px solid #eee;\">{}</td>",
                                    html_escape(&text)
                                )
                            })
                            .collect();
                        table_rows.push(format!("<tr>{}</tr>", cell_html.join("")));
                    }
                }
                if !table_rows.is_empty() {
                    parts.push(format!(
                        "<table style=\"width:100%;margin:8px 0;border-collapse:collapse;\">\
                         {}</table>",
                        table_rows.join("")
                    ));
                }
            }
        }

        _ => {}
    }
}

/// Convert AC actions to styled email button links.
fn email_action_buttons(action_list: &[Value]) -> Vec<String> {
    let mut btns = Vec::new();
    for action in action_list {
        let title = action
            .get("title")
            .and_then(Value::as_str)
            .unwrap_or_default();
        if title.is_empty() {
            continue;
        }
        let atype = action
            .get("type")
            .and_then(Value::as_str)
            .unwrap_or_default();
        let escaped = html_escape(title);
        if atype == "Action.OpenUrl" {
            let url = action.get("url").and_then(Value::as_str).unwrap_or("#");
            btns.push(format!(
                "<a href=\"{}\" style=\"display:inline-block;padding:8px 16px;\
                 background:#0078d4;color:#fff;text-decoration:none;\
                 border-radius:4px;margin:4px 4px 4px 0;font-size:14px;\">{escaped}</a>",
                html_escape(url)
            ));
        } else {
            // Non-URL actions rendered as disabled-style buttons.
            btns.push(format!(
                "<span style=\"display:inline-block;padding:8px 16px;\
                 background:#f0f0f0;color:#666;border-radius:4px;\
                 margin:4px 4px 4px 0;font-size:14px;\">{escaped}</span>"
            ));
        }
    }
    btns
}

/// Escape HTML special characters.
fn html_escape(text: &str) -> String {
    text.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

// ─── AdaptiveCardConverter trait impl ───────────────────────────────────

/// Marker type implementing [`provider_common::AdaptiveCardConverter`] for
/// the Email provider. Produces styled HTML email bodies from Adaptive Cards.
///
/// Currently only exercised by unit tests; `ops::encode_op` still calls
/// [`ac_to_email_html`] directly because it needs an `Option`-based fallback
/// path. The trait impl provides a uniform entry point for future generic
/// pipelines.
#[cfg_attr(not(test), allow(dead_code))]
pub(crate) struct EmailHtmlConverter;

impl provider_common::AdaptiveCardConverter for EmailHtmlConverter {
    type Output = String;

    fn convert(
        &self,
        adaptive_card: &Value,
        _caps: &greentic_messaging_renderer::PlannerCapabilities,
    ) -> Result<Self::Output, provider_common::ProviderError> {
        let ac_str = serde_json::to_string(adaptive_card).map_err(|e| {
            provider_common::ProviderError::Validation(format!("invalid adaptive card json: {e}"))
        })?;
        ac_to_email_html(&ac_str).ok_or_else(|| {
            provider_common::ProviderError::Validation(
                "adaptive card produced empty email html payload".to_string(),
            )
        })
    }

    fn provider_name(&self) -> &'static str {
        "email"
    }
}

#[cfg(test)]
mod converter_tests {
    use super::*;
    use provider_common::AdaptiveCardConverter;

    #[test]
    fn converter_provider_name() {
        assert_eq!(EmailHtmlConverter.provider_name(), "email");
    }

    #[test]
    fn converter_empty_card_errors() {
        let caps = greentic_messaging_renderer::capabilities_for("email").unwrap();
        let card = serde_json::json!({"type": "AdaptiveCard", "body": []});
        let err = EmailHtmlConverter.convert(&card, &caps).unwrap_err();
        assert!(matches!(err, provider_common::ProviderError::Validation(_)));
    }

    #[test]
    fn converter_renders_text_block() {
        let caps = greentic_messaging_renderer::capabilities_for("email").unwrap();
        let card = serde_json::json!({
            "type": "AdaptiveCard",
            "body": [{"type": "TextBlock", "text": "hello world"}]
        });
        let html = EmailHtmlConverter.convert(&card, &caps).unwrap();
        assert!(html.contains("hello world"));
        assert!(html.contains("<div"));
    }
}
