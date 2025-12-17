#![allow(unsafe_op_in_unsafe_fn)]

mod bindings {
    wit_bindgen::generate!({ path: "wit/webex", world: "webex", generate_all });
}

use bindings::Guest;
use bindings::greentic::http::http_client;
use bindings::greentic::secrets_store::secrets_store;
use serde_json::Value;

const WEBEX_API: &str = "https://webexapis.com/v1/messages";
const WEBEX_BOT_TOKEN: &str = "WEBEX_BOT_TOKEN";

struct Component;

impl Guest for Component {
    fn send_message(room_id: String, text: String) -> Result<String, String> {
        let token = get_secret(WEBEX_BOT_TOKEN)?;
        let payload = format_message_json(&room_id, &text);

        let req = http_client::Request {
            method: "POST".into(),
            url: WEBEX_API.into(),
            headers: vec![
                ("Content-Type".into(), "application/json".into()),
                ("Authorization".into(), format!("Bearer {}", token)),
            ],
            body: Some(payload.clone().into_bytes()),
        };

        let resp = http_client::send(&req, None)
            .map_err(|e| format!("transport error: {} ({})", e.message, e.code))?;

        if (200..300).contains(&resp.status) {
            Ok(payload)
        } else {
            Err(format!(
                "transport error: webex returned status {}",
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

    fn format_message(room_id: String, text: String) -> String {
        format_message_json(&room_id, &text)
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

fn format_message_json(room_id: &str, text: &str) -> String {
    let payload = serde_json::json!({
        "roomId": room_id,
        "text": text,
    });
    serde_json::to_string(&payload).unwrap_or_else(|_| "{\"roomId\":\"\",\"text\":\"\"}".into())
}

bindings::__export_world_webex_cabi!(Component with_types_in bindings);

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn formats_payload() {
        let json = format_message_json("room123", "hello");
        let v: Value = serde_json::from_str(&json).unwrap();
        assert_eq!(v["roomId"], "room123");
        assert_eq!(v["text"], "hello");
    }

    #[test]
    fn normalizes_webhook() {
        let res = Component::handle_webhook("{}".into(), r#"{"id":"1"}"#.into()).unwrap();
        let v: Value = serde_json::from_str(&res).unwrap();
        assert_eq!(v["ok"], true);
        assert_eq!(v["event"]["id"], "1");
    }
}
