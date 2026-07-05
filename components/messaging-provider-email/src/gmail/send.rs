//! Gmail API send layer: MIME builder + `users.messages.send`.
//!
//! Consumed by `ops::send_payload` for `kind: gmail` configs.

use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
use chrono::Utc;
use serde_json::{Value, json};
use urlencoding::encode as url_encode;

use crate::auth;
use crate::bindings::greentic::http::http_client as client;
use crate::config::ProviderConfig;

const GMAIL_API_BASE: &str = "https://gmail.googleapis.com/gmail/v1";

/// Strips CR/LF from a header value to prevent email-header injection
/// (e.g. an LLM-generated subject or an inbound sender address smuggling a
/// `\r\nBcc: ...` line into the composed MIME message).
fn sanitize_header(value: &str) -> String {
    value.replace("\r\n", " ").replace(['\r', '\n'], " ")
}

/// Builds an RFC 2822 single-part `text/plain; charset=UTF-8` MIME message
/// with CRLF line endings. `Date` uses the current UTC time; Gmail
/// overwrites it with its own receipt time on send regardless. Header
/// values (`to`/`from`/`subject`) are sanitized to strip embedded CR/LF
/// before composing, so header injection cannot smuggle extra headers.
pub(crate) fn build_mime(to: &str, from: &str, subject: &str, body: &str) -> String {
    let to = sanitize_header(to);
    let from = sanitize_header(from);
    let subject = sanitize_header(subject);
    format!(
        "To: {to}\r\nFrom: {from}\r\nSubject: {subject}\r\nDate: {date}\r\nMIME-Version: 1.0\r\nContent-Type: text/plain; charset=UTF-8\r\n\r\n{body}",
        date = Utc::now().to_rfc2822(),
    )
}

/// Builds the `users.messages.send` URL for a given mailbox user.
pub(crate) fn gmail_send_url(user: &str) -> String {
    format!(
        "{}/users/{}/messages/send",
        GMAIL_API_BASE,
        url_encode(user)
    )
}

