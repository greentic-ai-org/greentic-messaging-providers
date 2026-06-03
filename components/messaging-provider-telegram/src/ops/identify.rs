//! `identify-instance` export — extracts the bot's shared secret from
//! the `x-telegram-bot-api-secret-token` HTTP header so the host can
//! route the inbound to the right `MessagingEndpoint` when multiple
//! Telegram bots share one runtime.

use provider_common::http_compat::header_name_and_value;
use serde_json::Value;

const SECRET_TOKEN_HEADER: &str = "x-telegram-bot-api-secret-token";

/// **Routing discriminator only — not an authentication check.** See the
/// module doc for the host-routing context and the WIT contract at
/// `greentic:provider-instance-identity@0.1.0/identify-instance` for
/// the canonical semantics (including how `None` is folded against the
/// admit table). Downstream auth — Telegram webhook verification
/// rejecting any inbound whose header does not match the expected
/// token — MUST run before any event is admitted.
///
/// # Input shape (M1 IID.4d wrapper)
///
/// Accepts the wrapper shape. Header entries may be `{name, value}`
/// objects or `[name, value]` tuple arrays (some HTTP-compat layers use
/// the latter):
///
/// ```json
/// {
///   "headers": [
///     { "name": "x-telegram-bot-api-secret-token", "value": "tok-…" }
///   ],
///   "body": { /* Telegram update */ }
/// }
/// ```
///
/// Returns `None` when:
///
/// - the input is not JSON,
/// - the wrapper has no `headers` array (legacy bare body — Telegram's
///   body alone does not identify the receiving bot),
/// - no `x-telegram-bot-api-secret-token` header is present, or
/// - the header value is empty / whitespace-only (matching `Some("")`
///   against the admit table is a false-positive trap).
pub(crate) fn extract_secret_token(input_json: &[u8]) -> Option<String> {
    let value: Value = serde_json::from_slice(input_json).ok()?;
    secret_token_from_headers(&value)
}

fn secret_token_from_headers(value: &Value) -> Option<String> {
    let headers = value.get("headers")?.as_array()?;
    headers
        .iter()
        .filter_map(|header| {
            let (name, raw) = header_name_and_value(header)?;
            if !name.eq_ignore_ascii_case(SECRET_TOKEN_HEADER) {
                return None;
            }
            let trimmed = raw.trim();
            if trimmed.is_empty() {
                None
            } else {
                Some(trimmed.to_owned())
            }
        })
        .next()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn wrapper(headers: Value, body: Value) -> Vec<u8> {
        let payload = json!({ "headers": headers, "body": body });
        serde_json::to_vec(&payload).unwrap()
    }

    fn telegram_update() -> Value {
        json!({
            "update_id": 12345,
            "message": {
                "message_id": 1,
                "chat": { "id": 100, "type": "private" },
                "from": { "id": 200, "is_bot": false, "first_name": "Alice" },
                "text": "hi"
            }
        })
    }

    #[test]
    fn returns_secret_token_when_header_present() {
        let bytes = wrapper(
            json!([
                { "name": "x-forwarded-for", "value": "203.0.113.42" },
                { "name": "x-telegram-bot-api-secret-token", "value": "tok-legal-bot" }
            ]),
            telegram_update(),
        );
        assert_eq!(
            extract_secret_token(&bytes).as_deref(),
            Some("tok-legal-bot")
        );
    }

    #[test]
    fn header_lookup_is_case_insensitive() {
        // The host lowercases header names before wrapping, but a future
        // host or test harness might not — guard the contract.
        let bytes = wrapper(
            json!([
                { "name": "X-Telegram-Bot-Api-Secret-Token", "value": "tok-mixed-case" }
            ]),
            telegram_update(),
        );
        assert_eq!(
            extract_secret_token(&bytes).as_deref(),
            Some("tok-mixed-case")
        );
    }

    #[test]
    fn returns_first_match_when_header_duplicated() {
        // Telegram never sends the header twice, but a misconfigured proxy
        // might fan it out. Take the first match deterministically.
        let bytes = wrapper(
            json!([
                { "name": "x-telegram-bot-api-secret-token", "value": "first" },
                { "name": "x-telegram-bot-api-secret-token", "value": "second" }
            ]),
            telegram_update(),
        );
        assert_eq!(extract_secret_token(&bytes).as_deref(), Some("first"));
    }

    #[test]
    fn accepts_tuple_form_headers() {
        // Some operator HTTP-compat layers in the repo serialize headers
        // as `[name, value]` tuple arrays instead of `{name, value}`
        // objects. The Telegram identify-instance must accept both.
        let bytes = wrapper(
            json!([
                ["x-forwarded-for", "203.0.113.42"],
                ["x-telegram-bot-api-secret-token", "tok-tuple-form"]
            ]),
            telegram_update(),
        );
        assert_eq!(
            extract_secret_token(&bytes).as_deref(),
            Some("tok-tuple-form")
        );
    }

    #[test]
    fn returns_none_for_malformed_tuple_header() {
        // A 1-element tuple is neither shape — decline rather than panic.
        let bytes = wrapper(
            json!([["x-telegram-bot-api-secret-token"]]),
            telegram_update(),
        );
        assert!(extract_secret_token(&bytes).is_none());
    }

    #[test]
    fn trims_surrounding_whitespace() {
        let bytes = wrapper(
            json!([
                { "name": "x-telegram-bot-api-secret-token", "value": "  tok-with-spaces  " }
            ]),
            telegram_update(),
        );
        assert_eq!(
            extract_secret_token(&bytes).as_deref(),
            Some("tok-with-spaces")
        );
    }

    #[test]
    fn returns_none_when_header_missing() {
        let bytes = wrapper(
            json!([
                { "name": "x-forwarded-for", "value": "203.0.113.42" }
            ]),
            telegram_update(),
        );
        assert!(extract_secret_token(&bytes).is_none());
    }

    #[test]
    fn returns_none_when_header_value_empty_or_whitespace() {
        // Empty + whitespace both `trim()` to "" — same is_empty() branch.
        // Some("") would match an empty admit-table entry. Cover both as
        // a single test asserting the post-trim invariant.
        for value in ["", "   "] {
            let bytes = wrapper(
                json!([
                    { "name": "x-telegram-bot-api-secret-token", "value": value }
                ]),
                telegram_update(),
            );
            assert!(
                extract_secret_token(&bytes).is_none(),
                "expected None for value={value:?}"
            );
        }
    }

    #[test]
    fn returns_none_when_wrapper_has_no_headers_field() {
        // Legacy bare body, no wrapper. Telegram's body carries no
        // identification, so this MUST fall back to None.
        let bytes = serde_json::to_vec(&telegram_update()).unwrap();
        assert!(extract_secret_token(&bytes).is_none());
    }

    #[test]
    fn returns_none_when_headers_is_not_an_array() {
        // Defensive: a misshaped wrapper. Don't panic, just decline.
        let payload = json!({
            "headers": { "x-telegram-bot-api-secret-token": "tok" },
            "body": telegram_update(),
        });
        let bytes = serde_json::to_vec(&payload).unwrap();
        assert!(extract_secret_token(&bytes).is_none());
    }

    #[test]
    fn returns_none_when_header_entry_missing_name_or_value() {
        let bytes = wrapper(
            json!([
                { "name": "x-telegram-bot-api-secret-token" }, // no value
                { "value": "orphan-value" }                    // no name
            ]),
            telegram_update(),
        );
        assert!(extract_secret_token(&bytes).is_none());
    }

    #[test]
    fn returns_none_for_unparseable_input() {
        assert!(extract_secret_token(b"not json").is_none());
    }
}
