//! Twilio Messages API send — ported from
//! `greentic-events-providers/crates/provider-sms`'s `TwilioSendRequest`/
//! `build_send_request`, adapted to read secrets via this component's
//! `secrets-store` import instead of the native `SecretProvider` trait.

use base64::{Engine as _, engine::general_purpose::STANDARD};
use provider_common::helpers::json_bytes;
use serde_json::{Value, json};

use crate::bindings::greentic::http::http_client as client;
use crate::{ACCOUNT_SID_KEY, AUTH_TOKEN_KEY, FROM_NUMBER_KEY, PROVIDER_TYPE};

const TWILIO_API_BASE: &str = "https://api.twilio.com/2010-04-01";

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct TwilioSendRequest {
    pub(crate) to: String,
    pub(crate) from: String,
    pub(crate) body: String,
}

pub(crate) fn build_twilio_send(from: &str, to: &str, body: &str) -> TwilioSendRequest {
    TwilioSendRequest {
        to: to.to_string(),
        from: from.to_string(),
        body: body.to_string(),
    }
}

impl TwilioSendRequest {
    fn form_encoded_body(&self) -> Vec<u8> {
        format!(
            "To={}&From={}&Body={}",
            urlencoding::encode(&self.to),
            urlencoding::encode(&self.from),
            urlencoding::encode(&self.body),
        )
        .into_bytes()
    }
}

fn basic_auth_header(account_sid: &str, auth_token: &str) -> String {
    format!(
        "Basic {}",
        STANDARD.encode(format!("{account_sid}:{auth_token}"))
    )
}

fn messages_url(account_sid: &str) -> String {
    format!("{TWILIO_API_BASE}/Accounts/{account_sid}/Messages.json")
}

/// Twilio's REST send response, reduced to the fields this component reads.
struct TwilioResponse {
    status: u16,
    body: Value,
}

impl TwilioResponse {
    #[cfg(test)]
    fn success(sid: &str) -> Self {
        Self {
            status: 201,
            body: json!({"sid": sid, "status": "queued"}),
        }
    }

    #[cfg(test)]
    fn error(status: u16, code: &str, message: &str) -> Self {
        Self {
            status,
            body: json!({"code": code, "message": message}),
        }
    }
}

struct SendOutcome {
    ok: bool,
    sid: Option<String>,
    error: Option<String>,
    status: Option<u16>,
}

/// Map a Twilio Messages API response to a structured outcome: `2xx` ->
/// success + `sid`; non-`2xx` -> a message combining the Twilio error code
/// (when present) and HTTP status, never a panic.
fn send_payload_result(resp: TwilioResponse) -> SendOutcome {
    if (200..300).contains(&resp.status) {
        let sid = resp
            .body
            .get("sid")
            .and_then(Value::as_str)
            .unwrap_or("sms-message")
            .to_string();
        SendOutcome {
            ok: true,
            sid: Some(sid),
            error: None,
            status: Some(resp.status),
        }
    } else {
        let code = resp.body.get("code").and_then(Value::as_str);
        let message = resp
            .body
            .get("message")
            .and_then(Value::as_str)
            .unwrap_or("twilio send failed");
        let error = match code {
            Some(code) => format!("twilio error {code}: {message} (status {})", resp.status),
            None => format!("{message} (status {})", resp.status),
        };
        SendOutcome {
            ok: false,
            sid: None,
            error: Some(error),
            status: Some(resp.status),
        }
    }
}

pub(crate) fn handle_send(input_json: &[u8]) -> Vec<u8> {
    let parsed: Value = match serde_json::from_slice(input_json) {
        Ok(val) => val,
        Err(err) => {
            return json_bytes(&json!({"ok": false, "error": format!("invalid json: {err}")}));
        }
    };

    let to = match parsed.get("to").and_then(Value::as_str).map(str::trim) {
        Some(id) if !id.is_empty() => id.to_string(),
        _ => return json_bytes(&json!({"ok": false, "error": "destination required"})),
    };
    let body = match parsed.get("body").and_then(Value::as_str).map(str::trim) {
        Some(text) if !text.is_empty() => text.to_string(),
        _ => return json_bytes(&json!({"ok": false, "error": "text required"})),
    };

    let account_sid = match resolve_secret(ACCOUNT_SID_KEY) {
        Some(value) => value,
        None => {
            return json_bytes(
                &json!({"ok": false, "error": "missing secret: TWILIO_ACCOUNT_SID"}),
            );
        }
    };
    let auth_token = match resolve_secret(AUTH_TOKEN_KEY) {
        Some(value) => value,
        None => {
            return json_bytes(&json!({"ok": false, "error": "missing secret: TWILIO_AUTH_TOKEN"}));
        }
    };
    let from = match resolve_secret(FROM_NUMBER_KEY) {
        Some(value) => value,
        None => {
            return json_bytes(
                &json!({"ok": false, "error": "missing secret: TWILIO_FROM_NUMBER"}),
            );
        }
    };

    let twilio_request = build_twilio_send(&from, &to, &body);
    let http_request = client::Request {
        method: "POST".into(),
        url: messages_url(&account_sid),
        headers: vec![
            (
                "Content-Type".into(),
                "application/x-www-form-urlencoded".into(),
            ),
            (
                "Authorization".into(),
                basic_auth_header(&account_sid, &auth_token),
            ),
        ],
        body: Some(twilio_request.form_encoded_body()),
    };

    let resp = match client::send(&http_request, None, None) {
        Ok(resp) => resp,
        Err(err) => {
            return json_bytes(
                &json!({"ok": false, "error": format!("transport error: {}", err.message)}),
            );
        }
    };

    let body_bytes = resp.body.unwrap_or_default();
    let body_json: Value = serde_json::from_slice(&body_bytes).unwrap_or(Value::Null);
    let outcome = send_payload_result(TwilioResponse {
        status: resp.status,
        body: body_json,
    });

    if outcome.ok {
        json_bytes(&json!({
            "ok": true,
            "provider_type": PROVIDER_TYPE,
            "sid": outcome.sid,
        }))
    } else {
        json_bytes(&json!({
            "ok": false,
            "error": outcome.error.unwrap_or_else(|| "twilio send failed".to_string()),
            "status": outcome.status,
        }))
    }
}

