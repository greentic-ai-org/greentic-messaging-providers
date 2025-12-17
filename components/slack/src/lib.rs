#![allow(unsafe_op_in_unsafe_fn)]

use hmac::{Hmac, Mac};
use sha2::Sha256;

mod bindings {
    wit_bindgen::generate!({ path: "wit/slack", world: "slack", generate_all });
}

use bindings::Guest;
use bindings::greentic::http::http_client;
use bindings::greentic::secrets_store::secrets_store;

const SLACK_API_URL: &str = "https://slack.com/api/chat.postMessage";
const SLACK_BOT_TOKEN_KEY: &str = "SLACK_BOT_TOKEN";
const SLACK_SIGNING_SECRET_KEY: &str = "SLACK_SIGNING_SECRET";

struct Component;

impl Guest for Component {
    fn send_message(channel: String, text: String) -> Result<String, String> {
        let payload = format_message_json(&channel, &text);
        let token = get_secret_string(SLACK_BOT_TOKEN_KEY)
            .map_err(|e| format!("transport error: {}", e))?;
        let req = http_client::Request {
            method: "POST".to_string(),
            url: SLACK_API_URL.to_string(),
            headers: vec![
                ("Content-Type".into(), "application/json".into()),
                ("Authorization".into(), format!("Bearer {}", token)),
            ],
            body: Some(payload.clone().into_bytes()),
        };

        let resp = http_client::send(&req, None)
            .map_err(|e| format!("transport error: {} ({})", e.message, e.code))?;

        if resp.status >= 200 && resp.status < 300 {
            Ok(payload)
        } else {
            Err(format!(
                "transport error: slack returned status {}",
                resp.status
            ))
        }
    }

    fn handle_webhook(headers_json: String, body_json: String) -> Result<String, String> {
        let headers: serde_json::Map<String, serde_json::Value> =
            serde_json::from_str(&headers_json)
                .map_err(|_| "validation error: invalid headers".to_string())?;

        if let Some(secret_result) = get_optional_secret(SLACK_SIGNING_SECRET_KEY) {
            let signing_secret = secret_result.map_err(|e| format!("transport error: {}", e))?;
            verify_signature(&headers, &body_json, &signing_secret).map_err(|e| e.to_string())?;
        }

        let body_val: serde_json::Value = serde_json::from_str(&body_json)
            .map_err(|_| "validation error: invalid body json".to_string())?;
        let normalized = serde_json::json!({
            "ok": true,
            "event": body_val,
        });
        serde_json::to_string(&normalized)
            .map_err(|_| "other error: serialization failed".to_string())
    }

    fn refresh() -> Result<String, String> {
        Ok(r#"{"ok":true,"refresh":"not-needed"}"#.to_string())
    }

    fn format_message(channel: String, text: String) -> String {
        format_message_json(&channel, &text)
    }
}

fn get_secret_string(key: &str) -> Result<String, String> {
    match secrets_store::get(key) {
        Ok(Some(bytes)) => String::from_utf8(bytes).map_err(|_| "secret not valid utf-8".into()),
        Ok(None) => Err("secret not found".into()),
        Err(e) => Err(secret_error_message(e)),
    }
}

fn get_optional_secret(key: &str) -> Option<Result<String, String>> {
    match secrets_store::get(key) {
        Ok(Some(bytes)) => {
            Some(String::from_utf8(bytes).map_err(|_| "secret not valid utf-8".into()))
        }
        Ok(None) => None,
        Err(e) => Some(Err(secret_error_message(e))),
    }
}

fn format_message_json(channel: &str, text: &str) -> String {
    let payload = payload_with_blocks(channel, text, vec![section_md(text)]);
    serde_json::to_string(&payload).unwrap_or_else(|_| "{\"channel\":\"\",\"text\":\"\"}".into())
}

fn section_md(text: &str) -> serde_json::Value {
    serde_json::json!({
      "type": "section",
      "text": { "type": "mrkdwn", "text": text }
    })
}

fn payload_with_blocks(
    channel: &str,
    text: &str,
    blocks: Vec<serde_json::Value>,
) -> serde_json::Value {
    serde_json::json!({
      "channel": channel,
      "text": text,
      "blocks": blocks,
    })
}

fn verify_signature(
    headers: &serde_json::Map<String, serde_json::Value>,
    body: &str,
    signing_secret: &str,
) -> Result<(), VerificationError> {
    let ts = header_value(headers, "x-slack-request-timestamp")
        .ok_or(VerificationError::MissingTimestamp)?;
    let sig =
        header_value(headers, "x-slack-signature").ok_or(VerificationError::MissingSignature)?;

    let base = format!("v0:{}:{}", ts, body);
    let mut mac = Hmac::<Sha256>::new_from_slice(signing_secret.as_bytes())
        .map_err(|_| VerificationError::InvalidKey)?;
    mac.update(base.as_bytes());
    let computed = mac.finalize().into_bytes();
    let mut hex = String::with_capacity(64);
    for byte in computed {
        use std::fmt::Write;
        write!(&mut hex, "{:02x}", byte).unwrap();
    }
    let expected = format!("v0={}", hex);

    if constant_time_eq(expected.as_bytes(), sig.as_bytes()) {
        Ok(())
    } else {
        Err(VerificationError::SignatureMismatch)
    }
}

fn header_value(
    headers: &serde_json::Map<String, serde_json::Value>,
    name: &str,
) -> Option<String> {
    let lower = name.to_ascii_lowercase();
    headers.iter().find_map(|(k, v)| {
        if k.to_ascii_lowercase() == lower {
            match v {
                serde_json::Value::String(s) => Some(s.clone()),
                serde_json::Value::Array(arr) => arr
                    .iter()
                    .find_map(|val| val.as_str().map(|s| s.to_string())),
                _ => None,
            }
        } else {
            None
        }
    })
}

#[derive(Debug)]
enum VerificationError {
    MissingTimestamp,
    MissingSignature,
    InvalidKey,
    SignatureMismatch,
}

impl std::fmt::Display for VerificationError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            VerificationError::MissingTimestamp => write!(f, "missing timestamp"),
            VerificationError::MissingSignature => write!(f, "missing signature"),
            VerificationError::InvalidKey => write!(f, "invalid signing secret"),
            VerificationError::SignatureMismatch => write!(f, "signature mismatch"),
        }
    }
}

