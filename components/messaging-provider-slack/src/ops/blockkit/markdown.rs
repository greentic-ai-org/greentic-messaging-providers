//! AC markdown ↔ Slack mrkdwn utilities.

use serde_json::Value;

/// Convert AC markdown to Slack mrkdwn: `**bold**` → `*bold*`, `[text](url)` → `<url|text>`.
pub(super) fn ac_markdown_to_slack(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let chars: Vec<char> = text.chars().collect();
    let mut i = 0;
    while i < chars.len() {
        // **bold** → *bold*
        if i + 1 < chars.len()
            && chars[i] == '*'
            && chars[i + 1] == '*'
            && let Some(end) = chars[i + 2..]
                .windows(2)
                .position(|w| w[0] == '*' && w[1] == '*')
        {
            let inner: String = chars[i + 2..i + 2 + end].iter().collect();
            out.push('*');
            out.push_str(&inner);
            out.push('*');
            i += 4 + end;
            continue;
        }
        // [text](url) → <url|text>
        if chars[i] == '['
            && let Some(close_bracket) = chars[i + 1..].iter().position(|&c| c == ']')
        {
            let cb = i + 1 + close_bracket;
            if cb + 1 < chars.len()
                && chars[cb + 1] == '('
                && let Some(close_paren) = chars[cb + 2..].iter().position(|&c| c == ')')
            {
                let link_text: String = chars[i + 1..cb].iter().collect();
                let url: String = chars[cb + 2..cb + 2 + close_paren].iter().collect();
                out.push('<');
                out.push_str(&url);
                out.push('|');
                out.push_str(&link_text);
                out.push('>');
                i = cb + 3 + close_paren;
                continue;
            }
        }
        out.push(chars[i]);
        i += 1;
    }
    out
}

/// Extract all text from an AC element tree (for ColumnSet merging).
pub(super) fn extract_texts_from_items(items: &[Value]) -> Vec<String> {
    let mut texts = Vec::new();
    for item in items {
        let etype = item.get("type").and_then(Value::as_str).unwrap_or_default();
        match etype {
            "TextBlock" => {
                if let Some(t) = item.get("text").and_then(Value::as_str) {
                    let t = t.trim();
                    if !t.is_empty() {
                        let is_bold = item
                            .get("weight")
                            .and_then(Value::as_str)
                            .is_some_and(|w| w.eq_ignore_ascii_case("bolder"));
                        let converted = ac_markdown_to_slack(t);
                        if is_bold && !converted.starts_with('*') {
                            texts.push(format!("*{converted}*"));
                        } else {
                            texts.push(converted);
                        }
                    }
                }
            }
            "Container" => {
                if let Some(sub) = item.get("items").and_then(Value::as_array) {
                    texts.extend(extract_texts_from_items(sub));
                }
            }
            _ => {
                if let Some(t) = item.get("text").and_then(Value::as_str)
                    && !t.trim().is_empty()
                {
                    texts.push(ac_markdown_to_slack(t.trim()));
                }
            }
        }
    }
    texts
}