#[cfg(not(test))]
fn resolve_secret(key: &str) -> Option<String> {
    use crate::bindings::greentic::secrets_store::secrets_store;
    match secrets_store::get(key) {
        Ok(Some(bytes)) => String::from_utf8(bytes)
            .ok()
            .filter(|s| !s.trim().is_empty()),
        _ => None,
    }
}

#[cfg(test)]
fn resolve_secret(_key: &str) -> Option<String> {
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builds_twilio_send_request_from_reply() {
        let req = build_twilio_send("+15559990000", "+15551230001", "thanks!");
        assert_eq!(req.to, "+15551230001");
        assert_eq!(req.from, "+15559990000");
        assert_eq!(req.body, "thanks!");
    }

    #[test]
    fn twilio_send_request_form_encodes_to_from_body() {
        let req = build_twilio_send("+15559990000", "+15551230001", "hi there");
        let encoded = String::from_utf8(req.form_encoded_body()).expect("utf8 form body");
        assert!(encoded.contains("To=%2B15551230001"), "{encoded}");
        assert!(encoded.contains("From=%2B15559990000"), "{encoded}");
        assert!(encoded.contains("Body=hi%20there"), "{encoded}");
    }

    #[test]
    fn basic_auth_header_encodes_account_sid_colon_auth_token() {
        let header = basic_auth_header("AC123", "secret-token");
        assert!(header.starts_with("Basic "));
        let decoded = STANDARD
            .decode(header.trim_start_matches("Basic "))
            .expect("decode basic auth header");
        assert_eq!(String::from_utf8(decoded).unwrap(), "AC123:secret-token");
    }

    #[test]
    fn messages_url_targets_the_account_sid_path() {
        assert_eq!(
            messages_url("AC123"),
            "https://api.twilio.com/2010-04-01/Accounts/AC123/Messages.json"
        );
    }

    #[test]
    fn send_payload_maps_non_2xx_to_structured_error() {
        let out = send_payload_result(TwilioResponse::error(400, "21211", "invalid To"));
        assert!(!out.ok);
        assert_eq!(out.status, Some(400));
        assert!(out.sid.is_none());
        assert!(out.error.unwrap().contains("21211"));
    }

    #[test]
    fn send_payload_maps_2xx_to_success_with_sid() {
        let out = send_payload_result(TwilioResponse::success("SM123"));
        assert!(out.ok);
        assert_eq!(out.sid.as_deref(), Some("SM123"));
        assert!(out.error.is_none());
    }

    #[test]
    fn handle_send_rejects_missing_destination_without_network() {
        let out = handle_send(br#"{"body":"hi"}"#);
        let value: Value = serde_json::from_slice(&out).expect("json result");
        assert_eq!(value["ok"], false);
        assert_eq!(value["error"], "destination required");
    }

    #[test]
    fn handle_send_rejects_missing_text_without_network() {
        let out = handle_send(br#"{"to":"+15551230001"}"#);
        let value: Value = serde_json::from_slice(&out).expect("json result");
        assert_eq!(value["ok"], false);
        assert_eq!(value["error"], "text required");
    }

    #[test]
    fn handle_send_surfaces_missing_secret_without_network() {
        let out = handle_send(br#"{"to":"+15551230001","body":"hi"}"#);
        let value: Value = serde_json::from_slice(&out).expect("json result");
        assert_eq!(value["ok"], false);
        assert!(
            value["error"]
                .as_str()
                .unwrap_or_default()
                .contains("TWILIO_ACCOUNT_SID")
        );
    }

    #[test]
    fn handle_send_rejects_invalid_json_without_network() {
        let out = handle_send(b"not json");
        let value: Value = serde_json::from_slice(&out).expect("json result");
        assert_eq!(value["ok"], false);
    }
}