impl std::error::Error for VerificationError {}

fn constant_time_eq(a: &[u8], b: &[u8]) -> bool {
    if a.len() != b.len() {
        return false;
    }
    let mut res = 0u8;
    for (x, y) in a.iter().zip(b.iter()) {
        res |= x ^ y;
    }
    res == 0
}

fn secret_error_message(error: secrets_store::SecretsError) -> String {
    match error {
        secrets_store::SecretsError::NotFound => "secret not found".into(),
        secrets_store::SecretsError::Denied => "secret access denied".into(),
        secrets_store::SecretsError::InvalidKey => "secret key invalid".into(),
        secrets_store::SecretsError::Internal => "secret lookup failed".into(),
    }
}

bindings::__export_world_slack_cabi!(Component with_types_in bindings);

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn formats_message_payload() {
        let json = format_message_json("C123", "hello");
        let value: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert_eq!(value["channel"], "C123");
        assert_eq!(value["text"], "hello");
        assert_eq!(value["blocks"][0]["type"], "section");
        assert_eq!(value["blocks"][0]["text"]["text"], "hello");
    }

    #[test]
    fn verifies_signature() {
        let secret = "8f742231b10e8888abcd99yyyzzz85a5";
        let ts = "1531420618";
        let body = "token=OneLongToken&team_id=T1&api_app_id=A1&event=hello";
        let mut mac = Hmac::<Sha256>::new_from_slice(secret.as_bytes()).unwrap();
        mac.update(format!("v0:{}:{}", ts, body).as_bytes());
        let computed = mac.finalize().into_bytes();
        let mut hex = String::new();
        for byte in computed {
            use std::fmt::Write;
            write!(&mut hex, "{:02x}", byte).unwrap();
        }
        let sig = format!("v0={}", hex);

        let mut headers = serde_json::Map::new();
        headers.insert(
            "X-Slack-Request-Timestamp".into(),
            serde_json::Value::String(ts.to_string()),
        );
        headers.insert("X-Slack-Signature".into(), serde_json::Value::String(sig));

        verify_signature(&headers, body, secret).expect("signature should verify");
    }

    #[test]
    fn signature_mismatch_fails() {
        let mut headers = serde_json::Map::new();
        headers.insert(
            "X-Slack-Request-Timestamp".into(),
            serde_json::Value::String("1".into()),
        );
        headers.insert(
            "X-Slack-Signature".into(),
            serde_json::Value::String("v0=badsignature".into()),
        );
        let err = verify_signature(&headers, "{}", "secret").unwrap_err();
        assert!(matches!(
            err,
            VerificationError::SignatureMismatch | VerificationError::InvalidKey
        ));
    }
}