/// Builds the MIME message, base64url(no-pad) encodes it, acquires a Google
/// OAuth token, and POSTs it to `users.messages.send`. Returns the sent
/// message's `id` on success.
pub(crate) fn gmail_send(
    cfg: &ProviderConfig,
    to: &str,
    subject: &str,
    body: &str,
) -> Result<String, String> {
    let from = cfg
        .gmail_user
        .as_deref()
        .filter(|s| !s.is_empty())
        .ok_or_else(|| "missing gmail_user".to_string())?;
    let mime = build_mime(to, from, subject, body);
    let raw = URL_SAFE_NO_PAD.encode(mime.as_bytes());
    let token = auth::acquire_google_token(cfg)?;
    let url = gmail_send_url(from);
    let request_body = serde_json::to_vec(&json!({"raw": raw}))
        .map_err(|e| format!("invalid gmail send body: {e}"))?;
    let request = client::Request {
        method: "POST".into(),
        url,
        headers: vec![
            ("Authorization".into(), format!("Bearer {token}")),
            ("Content-Type".into(), "application/json".into()),
        ],
        body: Some(request_body),
    };
    let resp = client::send(&request, None, None)
        .map_err(|e| format!("gmail request error: {}", e.message))?;
    if resp.status < 200 || resp.status >= 300 {
        let err_body = resp
            .body
            .as_deref()
            .and_then(|b| std::str::from_utf8(b).ok())
            .unwrap_or("");
        return Err(format!(
            "gmail request returned {} body={}",
            resp.status,
            &err_body[..err_body.len().min(500)]
        ));
    }
    let body = match resp.body {
        Some(body) if !body.is_empty() => body,
        _ => return Err("gmail send response missing id".to_string()),
    };
    let value: Value =
        serde_json::from_slice(&body).map_err(|e| format!("gmail response decode failed: {e}"))?;
    value
        .get("id")
        .and_then(Value::as_str)
        .map(str::to_string)
        .ok_or_else(|| "gmail send response missing id".to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn build_mime_has_expected_headers_crlf_and_body() {
        let mime = build_mime(
            "to@example.com",
            "from@example.com",
            "Hi there",
            "Body text",
        );

        let mut lines = mime.split("\r\n");
        assert_eq!(lines.next(), Some("To: to@example.com"));
        assert_eq!(lines.next(), Some("From: from@example.com"));
        assert_eq!(lines.next(), Some("Subject: Hi there"));
        assert!(lines.next().unwrap_or("").starts_with("Date: "));
        assert_eq!(lines.next(), Some("MIME-Version: 1.0"));
        assert_eq!(
            lines.next(),
            Some("Content-Type: text/plain; charset=UTF-8")
        );
        assert_eq!(lines.next(), Some(""));
        assert_eq!(lines.next(), Some("Body text"));
    }

    #[test]
    fn build_mime_sanitizes_crlf_in_subject_to_prevent_header_injection() {
        let malicious_subject = "Hi there\r\nBcc: attacker@evil.com";
        let mime = build_mime(
            "to@example.com",
            "from@example.com",
            malicious_subject,
            "Body",
        );

        let mut lines = mime.split("\r\n");
        assert_eq!(lines.next(), Some("To: to@example.com"));
        assert_eq!(lines.next(), Some("From: from@example.com"));
        let subject_line = lines.next().unwrap_or("");
        assert_eq!(subject_line, "Subject: Hi there Bcc: attacker@evil.com");
        assert_eq!(
            sanitize_header(malicious_subject),
            "Hi there Bcc: attacker@evil.com"
        );
        assert!(lines.next().unwrap_or("").starts_with("Date: "));
        assert_eq!(lines.next(), Some("MIME-Version: 1.0"));
        assert_eq!(
            lines.next(),
            Some("Content-Type: text/plain; charset=UTF-8")
        );
        assert_eq!(lines.next(), Some(""));
        assert_eq!(lines.next(), Some("Body"));
        assert_eq!(lines.next(), None);

        // No standalone `Bcc:` header line was injected anywhere in the message.
        assert!(
            !mime
                .lines()
                .any(|line| line.trim_start().to_ascii_lowercase().starts_with("bcc:"))
        );
    }

    #[test]
    fn build_mime_sanitizes_crlf_in_to_and_from() {
        let mime = build_mime(
            "to@example.com\r\nBcc: attacker@evil.com",
            "from@example.com\nX-Injected: yes",
            "Subj",
            "Body",
        );

        let mut lines = mime.split("\r\n");
        assert_eq!(
            lines.next(),
            Some("To: to@example.com Bcc: attacker@evil.com")
        );
        assert_eq!(lines.next(), Some("From: from@example.com X-Injected: yes"));
        assert!(
            !mime
                .lines()
                .any(|line| line.trim_start().to_ascii_lowercase().starts_with("bcc:"))
        );
        assert!(!mime.lines().any(|line| {
            line.trim_start()
                .to_ascii_lowercase()
                .starts_with("x-injected:")
        }));
    }

    #[test]
    fn sanitize_header_strips_cr_and_lf() {
        assert_eq!(sanitize_header("a\r\nb\nc\rd"), "a b c d");
        assert_eq!(sanitize_header("clean value"), "clean value");
    }

    #[test]
    fn build_mime_uses_crlf_exclusively() {
        let mime = build_mime("to@example.com", "from@example.com", "Subj", "Body");

        assert!(mime.contains("\r\n"));
        for line_break in mime.match_indices('\n') {
            let idx = line_break.0;
            assert!(idx > 0 && mime.as_bytes()[idx - 1] == b'\r');
        }
    }

    #[test]
    fn gmail_send_url_builds_expected_endpoint() {
        assert_eq!(
            gmail_send_url("me@x.com"),
            "https://gmail.googleapis.com/gmail/v1/users/me%40x.com/messages/send"
        );
    }

    #[test]
    fn gmail_send_url_encodes_special_characters() {
        assert_eq!(
            gmail_send_url("me+alias@x.com"),
            "https://gmail.googleapis.com/gmail/v1/users/me%2Balias%40x.com/messages/send"
        );
    }

    #[test]
    fn gmail_send_url_accepts_plain_alias() {
        assert_eq!(
            gmail_send_url("me"),
            "https://gmail.googleapis.com/gmail/v1/users/me/messages/send"
        );
    }

    #[test]
    fn base64url_of_mime_is_url_safe_no_pad_and_round_trips() {
        let mime = build_mime("to@example.com", "from@example.com", "Subj", "Héllo body");

        let encoded = URL_SAFE_NO_PAD.encode(mime.as_bytes());

        assert!(!encoded.contains('+'));
        assert!(!encoded.contains('/'));
        assert!(!encoded.contains('='));

        let decoded = URL_SAFE_NO_PAD.decode(&encoded).expect("valid base64url");
        assert_eq!(decoded, mime.as_bytes());
    }

    fn gmail_config(user: &str) -> ProviderConfig {
        serde_json::from_value(json!({
            "public_base_url": "https://mail.example.com",
            "host": "smtp.example.com",
            "username": "mailer",
            "from_address": "bot@example.com",
            "kind": "gmail",
            "gmail_user": user,
            "gmail_client_id": "client-id",
            "gmail_client_secret": "client-secret",
            "gmail_refresh_token": "refresh-token"
        }))
        .expect("config")
    }

    #[test]
    fn gmail_send_requires_gmail_user() {
        let mut cfg = gmail_config("me@example.com");
        cfg.gmail_user = None;

        let err = gmail_send(&cfg, "to@example.com", "Subject", "Body")
            .expect_err("expected missing gmail_user error");

        assert_eq!(err, "missing gmail_user");
    }

    #[test]
    fn gmail_send_surfaces_token_acquisition_error_before_http() {
        let mut cfg = gmail_config("me@example.com");
        cfg.gmail_client_id = None;

        let err = gmail_send(&cfg, "to@example.com", "Subject", "Body")
            .expect_err("expected missing gmail_client_id error");

        assert_eq!(err, "missing gmail_client_id");
    }
}
