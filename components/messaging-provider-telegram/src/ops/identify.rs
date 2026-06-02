//! `identify-instance` export — extracts the bot's shared secret from
//! the `x-telegram-bot-api-secret-token` HTTP header so the host can
//! route the inbound to the right `MessagingEndpoint` when multiple
//! Telegram bots share one runtime.

use serde_json::Value;

const SECRET_TOKEN_HEADER: &str = "x-telegram-bot-api-secret-token";

/// **Routing discriminator only — not an authentication check.**
///
/// Telegram's webhook update body carries the chat and sender but does
/// not identify the receiving bot — by Telegram's design, the bot is
/// implicit in the webhook URL the operator registered via
/// `setWebhook`. The only payload surface that carries a per-bot
/// discriminator is the `X-Telegram-Bot-Api-Secret-Token` HTTP header,
/// which Telegram echoes on every inbound when the operator configured
/// a `secret_token` at `setWebhook` time.
///
/// The convention the host expects: the operator declares
/// `MessagingEndpoint.provider_id = <the same secret token>`. At
/// inbound time, the host wraps the request as `{headers, body}` (M1
/// IID.4d wrapper, see
/// `greentic:provider-instance-identity@0.1.0/identify-instance`),
/// this function reads the header value, and the host folds it against
/// the env's `(provider_type, provider_id) → endpoint_id` table.
///
/// **Auth is downstream.** The secret token is a shared secret known to
/// both operator and Telegram. Using it as a routing discriminator is
/// safe ONLY because the verifying webhook receiver (downstream of this
/// component) rejects any inbound whose header does not match the
/// expected token. Without that downstream check, anyone who learns the
/// token can address that endpoint.
///
/// # Input shape (M1 IID.4d wrapper)
///
/// Accepts the wrapper shape:
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
/// - the input is not JSON
/// - the wrapper has no `headers` array
/// - no `x-telegram-bot-api-secret-token` header is present (legacy
///   bare-body or single-instance setup without `secret_token`
///   configured)
/// - the header value is empty (matching `Some("")` against the admit
///   table is a false-positive trap)
///
/// `None` falls back to single-instance behavior at the host: if the
/// env declares only one Telegram endpoint, the request still admits
/// via the operator's static `provider_id`; if ≥ 2 are declared, the
/// resolver poisons to `Ambiguous` and the request is refused 422
/// (the correct fail-closed outcome — without a discriminator we
/// cannot route).
pub(crate) fn extract_secret_token(input_json: &[u8]) -> Option<String> {
    let value: Value = serde_json::from_slice(input_json).ok()?;
    secret_token_from_headers(&value)
}

fn secret_token_from_headers(value: &Value) -> Option<String> {
    let headers = value.get("headers")?.as_array()?;
    headers
        .iter()
        .filter_map(|header| {
            let name = header.get("name").and_then(Value::as_str)?;
            if !name.eq_ignore_ascii_case(SECRET_TOKEN_HEADER) {
                return None;
            }
            let raw = header.get("value").and_then(Value::as_str)?;
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
    fn returns_none_when_header_value_empty() {
        let bytes = wrapper(
            json!([
                { "name": "x-telegram-bot-api-secret-token", "value": "" }
            ]),
            telegram_update(),
        );
        // Some("") would match an empty admit-table entry — never do
        // that. Empty header → None → single-instance fallback.
        assert!(extract_secret_token(&bytes).is_none());
    }

    #[test]
    fn returns_none_when_header_value_only_whitespace() {
        let bytes = wrapper(
            json!([
                { "name": "x-telegram-bot-api-secret-token", "value": "   " }
            ]),
            telegram_update(),
        );
        assert!(extract_secret_token(&bytes).is_none());
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
