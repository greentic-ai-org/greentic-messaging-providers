//! Shared telemetry helpers for provider components and crates.
//!
//! Wraps [`greentic_telemetry::wasm_guest`] with conveniences that suit how
//! the messaging providers report work:
//!
//! - [`Span`] — RAII guard that pairs `span_start` / `span_end`, never
//!   forgotten on an early return.
//! - Re-exports of [`Level`], [`Field`], [`log`] so call sites only depend on
//!   `provider_common::telemetry`.
//! - Constants for the structured field names we use everywhere so spelling
//!   stays consistent across providers (`provider`, `tenant`, `event_kind`).
//!
//! System events (e.g. webhook received, signature verified, message
//! delivered) are modelled as spans with a distinct `event_kind` field plus
//! `log()` events inside, so downstream collectors can pivot on either the
//! span name or the kind tag.

pub use greentic_telemetry::wasm_guest::{Field, Level, log, span_end, span_start};

use std::panic::Location;
use std::sync::Once;

/// One-shot identity + emission-config registration for the calling
/// component.
///
/// Each provider component embeds its own copy of `provider_common`, so the
/// underlying `Once` is per-component; the first telemetry call from a
/// component registers its `PROVIDER_TYPE` with
/// [`greentic_telemetry::wasm_guest::set_component_name`] so every emitted
/// fallback line carries an explicit `[<provider>]` prefix. Subsequent calls
/// are no-ops.
///
/// The same init reads three wasi env vars (set by the runner / gtc):
///
/// - `GREENTIC_TELEMETRY_FILE_LINE` (`0|1`, `on|off`, `true|false`,
///   `yes|no`) toggles the `file:line` segment on each emitted line. Default
///   `on`.
/// - `GREENTIC_TELEMETRY_LEVEL` (`trace|debug|info|warn|error`,
///   case-insensitive) sets a hard floor for emission at the source. Events
///   below the floor short-circuit before any formatting. Default `trace`
///   (no filtering).
fn init_identity_once(provider: &str) {
    static ONCE: Once = Once::new();
    ONCE.call_once(|| {
        greentic_telemetry::wasm_guest::set_component_name(provider);
        if let Ok(val) = std::env::var("GREENTIC_TELEMETRY_FILE_LINE") {
            let on = matches!(
                val.as_str(),
                "1" | "true" | "TRUE" | "on" | "ON" | "yes" | "YES"
            );
            greentic_telemetry::wasm_guest::set_caller_location_enabled(on);
        }
        if let Ok(val) = std::env::var("GREENTIC_TELEMETRY_LEVEL")
            && let Some(level) = parse_level(&val)
        {
            greentic_telemetry::wasm_guest::set_min_level(level);
        }
    });
}

fn parse_level(value: &str) -> Option<Level> {
    let lower = value.trim().to_ascii_lowercase();
    Some(match lower.as_str() {
        "trace" => Level::Trace,
        "debug" => Level::Debug,
        "info" => Level::Info,
        "warn" | "warning" => Level::Warn,
        "error" | "err" => Level::Error,
        _ => return None,
    })
}

/// Canonical field keys. Use these constants so all providers tag events with
/// the same names — collectors index on them.
pub mod field {
    pub const PROVIDER: &str = "provider";
    pub const TENANT: &str = "tenant";
    pub const FLOW: &str = "flow";
    pub const NODE: &str = "node";
    pub const EVENT_KIND: &str = "event_kind";
    pub const STEP: &str = "step";
    pub const MESSAGE_ID: &str = "message_id";
    pub const CONVERSATION_ID: &str = "conversation_id";
    pub const ROOM_ID: &str = "room_id";
    pub const USER: &str = "user";
    pub const HTTP_STATUS: &str = "http_status";
    pub const HTTP_METHOD: &str = "http_method";
    pub const HTTP_HOST: &str = "http_host";
    pub const ERROR_KIND: &str = "error_kind";
    pub const ERROR: &str = "error";
    pub const DURATION_MS: &str = "duration_ms";
    pub const RESULT: &str = "result";
    pub const TIER: &str = "tier";
    pub const BODY: &str = "body";
    pub const SECRET: &str = "secret";
}

