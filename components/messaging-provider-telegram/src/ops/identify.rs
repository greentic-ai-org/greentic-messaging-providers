//! `identify-instance` export — extracts the receiving bot's numeric id
//! from the URL-embedded bot token so the host can route the inbound to
//! the right `MessagingEndpoint` when multiple Telegram bots share one
//! runtime.

use serde_json::Value;

/// Extract the Telegram bot id that received this inbound update.
///
/// Telegram's webhook convention puts the bot's API token in the URL
/// path (`setWebhook` registers `https://host/<token>` or
/// `https://host/<prefix>/<token>/...`). The token has the public shape
/// `<bot_id>:<secret>` where `bot_id` is a stable, public, numeric
/// identifier minted by BotFather. We extract that prefix as the
/// instance discriminator — never the secret half.
///
/// The input is an HttpInV1 / operator-format HTTP wrapper. If no
/// token-bearing segment is present in the path (e.g. the webhook URL
/// doesn't embed one), returns `None`.
pub(crate) fn extract_bot_id(input_json: &[u8]) -> Option<String> {
    let value: Value = serde_json::from_slice(input_json).ok()?;
    let path = value.get("path").and_then(Value::as_str)?;
    bot_id_from_path(path)
}

fn bot_id_from_path(path: &str) -> Option<String> {
    path.split('/').find_map(|segment| {
        let (prefix, _suffix) = segment.split_once(':')?;
        if prefix.is_empty() || !prefix.bytes().all(|b| b.is_ascii_digit()) {
            return None;
        }
        Some(prefix.to_string())
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn wrap(path: &str) -> Vec<u8> {
        serde_json::to_vec(&json!({
            "method": "POST",
            "path": path,
            "headers": [],
            "body_b64": ""
        }))
        .unwrap()
    }

    #[test]
    fn extracts_bot_id_from_canonical_telegram_webhook_path() {
        let bytes = wrap("/123456789:AAEhBP0av-4LkX9V2gqkX-eYZxFmqp_TjHE");
        assert_eq!(extract_bot_id(&bytes).as_deref(), Some("123456789"));
    }

    #[test]
    fn extracts_bot_id_from_prefixed_path() {
        let bytes = wrap("/telegram/webhook/987654321:AAFsecret-data-here");
        assert_eq!(extract_bot_id(&bytes).as_deref(), Some("987654321"));
    }

    #[test]
    fn extracts_bot_id_from_path_with_trailing_segments() {
        let bytes = wrap("/telegram/555000111:tokensecret/extra/stuff");
        assert_eq!(extract_bot_id(&bytes).as_deref(), Some("555000111"));
    }

    #[test]
    fn returns_none_when_no_token_segment() {
        let bytes = wrap("/telegram/webhook");
        assert!(extract_bot_id(&bytes).is_none());
    }

    #[test]
    fn returns_none_when_prefix_is_not_digits() {
        let bytes = wrap("/abc:notatoken");
        assert!(extract_bot_id(&bytes).is_none());
    }

    #[test]
    fn returns_none_for_unparseable_input() {
        assert!(extract_bot_id(b"not json").is_none());
    }

    #[test]
    fn returns_none_when_path_missing() {
        let bytes = serde_json::to_vec(&json!({
            "method": "POST",
            "headers": [],
            "body_b64": ""
        }))
        .unwrap();
        assert!(extract_bot_id(&bytes).is_none());
    }
}
