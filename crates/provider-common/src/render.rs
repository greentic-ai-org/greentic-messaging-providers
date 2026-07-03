//! Lightweight render-planning helpers shared by messaging providers.
//!
//! This module intentionally contains only the render surface used by provider
//! components: channel capabilities, Adaptive Card extraction, and deterministic
//! text/attachment planning. Keeping it here lets the provider common crate be
//! published independently instead of depending on an unpublished workspace-only
//! renderer crate.

use serde::{Deserialize, Serialize};
use serde_json::Value;

/// Describes what a target messaging channel supports.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct PlannerCapabilities {
    pub supports_adaptive_cards: bool,
    pub supports_markdown: bool,
    pub supports_html: bool,
    pub supports_images: bool,
    pub supports_buttons: bool,
    pub max_text_len: Option<u32>,
    pub max_payload_bytes: Option<u32>,
}

/// A button or link action extracted from an Adaptive Card.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PlannerAction {
    pub title: String,
    pub url: Option<String>,
}

/// Intermediate card representation for the planner.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct PlannerCard {
    pub title: Option<String>,
    pub text: Option<String>,
    pub actions: Vec<PlannerAction>,
    pub images: Vec<String>,
}

/// Capability tiers that describe the quality of the rendered plan.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RenderTier {
    TierA,
    TierB,
    TierC,
    TierD,
}

/// Warning emitted while constructing a render plan.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RenderWarning {
    pub code: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub path: Option<String>,
}

/// Items produced by the planner.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum RenderItem {
    Text(String),
    AdaptiveCard(Value),
}

/// A render plan produced from an Adaptive Card or message text.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RenderPlan {
    pub tier: RenderTier,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub summary_text: Option<String>,
    #[serde(default)]
    pub items: Vec<RenderItem>,
    #[serde(default)]
    pub warnings: Vec<RenderWarning>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub debug: Option<Value>,
}

impl Default for RenderPlan {
    fn default() -> Self {
        Self {
            tier: RenderTier::TierD,
            summary_text: None,
            items: Vec::new(),
            warnings: Vec::new(),
            debug: None,
        }
    }
}

/// Returns the canonical capabilities for a known provider name.
pub fn capabilities_for(provider: &str) -> Option<PlannerCapabilities> {
    match provider {
        "slack" => Some(PlannerCapabilities {
            supports_adaptive_cards: false,
            supports_markdown: true,
            supports_html: false,
            supports_images: false,
            supports_buttons: false,
            max_text_len: Some(40_000),
            max_payload_bytes: None,
        }),
        "teams" => Some(PlannerCapabilities {
            supports_adaptive_cards: true,
            supports_markdown: true,
            supports_html: true,
            supports_images: true,
            supports_buttons: true,
            max_text_len: None,
            max_payload_bytes: None,
        }),
        "telegram" => Some(PlannerCapabilities {
            supports_adaptive_cards: false,
            supports_markdown: true,
            supports_html: true,
            supports_images: true,
            supports_buttons: false,
            max_text_len: Some(4096),
            max_payload_bytes: None,
        }),
        "webex" => Some(PlannerCapabilities {
            supports_adaptive_cards: true,
            supports_markdown: true,
            supports_html: true,
            supports_images: true,
            supports_buttons: false,
            max_text_len: None,
            max_payload_bytes: None,
        }),
        "whatsapp" => Some(PlannerCapabilities {
            supports_adaptive_cards: false,
            supports_markdown: false,
            supports_html: false,
            supports_images: true,
            supports_buttons: false,
            max_text_len: Some(4096),
            max_payload_bytes: None,
        }),
        "webchat" => Some(PlannerCapabilities {
            supports_adaptive_cards: true,
            supports_markdown: true,
            supports_html: true,
            supports_images: true,
            supports_buttons: true,
            max_text_len: None,
            max_payload_bytes: None,
        }),
        "email" => Some(PlannerCapabilities {
            supports_adaptive_cards: false,
            supports_markdown: false,
            supports_html: true,
            supports_images: true,
            supports_buttons: false,
            max_text_len: None,
            max_payload_bytes: None,
        }),
        "sms" => Some(PlannerCapabilities {
            supports_adaptive_cards: false,
            supports_markdown: false,
            supports_html: false,
            supports_images: false,
            supports_buttons: false,
            // Long messages are left to Twilio's own segmentation, not truncated here.
            max_text_len: None,
            max_payload_bytes: None,
        }),
        _ => None,
    }
}

