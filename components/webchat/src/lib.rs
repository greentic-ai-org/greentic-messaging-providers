#![allow(unsafe_op_in_unsafe_fn)]

mod bindings {
    wit_bindgen::generate!({ path: "wit/webchat", world: "webchat", generate_all });
}

use bindings::Guest;
use bindings::greentic::http::http_client;
use bindings::greentic::secrets_store::secrets_store;
use serde_json::Value;

const DEFAULT_WEBCHAT_URL: &str = "https://example.invalid/webchat/send";
const WEBCHAT_BEARER: &str = "WEBCHAT_BEARER_TOKEN";

struct Component;

impl Guest for Component {
    fn send_message(session_id: String, text: String) -> Result<String, String> {
        let payload = format_message_json(&session_id, &text);
        let token = get_optional_secret(WEBCHAT_BEARER);

        let req = http_client::Request {
            method: "POST".into(),
            url: DEFAULT_WEBCHAT_URL.into(),
            headers: match token {
                Some(Ok(t)) => vec![
                    ("Content-Type".into(), "application/json".into()),
                    ("Authorization".into(), format!("Bearer {}", t)),
                ],
                _ => vec![("Content-Type".into(), "application/json".into())],
            },
            body: Some(payload.clone().into_bytes()),
        };

        let resp = http_client::send(&req, None)
            .map_err(|e| format!("transport error: {} ({})", e.message, e.code))?;

        if (200..300).contains(&resp.status) {
            Ok(payload)
        } else {
            Err(format!(
                "transport error: webchat returned status {}",
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

    fn format_message(session_id: String, text: String) -> String {
        format_message_json(&session_id, &text)
    }
}

fn get_optional_secret(key: &str) -> Option<Result<String, String>> {
    match secrets_store::get(key) {
        Ok(Some(bytes)) => {
            Some(String::from_utf8(bytes).map_err(|_| "secret not valid utf-8".into()))
        }
        Ok(None) => None,
        Err(e) => Some(secret_error(e)),
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

fn format_message_json(session_id: &str, text: &str) -> String {
    let payload = serde_json::json!({
        "session_id": session_id,
        "text": text,
    });
    serde_json::to_string(&payload).unwrap_or_else(|_| "{\"session_id\":\"\",\"text\":\"\"}".into())
}

bindings::__export_world_webchat_cabi!(Component with_types_in bindings);

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn formats_payload() {
        let json = format_message_json("sess-1", "hello");
        let v: Value = serde_json::from_str(&json).unwrap();
        assert_eq!(v["session_id"], "sess-1");
        assert_eq!(v["text"], "hello");
    }

    #[test]
    fn normalizes_webhook() {
        let res = Component::handle_webhook("{}".into(), r#"{"message":"hi"}"#.into()).unwrap();
        let v: Value = serde_json::from_str(&res).unwrap();
        assert_eq!(v["ok"], true);
        assert_eq!(v["event"]["message"], "hi");
    }
}
