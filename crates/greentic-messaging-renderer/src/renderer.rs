use crate::{
    ac_extract::extract_planner_card,
    context::RenderContext,
    mode::RendererMode,
    plan::{RenderItem, RenderPlan, RenderTier},
    planner::{PlannerCapabilities, plan_render},
};
use greentic_types::ChannelMessageEnvelope;
use serde_json::{Value, json};

/// Resolve an Adaptive Card from message metadata.
fn resolve_adaptive_card(envelope: &ChannelMessageEnvelope) -> Option<Value> {
    envelope
        .metadata
        .get("adaptive_card")
        .and_then(|s| serde_json::from_str::<Value>(s).ok())
}

/// Trait describing a renderer that turns an envelope into a plan.
pub trait CardRenderer {
    fn render_plan(
        &self,
        envelope: &ChannelMessageEnvelope,
        context: &RenderContext,
        mode: RendererMode,
    ) -> RenderPlan;
}

/// No-op renderer that passes text and saved Adaptive Cards through unchanged.
#[derive(Debug, Default, Clone, Copy)]
pub struct NoopCardRenderer;

impl CardRenderer for NoopCardRenderer {
    fn render_plan(
        &self,
        envelope: &ChannelMessageEnvelope,
        context: &RenderContext,
        mode: RendererMode,
    ) -> RenderPlan {
        let summary_text = envelope
            .text
            .as_ref()
            .map(|value| value.trim().to_owned())
            .filter(|value| !value.is_empty());

        let mut items = Vec::new();
        if let Some(text) = summary_text.clone() {
            items.push(RenderItem::Text(text));
        }
        if let Some(card) = resolve_adaptive_card(envelope) {
            items.push(RenderItem::AdaptiveCard(card));
        }

        let debug = json!({
            "mode": format!("{:?}", mode),
            "target": context.target.clone(),
        });

        RenderPlan {
            tier: RenderTier::TierA,
            summary_text,
            items,
            warnings: Vec::new(),
            debug: Some(debug),
        }
    }
}

/// Convenience helper that builds a plan using the no-op renderer.
pub fn render_plan_from_envelope(
    envelope: &ChannelMessageEnvelope,
    context: &RenderContext,
    mode: RendererMode,
) -> RenderPlan {
    NoopCardRenderer.render_plan(envelope, context, mode)
}

/// Renderer that applies deterministic downsampling based on channel capabilities.
pub struct DownsampleCardRenderer {
    pub capabilities: PlannerCapabilities,
}

impl CardRenderer for DownsampleCardRenderer {
    fn render_plan(
        &self,
        envelope: &ChannelMessageEnvelope,
        context: &RenderContext,
        mode: RendererMode,
    ) -> RenderPlan {
        // In Passthrough mode, delegate to NoopCardRenderer
        if mode == RendererMode::Passthrough {
            return NoopCardRenderer.render_plan(envelope, context, mode);
        }

        let ac = resolve_adaptive_card(envelope);

        match ac {
            Some(ac_value) => {
                let card = extract_planner_card(&ac_value);
                let ac_ref = if self.capabilities.supports_adaptive_cards {
                    Some(&ac_value)
                } else {
                    None
                };
                let mut plan = plan_render(&card, &self.capabilities, ac_ref);
                // If AC-capable but planner didn't include the card (shouldn't happen for TierA/B),
                // ensure it's included
                if self.capabilities.supports_adaptive_cards
                    && !plan
                        .items
                        .iter()
                        .any(|i| matches!(i, RenderItem::AdaptiveCard(_)))
                {
                    plan.items.push(RenderItem::AdaptiveCard(ac_value));
                }
                plan
            }
            None => {
                // No AC present - text-only plan
                let summary_text = envelope
                    .text
                    .as_ref()
                    .map(|v| v.trim().to_owned())
                    .filter(|v| !v.is_empty());

                let mut items = Vec::new();
                if let Some(text) = summary_text.clone() {
                    items.push(RenderItem::Text(text));
                }

                RenderPlan {
                    tier: RenderTier::TierD,
                    summary_text,
                    items,
                    warnings: Vec::new(),
                    debug: None,
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use greentic_types::{ChannelMessageEnvelope, EnvId, MessageMetadata, TenantCtx, TenantId};

    fn envelope(text: Option<&str>, adaptive_card: Option<Value>) -> ChannelMessageEnvelope {
        let mut metadata = MessageMetadata::new();
        if let Some(card) = adaptive_card {
            metadata.insert("adaptive_card".to_string(), card.to_string());
        }
        ChannelMessageEnvelope {
            id: "msg-1".to_string(),
            tenant: TenantCtx::new(
                EnvId::try_from("dev").expect("env"),
                TenantId::try_from("tenant").expect("tenant"),
            ),
            channel: "test".to_string(),
            session_id: "session-1".to_string(),
            reply_scope: None,
            from: None,
            to: Vec::new(),
            correlation_id: None,
            text: text.map(str::to_string),
            attachments: Vec::new(),
            metadata,
            extensions: Default::default(),
        }
    }

    #[test]
    fn noop_renderer_passes_trimmed_text_and_adaptive_card() {
        let card = json!({
            "type": "AdaptiveCard",
            "body": [{"type": "TextBlock", "text": "Card text"}]
        });
        let context = RenderContext::new(Some("telegram".to_string()));
        let plan = NoopCardRenderer.render_plan(
            &envelope(Some("  hello  "), Some(card.clone())),
            &context,
            RendererMode::Passthrough,
        );

        assert_eq!(plan.tier, RenderTier::TierA);
        assert_eq!(plan.summary_text.as_deref(), Some("hello"));
        assert!(matches!(&plan.items[0], RenderItem::Text(text) if text == "hello"));
        assert!(matches!(&plan.items[1], RenderItem::AdaptiveCard(value) if value == &card));
        assert_eq!(plan.debug.as_ref().unwrap()["target"], "telegram");
    }

    #[test]
    fn downsample_renderer_without_card_returns_text_only_tier_d() {
        let renderer = DownsampleCardRenderer {
            capabilities: PlannerCapabilities::default(),
        };
        let plan = renderer.render_plan(
            &envelope(Some(" fallback "), None),
            &RenderContext::default(),
            RendererMode::Downsample,
        );

        assert_eq!(plan.tier, RenderTier::TierD);
        assert_eq!(plan.summary_text.as_deref(), Some("fallback"));
        assert_eq!(plan.items, vec![RenderItem::Text("fallback".to_string())]);
        assert!(plan.debug.is_none());
    }

    #[test]
    fn downsample_renderer_passthrough_mode_uses_noop_behavior() {
        let card = json!({"type": "AdaptiveCard", "body": []});
        let renderer = DownsampleCardRenderer {
            capabilities: PlannerCapabilities::default(),
        };
        let plan = renderer.render_plan(
            &envelope(None, Some(card.clone())),
            &RenderContext::default(),
            RendererMode::Passthrough,
        );

        assert_eq!(plan.tier, RenderTier::TierA);
        assert!(matches!(&plan.items[0], RenderItem::AdaptiveCard(value) if value == &card));
    }

    #[test]
    fn helper_uses_noop_renderer() {
        let plan = render_plan_from_envelope(
            &envelope(Some("hello"), None),
            &RenderContext::default(),
            RendererMode::Passthrough,
        );

        assert_eq!(plan.tier, RenderTier::TierA);
        assert_eq!(plan.summary_text.as_deref(), Some("hello"));
    }
}