/// Extract a [`PlannerCard`] from an Adaptive Card JSON value.
pub fn extract_planner_card(ac: &Value) -> PlannerCard {
    let body = ac.get("body").and_then(Value::as_array);
    let ac_actions = ac.get("actions").and_then(Value::as_array);

    let mut title = None;
    let mut text_parts = Vec::new();
    let mut actions = Vec::new();
    let mut images = Vec::new();

    if let Some(body) = body {
        extract_body_elements(
            body,
            &mut title,
            &mut text_parts,
            &mut actions,
            &mut images,
            0,
        );
    }

    if let Some(ac_actions) = ac_actions {
        extract_actions(ac_actions, &mut actions);
    }

    PlannerCard {
        title,
        text: (!text_parts.is_empty()).then(|| text_parts.join("\n")),
        actions,
        images,
    }
}

fn extract_body_elements(
    elements: &[Value],
    title: &mut Option<String>,
    text_parts: &mut Vec<String>,
    actions: &mut Vec<PlannerAction>,
    images: &mut Vec<String>,
    depth: usize,
) {
    if depth > 32 {
        return;
    }

    for element in elements {
        match element
            .get("type")
            .and_then(Value::as_str)
            .unwrap_or_default()
        {
            "TextBlock" => {
                if let Some(text) = element.get("text").and_then(Value::as_str) {
                    let trimmed = text.trim();
                    if trimmed.is_empty() {
                        continue;
                    }
                    if title.is_none() && is_title_textblock(element) {
                        *title = Some(trimmed.to_string());
                    } else {
                        text_parts.push(trimmed.to_string());
                    }
                }
            }
            "RichTextBlock" => {
                let mut parts = Vec::new();
                if let Some(inlines) = element.get("inlines").and_then(Value::as_array) {
                    for inline in inlines {
                        if let Some(text) = inline.get("text").and_then(Value::as_str) {
                            if !text.is_empty() {
                                parts.push(text.to_string());
                            }
                        } else if let Some(text) = inline.as_str()
                            && !text.is_empty()
                        {
                            parts.push(text.to_string());
                        }
                    }
                }
                let joined = parts.join("").trim().to_string();
                if !joined.is_empty() {
                    text_parts.push(joined);
                }
            }
            "Image" => {
                if let Some(url) = element.get("url").and_then(Value::as_str) {
                    images.push(url.to_string());
                }
            }
            "ImageSet" => {
                if let Some(items) = element.get("images").and_then(Value::as_array) {
                    for image in items {
                        if let Some(url) = image.get("url").and_then(Value::as_str) {
                            images.push(url.to_string());
                        }
                    }
                }
            }
            "ActionSet" => {
                if let Some(action_list) = element.get("actions").and_then(Value::as_array) {
                    extract_actions(action_list, actions);
                }
            }
            "Container" => {
                if let Some(items) = element.get("items").and_then(Value::as_array) {
                    extract_body_elements(items, title, text_parts, actions, images, depth + 1);
                }
            }
            "ColumnSet" => {
                if let Some(columns) = element.get("columns").and_then(Value::as_array) {
                    for column in columns {
                        if let Some(items) = column.get("items").and_then(Value::as_array) {
                            extract_body_elements(
                                items,
                                title,
                                text_parts,
                                actions,
                                images,
                                depth + 1,
                            );
                        }
                    }
                }
            }
            "FactSet" => {
                if let Some(facts) = element.get("facts").and_then(Value::as_array) {
                    for fact in facts {
                        let fact_title = fact.get("title").and_then(Value::as_str).unwrap_or("");
                        let fact_value = fact.get("value").and_then(Value::as_str).unwrap_or("");
                        if !fact_title.is_empty() || !fact_value.is_empty() {
                            text_parts.push(format!("{fact_title}: {fact_value}"));
                        }
                    }
                }
            }
            _ => {}
        }
    }
}

fn extract_actions(action_list: &[Value], actions: &mut Vec<PlannerAction>) {
    for action in action_list {
        let title = action
            .get("title")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string();
        if title.is_empty() {
            continue;
        }
        let url = match action
            .get("type")
            .and_then(Value::as_str)
            .unwrap_or_default()
        {
            "Action.OpenUrl" => action.get("url").and_then(Value::as_str).map(String::from),
            _ => None,
        };
        actions.push(PlannerAction { title, url });
    }
}

