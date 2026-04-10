//! Generic contract for "Adaptive Card → provider-native payload" converters.
//!
//! Each messaging provider that downsamples or transforms an Adaptive Card
//! into its own native representation (Slack Block Kit, Telegram HTML +
//! inline keyboards, WhatsApp interactive messages, e-mail HTML, etc.)
//! should implement this trait. Implementations stay focused on a single
//! responsibility — the rendering pipeline (`render_plan_common`) and the
//! encode step can stay generic over `AdaptiveCardConverter`.

use greentic_messaging_renderer::PlannerCapabilities;
use serde_json::Value;

use crate::ProviderError;

/// Converts an Adaptive Card JSON value into a provider-native payload.
///
/// Implementations are expected to be **pure** (no I/O, no global state) so
/// they can be invoked from inside WASM components and unit-tested in
/// isolation. Information loss (truncation, dropped elements, downgrades)
/// should be communicated through the renderer's `RenderWarning` mechanism
/// upstream — converters themselves should focus on the transform.
pub trait AdaptiveCardConverter {
    /// Provider-specific output (e.g. Slack `SlackBlocksResult`,
    /// Telegram `TelegramAcContent`, an HTML `String`, etc.).
    type Output;

    /// Converts the given Adaptive Card.
    ///
    /// `caps` is the channel capability matrix for the target provider —
    /// converters may consult it (for instance to honour `max_text_len` or
    /// to skip features the channel cannot render).
    fn convert(
        &self,
        adaptive_card: &Value,
        caps: &PlannerCapabilities,
    ) -> Result<Self::Output, ProviderError>;

    /// Provider name (lowercase, e.g. `"slack"`, `"telegram"`).
    /// Used by diagnostics and registry lookups.
    fn provider_name(&self) -> &'static str;
}
