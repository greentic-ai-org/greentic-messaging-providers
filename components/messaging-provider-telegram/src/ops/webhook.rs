//! `setup_webhook` op for the Telegram provider.
//!
//! Calls the Telegram Bot API `setWebhook`. The caller may pass a fully-formed
//! `webhook_url` (the revision-serve model builds `{prefix}/webhook/telegram`);
//! otherwise it falls back to the legacy
//! `{public_base_url}/v1/messaging/ingress/{provider_id}/{tenant}/{team}` path.
//! Accepts config either as top-level fields or under a
//! nested `"config"` object, and falls back to the secrets store for the bot
//! token when no explicit value is supplied.

use provider_common::helpers::json_bytes;
use serde_json::{Value, json};

use crate::DEFAULT_API_BASE;
use crate::bindings::greentic::http::http_client as client;

/// Handle `setup_webhook` op — calls Telegram `setWebhook` API.
///
/// Input JSON:
/// ```json
/// {
///   "bot_token": "123:ABC",
///   "webhook_url": "https://host/bot/webhook/telegram", // optional, takes precedence
///   "secret_token": "<endpoint provider_id>",           // optional, IID discriminator
///   "public_base_url": "https://example.ngrok-free.app", // legacy fallback
///   "api_base_url": "https://api.telegram.org",   // optional
///   "provider_id": "messaging-telegram",           // optional, default
///   "tenant": "default",                           // optional, default
///   "team": "default"                              // optional, default
/// }
/// ```
///
/// Calls: POST {api_base}/bot{token}/setWebhook
///   { "url": <webhook_url | legacy path>, "secret_token"?: ... }
pub(crate) fn setup_webhook(input_json: &[u8]) -> Vec<u8> {
    let parsed: Value = match serde_json::from_slice(input_json) {
        Ok(val) => val,
        Err(err) => {
            return json_bytes(&json!({"ok": false, "error": format!("invalid json: {err}")}));
        }
    };

    // Try loading config from standard config structure first, fall back to flat fields
    let bot_token = parsed
        .get("bot_token")
        .and_then(Value::as_str)
        .or_else(|| {
            parsed
                .get("config")
                .and_then(|c| c.get("bot_token"))
                .and_then(Value::as_str)
        })
        .map(str::trim)
        .filter(|s| !s.is_empty());

    let bot_token = match bot_token {
        Some(t) => t.to_string(),
        None => {
            // Try secrets store as fallback
            match crate::config::get_bot_token(&crate::config::ProviderConfig {
                enabled: true,
                public_base_url: String::new(),
                default_chat_id: None,
                api_base_url: None,
                bot_token: None,
            }) {
                Ok(t) => t,
                Err(_) => {
                    return json_bytes(
                        &json!({"ok": false, "error": "bot_token required for webhook setup"}),
                    );
                }
            }
        }
    };

    let api_base = parsed
        .get("api_base_url")
        .and_then(Value::as_str)
        .or_else(|| {
            parsed
                .get("config")
                .and_then(|c| c.get("api_base_url"))
                .and_then(Value::as_str)
        })
        .unwrap_or(DEFAULT_API_BASE);

    let body = match build_set_webhook_body(&parsed) {
        Ok(body) => body,
        Err(err) => return json_bytes(&json!({"ok": false, "error": err})),
    };
    let webhook_url = body
        .get("url")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string();

    // Call setWebhook
    let url = format!("{api_base}/bot{bot_token}/setWebhook");
    let body_bytes = serde_json::to_vec(&body).unwrap_or_default();
    let req = client::Request {
        method: "POST".to_string(),
        url: url.clone(),
        headers: vec![("Content-Type".into(), "application/json".into())],
        body: Some(body_bytes),
    };

    match client::send(&req, None, None) {
        Ok(resp) => {
            let resp_body: Value = resp
                .body
                .as_ref()
                .and_then(|b| serde_json::from_slice(b).ok())
                .unwrap_or(Value::Null);

            if (200..300).contains(&resp.status) {
                let tg_ok = resp_body
                    .get("ok")
                    .and_then(Value::as_bool)
                    .unwrap_or(false);
                let description = resp_body
                    .get("description")
                    .and_then(Value::as_str)
                    .unwrap_or("");
                json_bytes(&json!({
                    "ok": tg_ok,
                    "webhook_url": webhook_url,
                    "description": description,
                    "telegram_response": resp_body,
                }))
            } else {
                json_bytes(&json!({
                    "ok": false,
                    "error": format!("setWebhook HTTP {}: {}", resp.status, resp_body),
                    "webhook_url": webhook_url,
                }))
            }
        }
        Err(err) => json_bytes(&json!({
            "ok": false,
            "error": format!("transport error: {}", err.message),
            "webhook_url": webhook_url,
        })),
    }
}