fn is_title_textblock(element: &Value) -> bool {
    if let Some(weight) = element.get("weight").and_then(Value::as_str)
        && weight.eq_ignore_ascii_case("bolder")
    {
        return true;
    }
    if let Some(size) = element.get("size").and_then(Value::as_str) {
        match size.to_ascii_lowercase().as_str() {
            "large" | "extralarge" | "medium" => return true,
            _ => {}
        }
    }
    if let Some(style) = element.get("style").and_then(Value::as_str)
        && style.eq_ignore_ascii_case("heading")
    {
        return true;
    }
    false
}

/// Produce a deterministic render plan from a card and capabilities.
pub fn plan_render(
    card: &PlannerCard,
    caps: &PlannerCapabilities,
    ac_json: Option<&Value>,
) -> RenderPlan {
    let tier = select_tier(caps, ac_json.is_some());
    let mut warnings = Vec::new();
    let mut items = Vec::new();

    match tier {
        RenderTier::TierA | RenderTier::TierB => {
            if let Some(text) = build_summary_text(card, caps, &mut warnings) {
                items.push(RenderItem::Text(text));
            }
            if let Some(ac) = ac_json {
                items.push(RenderItem::AdaptiveCard(ac.clone()));
            }
            if tier == RenderTier::TierB && has_unsupported_elements(card, caps) {
                warnings.push(RenderWarning {
                    code: "unsupported_elements_removed".into(),
                    message: Some("Some card elements were removed for this channel".into()),
                    path: None,
                });
            }
        }
        RenderTier::TierC | RenderTier::TierD => {
            if let Some(text) = build_summary_text(card, caps, &mut warnings) {
                items.push(RenderItem::Text(text));
            }
            if ac_json.is_some() {
                warnings.push(RenderWarning {
                    code: "adaptive_card_downsampled".into(),
                    message: Some("Adaptive Card was converted to text for this channel".into()),
                    path: None,
                });
            }
        }
    }

    let summary_text = items.iter().find_map(|item| match item {
        RenderItem::Text(text) => Some(text.clone()),
        RenderItem::AdaptiveCard(_) => None,
    });

    RenderPlan {
        tier,
        summary_text,
        items,
        warnings,
        debug: None,
    }
}

fn select_tier(caps: &PlannerCapabilities, has_ac: bool) -> RenderTier {
    if !has_ac {
        return RenderTier::TierD;
    }
    if caps.supports_adaptive_cards {
        if caps.supports_buttons && caps.supports_images {
            RenderTier::TierA
        } else {
            RenderTier::TierB
        }
    } else {
        RenderTier::TierD
    }
}

fn build_summary_text(
    card: &PlannerCard,
    caps: &PlannerCapabilities,
    warnings: &mut Vec<RenderWarning>,
) -> Option<String> {
    let mut parts = Vec::new();

    if let Some(title) = &card.title {
        let title = title.trim();
        if !title.is_empty() {
            parts.push(title.to_string());
        }
    }
    if let Some(text) = &card.text {
        let text = text.trim();
        if !text.is_empty() {
            parts.push(text.to_string());
        }
    }
    if !caps.supports_buttons && !card.actions.is_empty() {
        parts.push(
            card.actions
                .iter()
                .map(|action| {
                    if let Some(url) = &action.url {
                        format!("[{}]({})", action.title, url)
                    } else {
                        action.title.clone()
                    }
                })
                .collect::<Vec<_>>()
                .join(" | "),
        );
    }

    if parts.is_empty() {
        return None;
    }

    let mut text = parts.join("\n\n");
    let (sanitized, did_sanitize) = sanitize_text(&text, caps);
    text = sanitized;
    if did_sanitize {
        warnings.push(RenderWarning {
            code: "text_sanitized".into(),
            message: Some("Text was sanitized for this channel".into()),
            path: None,
        });
    }
    if let Some(max_len) = caps.max_text_len {
        let (truncated, did_truncate) = truncate_chars(&text, max_len as usize);
        text = truncated;
        if did_truncate {
            warnings.push(RenderWarning {
                code: "text_truncated".into(),
                message: Some(format!("Text truncated to {max_len} chars")),
                path: None,
            });
        }
    }
    if let Some(max_bytes) = caps.max_payload_bytes {
        let (truncated, did_truncate) = truncate_bytes(&text, max_bytes as usize);
        text = truncated;
        if did_truncate {
            warnings.push(RenderWarning {
                code: "payload_truncated".into(),
                message: Some(format!("Payload truncated to {max_bytes} bytes")),
                path: None,
            });
        }
    }

    Some(text)
}

