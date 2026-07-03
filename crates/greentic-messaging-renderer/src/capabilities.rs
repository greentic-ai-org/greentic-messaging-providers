//! Centralized capability registry for known messaging providers.
//!
//! Single source of truth for which channels support Adaptive Cards
//! natively, which need downsampling, and what their text/payload limits are.
//! Providers should call `capabilities_for(name)` instead of hardcoding
//! `PlannerCapabilities` literals in their `render_plan` ops.

use crate::planner::PlannerCapabilities;

/// Returns the canonical capabilities for a known provider name,
/// or `None` if the name is not registered.
///
/// Known providers: `slack`, `teams`, `telegram`, `webex`,
/// `whatsapp`, `webchat`, `email`, `sms`.
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn known_providers_return_some() {
        for name in [
            "slack", "teams", "telegram", "webex", "whatsapp", "webchat", "email", "sms",
        ] {
            assert!(capabilities_for(name).is_some(), "missing: {name}");
        }
    }

    #[test]
    fn unknown_provider_returns_none() {
        assert!(capabilities_for("nonexistent").is_none());
    }

    #[test]
    fn ac_capable_providers_have_ac_support() {
        for name in ["teams", "webex", "webchat"] {
            let caps = capabilities_for(name).unwrap();
            assert!(caps.supports_adaptive_cards, "{name} should support AC");
        }
    }

    #[test]
    fn non_ac_providers_disable_ac() {
        for name in ["slack", "telegram", "whatsapp", "email", "sms"] {
            let caps = capabilities_for(name).unwrap();
            assert!(
                !caps.supports_adaptive_cards,
                "{name} should NOT support AC"
            );
        }
    }
}
