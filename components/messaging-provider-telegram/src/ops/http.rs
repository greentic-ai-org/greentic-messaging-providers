//! Thin wrappers around the WASI HTTP client for Telegram Bot API calls.

use serde_json::Value;

use crate::bindings::greentic::http::http_client as client;

/// Send a Telegram `sendMessage` request.
pub(crate) fn tg_send_message(
    api_base: &str,
    token: &str,
    payload: &Value,
) -> Result<client::Response, String> {
    let url = format!("{api_base}/bot{token}/sendMessage");
    let body = serde_json::to_vec(payload).unwrap_or_else(|_| b"{}".to_vec());
    let req = client::Request {
        method: "POST".to_string(),
        url,
        headers: vec![("Content-Type".into(), "application/json".into())],
        body: Some(body),
    };
    client::send(&req, None, None).map_err(|err| format!("transport error: {}", err.message))
}

/// Send a Telegram media message (video, audio, document, sticker, etc.).
pub(crate) fn tg_send_media(
    api_base: &str,
    token: &str,
    method: &str,
    payload: &Value,
) -> Result<Value, String> {
    let url = format!("{api_base}/bot{token}/{method}");
    let body = serde_json::to_vec(payload).unwrap_or_else(|_| b"{}".to_vec());
    let req = client::Request {
        method: "POST".to_string(),
        url,
        headers: vec![("Content-Type".into(), "application/json".into())],
        body: Some(body),
    };
    match client::send(&req, None, None) {
        Ok(resp) => {
            let body_json: Value = resp
                .body
                .as_ref()
                .and_then(|b| serde_json::from_slice(b).ok())
                .unwrap_or(Value::Null);
            if (200..300).contains(&resp.status) {
                Ok(body_json)
            } else {
                Err(format!(
                    "telegram {} status {}: {}",
                    method, resp.status, body_json
                ))
            }
        }
        Err(err) => Err(format!("transport error: {}", err.message)),
    }
}