fn sanitize_text(text: &str, caps: &PlannerCapabilities) -> (String, bool) {
    let mut result = text.to_string();
    let mut changed = false;

    if !caps.supports_html {
        let stripped = strip_html_tags(&result);
        if stripped != result {
            changed = true;
            result = stripped;
        }
    }
    if !caps.supports_markdown {
        let stripped = strip_markdown_markers(&result);
        if stripped != result {
            changed = true;
            result = stripped;
        }
    }

    (result, changed)
}

fn strip_html_tags(text: &str) -> String {
    let mut result = String::with_capacity(text.len());
    let mut in_tag = false;
    for ch in text.chars() {
        match ch {
            '<' => in_tag = true,
            '>' => in_tag = false,
            _ if !in_tag => result.push(ch),
            _ => {}
        }
    }
    result
}

fn strip_markdown_markers(text: &str) -> String {
    text.replace("**", "")
        .replace("__", "")
        .replace("~~", "")
        .replace('`', "")
}

/// Truncate to at most `max` characters, appending ellipsis if truncated.
pub fn truncate_chars(text: &str, max: usize) -> (String, bool) {
    if max == 0 {
        return (String::new(), !text.is_empty());
    }
    if text.chars().count() <= max {
        return (text.to_string(), false);
    }
    let truncated: String = text.chars().take(max.saturating_sub(1)).collect();
    (format!("{truncated}\u{2026}"), true)
}

/// Truncate to at most `max` bytes on a char boundary, appending ellipsis.
pub fn truncate_bytes(text: &str, max: usize) -> (String, bool) {
    if text.len() <= max {
        return (text.to_string(), false);
    }
    if max < 4 {
        return (String::new(), true);
    }
    let boundary = max - 3;
    let end = text
        .char_indices()
        .take_while(|(index, _)| *index <= boundary)
        .last()
        .map(|(index, ch)| index + ch.len_utf8())
        .unwrap_or(0);
    (format!("{}\u{2026}", &text[..end]), true)
}

fn has_unsupported_elements(card: &PlannerCard, caps: &PlannerCapabilities) -> bool {
    (!caps.supports_buttons && !card.actions.is_empty())
        || (!caps.supports_images && !card.images.is_empty())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn extracts_title_text_actions_and_images() {
        let card = extract_planner_card(&json!({
            "type": "AdaptiveCard",
            "body": [
                {"type": "TextBlock", "text": "Title", "weight": "Bolder"},
                {"type": "TextBlock", "text": "Body"},
                {"type": "Image", "url": "https://example.com/a.png"}
            ],
            "actions": [{"type": "Action.OpenUrl", "title": "Open", "url": "https://example.com"}]
        }));

        assert_eq!(card.title.as_deref(), Some("Title"));
        assert_eq!(card.text.as_deref(), Some("Body"));
        assert_eq!(card.images, vec!["https://example.com/a.png"]);
        assert_eq!(card.actions[0].title, "Open");
    }

    #[test]
    fn capabilities_are_available_for_known_providers() {
        assert!(capabilities_for("slack").is_some_and(|caps| caps.supports_markdown));
        assert!(capabilities_for("unknown").is_none());
    }

    #[test]
    fn plans_adaptive_card_passthrough_when_supported() {
        let caps = PlannerCapabilities {
            supports_adaptive_cards: true,
            supports_images: true,
            supports_buttons: true,
            ..Default::default()
        };
        let card = PlannerCard {
            title: Some("Title".into()),
            text: Some("Body".into()),
            ..Default::default()
        };
        let ac = json!({"type": "AdaptiveCard"});
        let plan = plan_render(&card, &caps, Some(&ac));

        assert_eq!(plan.tier, RenderTier::TierA);
        assert!(
            plan.items
                .iter()
                .any(|item| matches!(item, RenderItem::AdaptiveCard(_)))
        );
    }

    #[test]
    fn plans_adaptive_card_downsample_when_not_supported() {
        let card = PlannerCard {
            title: Some("Title".into()),
            text: Some("Body".into()),
            ..Default::default()
        };
        let plan = plan_render(&card, &PlannerCapabilities::default(), Some(&json!({})));

        assert_eq!(plan.tier, RenderTier::TierD);
        assert!(
            plan.warnings
                .iter()
                .any(|warning| warning.code == "adaptive_card_downsampled")
        );
    }
}
