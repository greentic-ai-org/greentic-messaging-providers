use serde::{Deserialize, Serialize};
use serde_json::Value;

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

/// Items produced by the renderer.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum RenderItem {
    Text(String),
    AdaptiveCard(Value),
}

/// A render plan produced from a channel message envelope.
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

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn render_plan_default_is_empty_tier_d() {
        let plan = RenderPlan::default();

        assert_eq!(plan.tier, RenderTier::TierD);
        assert!(plan.summary_text.is_none());
        assert!(plan.items.is_empty());
        assert!(plan.warnings.is_empty());
        assert!(plan.debug.is_none());
    }

    #[test]
    fn render_plan_serializes_without_empty_optional_fields() {
        let plan = RenderPlan {
            tier: RenderTier::TierA,
            summary_text: Some("hello".to_string()),
            items: vec![
                RenderItem::Text("hello".to_string()),
                RenderItem::AdaptiveCard(json!({"type": "AdaptiveCard"})),
            ],
            warnings: vec![RenderWarning {
                code: "notice".to_string(),
                message: None,
                path: Some("$.body[0]".to_string()),
            }],
            debug: None,
        };

        let value = serde_json::to_value(&plan).expect("plan json");

        assert_eq!(value["tier"], "tier_a");
        assert_eq!(value["summary_text"], "hello");
        assert_eq!(value["items"][0]["Text"], "hello");
        assert!(value.get("debug").is_none());
        assert!(value["warnings"][0].get("message").is_none());
        assert_eq!(value["warnings"][0]["path"], "$.body[0]");
    }

    #[test]
    fn render_tier_round_trips_snake_case() {
        let tier: RenderTier = serde_json::from_value(json!("tier_c")).expect("tier");

        assert_eq!(tier, RenderTier::TierC);
        assert_eq!(
            serde_json::to_value(tier).expect("tier json"),
            json!("tier_c")
        );
    }
}