/// Build the `setWebhook` request body from the op input. Prefers a fully-formed
/// `webhook_url` (revision-serve model); otherwise composes the legacy ingress
/// path from `public_base_url` + `provider_id`/`tenant`/`team`. Adds an optional
/// `secret_token` (the endpoint's `provider_id`) when present.
fn build_set_webhook_body(parsed: &Value) -> Result<Value, String> {
    let field = |key: &str| -> Option<String> {
        parsed
            .get(key)
            .or_else(|| parsed.get("config").and_then(|c| c.get(key)))
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(String::from)
    };

    let webhook_url = match field("webhook_url") {
        Some(url) => {
            if !url.starts_with("https://") {
                return Err("webhook_url must be an https:// URL".to_string());
            }
            url
        }
        None => {
            let public_base_url = field("public_base_url").unwrap_or_default();
            if public_base_url.is_empty() || !public_base_url.starts_with("https://") {
                return Err("public_base_url or webhook_url required (https://)".to_string());
            }
            let provider_id = field("provider_id").unwrap_or_else(|| "messaging-telegram".into());
            let tenant = field("tenant").unwrap_or_else(|| "default".into());
            let team = field("team").unwrap_or_else(|| "default".into());
            format!(
                "{}/v1/messaging/ingress/{}/{}/{}",
                public_base_url.trim_end_matches('/'),
                provider_id,
                tenant,
                team,
            )
        }
    };

    let mut body = json!({
        "url": webhook_url,
        "allowed_updates": ["message", "callback_query", "edited_message"]
    });
    if let Some(token) = field("secret_token") {
        body["secret_token"] = json!(token);
    }
    Ok(body)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn webhook_url_override_takes_precedence_and_carries_secret_token() {
        let body = build_set_webhook_body(&json!({
            "webhook_url": "https://host.example/bot/webhook/telegram",
            "secret_token": "abc123",
            "public_base_url": "https://ignored.example",
        }))
        .expect("body");
        assert_eq!(body["url"], "https://host.example/bot/webhook/telegram");
        assert_eq!(body["secret_token"], "abc123");
    }

    #[test]
    fn legacy_path_built_when_no_override() {
        let body = build_set_webhook_body(&json!({
            "public_base_url": "https://host.example/",
            "provider_id": "messaging-telegram",
            "tenant": "demo",
            "team": "support",
        }))
        .expect("body");
        assert_eq!(
            body["url"],
            "https://host.example/v1/messaging/ingress/messaging-telegram/demo/support"
        );
        assert!(body.get("secret_token").is_none());
    }

    #[test]
    fn http_override_rejected() {
        let err = build_set_webhook_body(&json!({"webhook_url": "http://host.example/x"}))
            .expect_err("must reject non-https override");
        assert!(err.contains("https"));
    }

    #[test]
    fn missing_url_sources_rejected() {
        let err = build_set_webhook_body(&json!({})).expect_err("must require a url source");
        assert!(err.contains("required"));
    }
}
