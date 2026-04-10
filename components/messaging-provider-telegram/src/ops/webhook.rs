//! `setup_webhook` op for the Telegram provider.
//!
//! Calls the Telegram Bot API `setWebhook` endpoint with the provider's
//! configured `public_base_url` plus an optional `webhook_path` (defaults to
//! `/messaging/telegram`). Accepts config either as top-level fields or under a
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
///   "public_base_url": "https://example.ngrok-free.app",
///   "api_base_url": "https://api.telegram.org",   // optional
///   "webhook_path": "/messaging/telegram"          // optional, default
/// }
/// ```
///
/// Calls: POST {api_base}/bot{token}/setWebhook { "url": "{public_base_url}{webhook_path}" }
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

    let public_base_url = parsed
        .get("public_base_url")
        .and_then(Value::as_str)
        .or_else(|| {
            parsed
                .get("config")
                .and_then(|c| c.get("public_base_url"))
                .and_then(Value::as_str)
        })
        .unwrap_or("");

    if public_base_url.is_empty() || !public_base_url.starts_with("https://") {
        return json_bytes(
            &json!({"ok": false, "error": "public_base_url must be an https:// URL"}),
        );
    }

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

    let webhook_path = parsed
        .get("webhook_path")
        .and_then(Value::as_str)
        .unwrap_or("/messaging/telegram");

    let webhook_url = format!("{}{}", public_base_url.trim_end_matches('/'), webhook_path);

    // Call setWebhook
    let url = format!("{api_base}/bot{bot_token}/setWebhook");
    let body = json!({
        "url": webhook_url,
        "allowed_updates": ["message", "callback_query", "edited_message"]
    });
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
