use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct MessageCard(pub Value);

impl MessageCard {
    pub fn as_value(&self) -> &Value {
        &self.0
    }
}

impl From<Value> for MessageCard {
    fn from(value: Value) -> Self {
        Self(value)
    }
}

impl From<MessageCard> for Value {
    fn from(card: MessageCard) -> Self {
        card.0
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum Tier {
    #[default]
    Basic,
    Advanced,
    Premium,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct CapabilityProfile {
    pub allow_images: bool,
    pub allow_factset: bool,
    pub allow_inputs: bool,
    pub allow_postbacks: bool,
}

impl CapabilityProfile {
    pub fn for_tier(tier: Tier) -> Self {
        match tier {
            Tier::Premium => Self {
                allow_images: true,
                allow_factset: true,
                allow_inputs: true,
                allow_postbacks: true,
            },
            Tier::Advanced => Self {
                allow_images: true,
                allow_factset: true,
                allow_inputs: true,
                allow_postbacks: false,
            },
            Tier::Basic => Self {
                allow_images: false,
                allow_factset: false,
                allow_inputs: false,
                allow_postbacks: false,
            },
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum RenderIntent {
    Card,
    Text,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum MessageCardKind {
    Standard,
}

#[derive(Clone, Debug)]
pub struct AuthRenderSpec {
    pub kind: MessageCardKind,
}

impl AuthRenderSpec {
    pub fn new(kind: MessageCardKind) -> Self {
        Self { kind }
    }
}

#[derive(Clone, Debug)]
pub struct RenderSpec {
    pub intent: RenderIntent,
    pub card: MessageCard,
    pub kind: MessageCardKind,
}

impl RenderSpec {
    pub fn card(card: MessageCard) -> Self {
        Self {
            intent: RenderIntent::Card,
            kind: MessageCardKind::Standard,
            card,
        }
    }
}

#[derive(Clone, Debug)]
pub struct RenderOutput {
    pub payload: Value,
    pub warnings: Vec<String>,
    pub used_modal: bool,
    pub limit_exceeded: bool,
    pub sanitized_count: usize,
    pub url_blocked_count: usize,
}

#[derive(Clone, Debug)]
pub struct RenderSnapshot {
    pub tier: Tier,
    pub target_tier: Tier,
    pub downgraded: bool,
    pub output: RenderOutput,
}

#[derive(Clone, Debug)]
pub struct PlatformPreview {
    pub payload: Value,
    pub tier: Tier,
    pub target_tier: Tier,
    pub downgraded: bool,
    pub used_modal: bool,
    pub limit_exceeded: bool,
    pub sanitized_count: usize,
    pub url_blocked_count: usize,
    pub warnings: Vec<String>,
}

#[derive(Clone, Debug)]
pub struct RenderResponse {
    pub intent: RenderIntent,
    pub payload: Value,
    pub preview: PlatformPreview,
    pub warnings: Vec<String>,
    pub downgraded: bool,
    pub capability: Option<CapabilityProfile>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn message_card_wraps_and_unwraps_json_values() {
        let value = json!({"type": "AdaptiveCard", "body": []});
        let card = MessageCard::from(value.clone());

        assert_eq!(card.as_value(), &value);
        let unwrapped: Value = card.into();
        assert_eq!(unwrapped, value);
    }

    #[test]
    fn tiers_serialize_and_default_to_basic() {
        assert_eq!(Tier::default(), Tier::Basic);
        assert_eq!(
            serde_json::to_value(Tier::Advanced).unwrap(),
            json!("Advanced")
        );
        assert_eq!(
            serde_json::from_value::<Tier>(json!("Premium")).unwrap(),
            Tier::Premium
        );
    }

    #[test]
    fn capability_profile_toggles_by_tier() {
        let basic = CapabilityProfile::for_tier(Tier::Basic);
        assert!(!basic.allow_images);
        assert!(!basic.allow_factset);
        assert!(!basic.allow_inputs);
        assert!(!basic.allow_postbacks);

        let advanced = CapabilityProfile::for_tier(Tier::Advanced);
        assert!(advanced.allow_images);
        assert!(advanced.allow_factset);
        assert!(advanced.allow_inputs);
        assert!(!advanced.allow_postbacks);

        let premium = CapabilityProfile::for_tier(Tier::Premium);
        assert!(premium.allow_postbacks);
    }

    #[test]
    fn render_specs_capture_intent_and_kind() {
        let card = MessageCard::from(json!({"hello": "world"}));
        let auth = AuthRenderSpec::new(MessageCardKind::Standard);
        let spec = RenderSpec::card(card.clone());

        assert_eq!(auth.kind, MessageCardKind::Standard);
        assert_eq!(spec.intent, RenderIntent::Card);
        assert_eq!(spec.kind, MessageCardKind::Standard);
        assert_eq!(spec.card, card);
    }

    #[test]
    fn render_response_structs_hold_preview_and_output_metadata() {
        let output = RenderOutput {
            payload: json!({"text": "hello"}),
            warnings: vec!["trimmed".to_string()],
            used_modal: true,
            limit_exceeded: false,
            sanitized_count: 1,
            url_blocked_count: 2,
        };
        let snapshot = RenderSnapshot {
            tier: Tier::Basic,
            target_tier: Tier::Premium,
            downgraded: true,
            output: output.clone(),
        };
        let preview = PlatformPreview {
            payload: output.payload.clone(),
            tier: snapshot.tier,
            target_tier: snapshot.target_tier,
            downgraded: snapshot.downgraded,
            used_modal: output.used_modal,
            limit_exceeded: output.limit_exceeded,
            sanitized_count: output.sanitized_count,
            url_blocked_count: output.url_blocked_count,
            warnings: output.warnings.clone(),
        };
        let response = RenderResponse {
            intent: RenderIntent::Text,
            payload: preview.payload.clone(),
            preview,
            warnings: output.warnings,
            downgraded: true,
            capability: Some(CapabilityProfile::for_tier(Tier::Basic)),
        };

        assert_eq!(response.intent, RenderIntent::Text);
        assert_eq!(response.payload["text"], "hello");
        assert!(response.preview.used_modal);
        assert_eq!(response.preview.url_blocked_count, 2);
        assert_eq!(response.warnings, vec!["trimmed"]);
        assert!(response.downgraded);
        assert_eq!(
            response.capability.as_ref().map(|cap| cap.allow_images),
            Some(false)
        );
    }
}
