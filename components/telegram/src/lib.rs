#![allow(unsafe_op_in_unsafe_fn)]

mod bindings {
    wit_bindgen::generate!({ path: "wit/telegram", world: "telegram", generate_all });
}

use bindings::Guest;
use bindings::greentic::http::http_client;
use bindings::greentic::secrets_store::secrets_store;
use serde_json::Value;

const TELEGRAM_API: &str = "https://api.telegram.org";
const TELEGRAM_BOT_TOKEN: &str = "TELEGRAM_BOT_TOKEN";

struct Component;

impl Guest for Component {
    fn send_message(chat_id: String, text: String) -> Result<String, String> {
        let token = get_secret(TELEGRAM_BOT_TOKEN)?;
        let url = format!("{}/bot{}/sendMessage", TELEGRAM_API, token);
        let payload = format_message_json(&chat_id, &text);

        let req = http_client::Request {
            method: "POST".into(),
            url,
            headers: vec![("Content-Type".into(), "application/json".into())],
            body: Some(payload.clone().into_bytes()),
        };

        let resp = http_client::send(&req, None)
            .map_err(|e| format!("transport error: {} ({})", e.message, e.code))?;

        if (200..300).contains(&resp.status) {
            Ok(payload)
        } else {
            Err(format!(
                "transport error: telegram returned status {}",
                resp.status
            ))
        }
    }

    fn handle_webhook(_headers_json: String, body_json: String) -> Result<String, String> {
        let parsed: Value = serde_json::from_str(&body_json)
            .map_err(|_| "validation error: invalid body".to_string())?;
        let normalized = serde_json::json!({ "ok": true, "event": parsed });
        serde_json::to_string(&normalized).map_err(|_| "other error: serialization failed".into())
    }

    fn refresh() -> Result<String, String> {
        Ok(r#"{"ok":true,"refresh":"not-needed"}"#.to_string())
    }

    fn format_message(chat_id: String, text: String) -> String {
        format_message_json(&chat_id, &text)
    }
}

fn get_secret(key: &str) -> Result<String, String> {
    match secrets_store::get(key) {
        Ok(Some(bytes)) => String::from_utf8(bytes).map_err(|_| "secret not valid utf-8".into()),
        Ok(None) => Err("secret not found".into()),
        Err(e) => secret_error(e),
    }
}

fn secret_error(error: secrets_store::SecretsError) -> Result<String, String> {
    Err(match error {
        secrets_store::SecretsError::NotFound => "secret not found".into(),
        secrets_store::SecretsError::Denied => "secret access denied".into(),
        secrets_store::SecretsError::InvalidKey => "secret key invalid".into(),
        secrets_store::SecretsError::Internal => "secret lookup failed".into(),
    })
}

fn format_message_json(chat_id: &str, text: &str) -> String {
    let payload = serde_json::json!({
        "chat_id": chat_id,
        "text": text,
        "parse_mode": "HTML"
    });
    serde_json::to_string(&payload).unwrap_or_else(|_| "{\"chat_id\":\"\",\"text\":\"\"}".into())
}

bindings::__export_world_telegram_cabi!(Component with_types_in bindings);

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn formats_payload() {
        let json = format_message_json("123", "hello");
        let v: Value = serde_json::from_str(&json).unwrap();
        assert_eq!(v["chat_id"], "123");
        assert_eq!(v["text"], "hello");
        assert_eq!(v["parse_mode"], "HTML");
    }

    #[test]
    fn normalizes_webhook() {
        let res = Component::handle_webhook("{}".into(), r#"{"update_id":1}"#.into()).unwrap();
        let v: Value = serde_json::from_str(&res).unwrap();
        assert_eq!(v["ok"], true);
        assert_eq!(v["event"]["update_id"], 1);
    }
}