/// Canonical event kinds. Spans/events tagged with these are stable contract
/// for dashboards and alerting.
pub mod event {
    pub const SEND_PAYLOAD: &str = "send_payload";
    pub const RENDER_PLAN: &str = "render_plan";
    pub const ENCODE: &str = "encode";
    pub const INGEST_HTTP: &str = "ingest_http";
    pub const SETUP_WEBHOOK: &str = "setup_webhook";
    pub const WEBHOOK_VERIFIED: &str = "webhook_verified";
    pub const WEBHOOK_REJECTED: &str = "webhook_rejected";
    pub const OAUTH_REFRESH: &str = "oauth_refresh";
    pub const SECRET_FETCH: &str = "secret_fetch";
    pub const DOWNSTREAM_CALL: &str = "downstream_call";
    pub const DOWNSTREAM_ERROR: &str = "downstream_error";
    pub const MESSAGE_DELIVERED: &str = "message_delivered";
    pub const MESSAGE_REJECTED: &str = "message_rejected";
}

/// RAII span guard. Drop ends the span (works correctly even on `?` early
/// returns or panics).
///
/// Built on
/// [`greentic_telemetry::wasm_guest::span_start_at`] /
/// [`greentic_telemetry::wasm_guest::span_end_at`]. The caller's
/// `Location` is captured once at `enter()` time and stored on the guard
/// so that the end line emitted from `Drop` attributes back to the source
/// line that opened the span, not to this wrapper or `Drop::drop`.
#[must_use = "the span ends when the guard drops; bind it to a local"]
pub struct Span {
    id: u64,
    caller: &'static Location<'static>,
}

impl Span {
    /// Start a span named after a canonical event kind, attaching the
    /// provider identifier as a field.
    ///
    /// `#[track_caller]` captures the caller's `Location` once at this
    /// call site; the same `Location` is reused by `Drop` so the matching
    /// end line attributes to the same source point.
    #[track_caller]
    pub fn enter(event_kind: &str, provider: &str, extra: &[Field<'_>]) -> Self {
        init_identity_once(provider);
        let caller = Location::caller();
        let mut fields: Vec<Field<'_>> = Vec::with_capacity(extra.len() + 2);
        fields.push(Field {
            key: field::EVENT_KIND,
            value: event_kind,
        });
        fields.push(Field {
            key: field::PROVIDER,
            value: provider,
        });
        fields.extend_from_slice(extra);
        let id = greentic_telemetry::wasm_guest::span_start_at(event_kind, &fields, caller);
        Span { id, caller }
    }

    /// Record a structured event inside the span without ending it.
    #[track_caller]
    pub fn event(&self, level: Level, message: &str, fields: &[Field<'_>]) {
        log(level, message, fields);
    }
}

impl Drop for Span {
    fn drop(&mut self) {
        // Use the caller stored at `enter()` so the end line points at the
        // source location that opened the span, not at `Drop::drop`.
        greentic_telemetry::wasm_guest::span_end_at(self.id, self.caller);
    }
}

/// Emit a structured log line tagged with the provider name.
///
/// `#[track_caller]` so emitted lines carry the caller's `file:line`, not
/// this wrapper.
#[track_caller]
pub fn emit(level: Level, provider: &str, message: &str, extra: &[Field<'_>]) {
    init_identity_once(provider);
    let mut fields: Vec<Field<'_>> = Vec::with_capacity(extra.len() + 1);
    fields.push(Field {
        key: field::PROVIDER,
        value: provider,
    });
    fields.extend_from_slice(extra);
    log(level, message, &fields);
}

/// Convenience: log a downstream HTTP error with the response body redacted.
///
/// `endpoint` should be the bare host or host+path (never the full URL with
/// query string, since query strings sometimes carry tokens). `body` is run
/// through [`crate::redact::response_snippet`] before reaching the log layer.
#[track_caller]
pub fn downstream_error(provider: &str, endpoint: &str, status: u16, body: &str) {
    init_identity_once(provider);
    let status_str = status.to_string();
    let snippet = crate::redact::response_snippet(body);
    let fields = [
        Field {
            key: field::EVENT_KIND,
            value: event::DOWNSTREAM_ERROR,
        },
        Field {
            key: field::PROVIDER,
            value: provider,
        },
        Field {
            key: field::HTTP_HOST,
            value: endpoint,
        },
        Field {
            key: field::HTTP_STATUS,
            value: &status_str,
        },
        Field {
            key: field::BODY,
            value: &snippet,
        },
    ];
    log(Level::Error, "downstream call failed", &fields);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn span_can_be_entered_and_dropped() {
        let span = Span::enter(event::SEND_PAYLOAD, "test-provider", &[]);
        span.event(Level::Info, "step ok", &[]);
        drop(span);
    }

    #[test]
    fn emit_does_not_panic_on_empty_fields() {
        emit(Level::Info, "test-provider", "hello", &[]);
    }

    #[test]
    fn downstream_error_redacts_body() {
        downstream_error(
            "test-provider",
            "api.example.com/v1/messages",
            500,
            r#"{"token":"abc123","error":"oops"}"#,
        );
    }
}
