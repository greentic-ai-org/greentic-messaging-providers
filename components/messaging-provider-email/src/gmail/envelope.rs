//! Gmail message → `ChannelMessageEnvelope` mapping + Pub/Sub push handling.
//!
//! `gmail_message_to_envelope` maps a `users.messages.get` (format=full) JSON
//! body to the same envelope shape the Graph inbound path produces, so both
//! backends route identically. `handle_gmail_push` assembles the full push
//! pipeline: verify -> parse -> acquire token -> list_history -> get_message
//! -> map, ACKing (200, no events) on any post-verification error so Pub/Sub
//! doesn't redeliver forever.

use base64::{
    Engine,
    engine::general_purpose::{STANDARD, URL_SAFE_NO_PAD},
};
use serde_json::Value;

use crate::auth::acquire_google_token;
use crate::config::ProviderConfig;
use crate::gmail::fetch::{get_message, list_history};
use crate::gmail::push::{parse_pubsub_push, verify_push};
use crate::ingress::{default_env, default_tenant};
use greentic_types::messaging::universal_dto::{HttpInV1, HttpOutV1};
use greentic_types::{Actor, ChannelMessageEnvelope, Destination, MessageMetadata, TenantCtx};
use provider_common::http_compat::{http_out_error, http_out_v1_bytes};

#[derive(Default)]
struct PartScan {
    text_plain: Option<String>,
    text_html: Option<String>,
    attachments_dropped: usize,
}

fn decode_body_data(data: &str) -> Option<String> {
    let bytes = URL_SAFE_NO_PAD.decode(data).ok()?;
    String::from_utf8(bytes).ok()
}

fn scan_parts(part: &Value, scan: &mut PartScan) {
    let mime_type = part.get("mimeType").and_then(Value::as_str).unwrap_or("");
    let has_filename = part
        .get("filename")
        .and_then(Value::as_str)
        .is_some_and(|name| !name.is_empty());
    let data = part
        .get("body")
        .and_then(|body| body.get("data"))
        .and_then(Value::as_str);
    if has_filename {
        scan.attachments_dropped += 1;
    } else if let Some(data) = data {
        match mime_type {
            "text/plain" if scan.text_plain.is_none() => scan.text_plain = decode_body_data(data),
            "text/html" if scan.text_html.is_none() => scan.text_html = decode_body_data(data),
            _ => {}
        }
    }
    if let Some(children) = part.get("parts").and_then(Value::as_array) {
        for child in children {
            scan_parts(child, scan);
        }
    }
}

fn header_value<'a>(headers: &'a [Value], name: &str) -> Option<&'a str> {
    headers.iter().find_map(|header| {
        let header_name = header.get("name").and_then(Value::as_str)?;
        if header_name.eq_ignore_ascii_case(name) {
            header.get("value").and_then(Value::as_str)
        } else {
            None
        }
    })
}

/// Extracts the bare address from an RFC 5322 `From`-style header value
/// (`"Jane Doe <jane@example.com>"` -> `"jane@example.com"`).
fn extract_email_address(raw: &str) -> String {
    if let Some(start) = raw.find('<')
        && let Some(end) = raw[start + 1..].find('>')
    {
        return raw[start + 1..start + 1 + end].trim().to_string();
    }
    raw.trim().to_string()
}

/// Maps a Gmail `users.messages.get` (format=full) JSON body to a
/// `ChannelMessageEnvelope`, mirroring the Graph inbound envelope shape.
pub(crate) fn gmail_message_to_envelope(
    msg: &Value,
    cfg: &ProviderConfig,
    tenant: &TenantCtx,
) -> Option<ChannelMessageEnvelope> {
    let id = msg.get("id").and_then(Value::as_str)?;
    let payload = msg.get("payload")?;
    let headers: Vec<Value> = payload
        .get("headers")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    let subject = header_value(&headers, "Subject")
        .unwrap_or("email message")
        .to_string();
    let from_address = header_value(&headers, "From")
        .map(extract_email_address)
        .unwrap_or_default();

    let mut scan = PartScan::default();
    scan_parts(payload, &mut scan);

    let (text, text_source) = match scan.text_plain {
        Some(plain) => (Some(plain), "text_plain"),
        None => (
            msg.get("snippet")
                .and_then(Value::as_str)
                .map(str::to_string),
            "snippet",
        ),
    };

    let mut metadata = MessageMetadata::new();
    metadata.insert("gmail_message_id".to_string(), id.to_string());
    metadata.insert("subject".to_string(), subject.clone());
    metadata.insert("text_source".to_string(), text_source.to_string());
    if let Some(html) = scan.text_html {
        metadata.insert("gmail_html_body".to_string(), html);
    }
    if scan.attachments_dropped > 0 {
        metadata.insert(
            "gmail_attachments_dropped".to_string(),
            scan.attachments_dropped.to_string(),
        );
    }
    if !from_address.is_empty() {
        metadata.insert("from".to_string(), from_address.clone());
        metadata.insert("to".to_string(), from_address.clone());
    }

    let mailbox_user = cfg
        .gmail_user
        .clone()
        .filter(|user| !user.is_empty())
        .unwrap_or_else(|| "gmail".to_string());
    let destinations = if from_address.is_empty() {
        Vec::new()
    } else {
        vec![Destination {
            id: from_address,
            kind: Some("email".into()),
        }]
    };

    Some(ChannelMessageEnvelope {
        id: format!("email-{id}"),
        tenant: tenant.clone(),
        channel: "email".to_string(),
        session_id: id.to_string(),
        reply_scope: None,
        from: Some(Actor {
            id: mailbox_user,
            kind: Some("user".into()),
        }),
        to: destinations,
        correlation_id: Some(id.to_string()),
        text,
        attachments: Vec::new(),
        metadata,
        extensions: Default::default(),
    })
}

fn ack_empty() -> Vec<u8> {
    let out = HttpOutV1 {
        status: 200,
        headers: Vec::new(),
        body_b64: String::new(),
        events: Vec::new(),
    };
    http_out_v1_bytes(&out)
}

/// Handles an inbound Gmail Pub/Sub push: verify -> parse -> acquire token ->
/// list_history -> get_message per id -> map to envelopes.
///
/// Verification failure -> 403. A malformed body -> 400. Any error past
/// verification (token exchange, Gmail API fetch) is logged and ACKed with
/// `200` + no events, since a non-2xx status makes Pub/Sub redeliver forever.
pub(crate) fn handle_gmail_push(http: &HttpInV1, cfg: &ProviderConfig) -> Vec<u8> {
    let expected_token = cfg
        .gmail_pubsub_verification_token
        .as_deref()
        .unwrap_or_default();
    if !verify_push(http, expected_token) {
        return http_out_error(403, "gmail push verification failed");
    }
    let body = match STANDARD.decode(&http.body_b64) {
        Ok(bytes) => bytes,
        Err(err) => return http_out_error(400, &format!("invalid gmail push body: {err}")),
    };
    let notification = match parse_pubsub_push(&body) {
        Ok(value) => value,
        Err(err) => return http_out_error(400, &err),
    };

    let token = match acquire_google_token(cfg) {
        Ok(value) => value,
        Err(err) => {
            eprintln!(
                "gmail push: token acquisition failed for mailbox {}: {err}",
                notification.email_address
            );
            return ack_empty();
        }
    };
    let message_ids = match list_history(&token, cfg, &notification.history_id) {
        Ok(ids) => ids,
        Err(err) => {
            eprintln!(
                "gmail push: history.list failed for mailbox {}: {err}",
                notification.email_address
            );
            return ack_empty();
        }
    };

    let tenant = TenantCtx::new(default_env(), default_tenant());
    let mut events = Vec::new();
    for id in message_ids {
        match get_message(&token, cfg, &id) {
            Ok(message) => {
                if let Some(envelope) = gmail_message_to_envelope(&message, cfg, &tenant) {
                    events.push(envelope);
                }
            }
            Err(err) => eprintln!("gmail push: messages.get failed for id {id}: {err}"),
        }
    }

    let out = HttpOutV1 {
        status: 200,
        headers: Vec::new(),
        body_b64: String::new(),
        events,
    };
    http_out_v1_bytes(&out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn cfg(gmail_user: Option<&str>, token: Option<&str>) -> ProviderConfig {
        let mut value = json!({
            "public_base_url": "https://mail.example.com",
            "host": "smtp.example.com",
            "username": "mailer",
            "from_address": "bot@example.com",
            "kind": "gmail",
        });
        if let Some(user) = gmail_user {
            value["gmail_user"] = json!(user);
        }
        if let Some(token) = token {
            value["gmail_pubsub_verification_token"] = json!(token);
        }
        serde_json::from_value(value).expect("config")
    }

    fn tenant() -> TenantCtx {
        TenantCtx::new(default_env(), default_tenant())
    }

    #[test]
    fn maps_simple_message_with_text_plain_body() {
        let cfg = cfg(Some("me@example.com"), None);
        let text_b64 = URL_SAFE_NO_PAD.encode(b"Hi there, this is the body.");
        let msg = json!({
            "id": "18abc123",
            "snippet": "Hi there, this is the body.",
            "payload": {
                "mimeType": "text/plain",
                "headers": [
                    {"name": "From", "value": "Jane Doe <jane@example.com>"},
                    {"name": "Subject", "value": "Hello there"},
                ],
                "body": {"data": text_b64},
            },
        });

        let env = gmail_message_to_envelope(&msg, &cfg, &tenant()).expect("envelope");

        assert_eq!(env.channel, "email");
        assert_eq!(env.id, "email-18abc123");
        assert_eq!(env.correlation_id.as_deref(), Some("18abc123"));
        assert_eq!(env.text.as_deref(), Some("Hi there, this is the body."));
        assert_eq!(env.to[0].id, "jane@example.com");
        assert_eq!(
            env.metadata.get("subject").map(String::as_str),
            Some("Hello there")
        );
        assert_eq!(
            env.metadata.get("text_source").map(String::as_str),
            Some("text_plain")
        );
        assert_eq!(env.from.as_ref().unwrap().id, "me@example.com");
    }

    #[test]
    fn multipart_message_picks_text_plain_part_over_html() {
        let cfg = cfg(Some("me@example.com"), None);
        let plain_b64 = URL_SAFE_NO_PAD.encode(b"Plain body");
        let html_b64 = URL_SAFE_NO_PAD.encode(b"<p>Html body</p>");
        let msg = json!({
            "id": "m2",
            "snippet": "Plain body",
            "payload": {
                "mimeType": "multipart/alternative",
                "headers": [
                    {"name": "from", "value": "sender@example.com"},
                    {"name": "subject", "value": "Multipart"},
                ],
                "parts": [
                    {"mimeType": "text/plain", "body": {"data": plain_b64}},
                    {"mimeType": "text/html", "body": {"data": html_b64}},
                ],
            },
        });

        let env = gmail_message_to_envelope(&msg, &cfg, &tenant()).expect("envelope");

        assert_eq!(env.text.as_deref(), Some("Plain body"));
        assert_eq!(
            env.metadata.get("gmail_html_body").map(String::as_str),
            Some("<p>Html body</p>")
        );
        assert_eq!(
            env.metadata.get("text_source").map(String::as_str),
            Some("text_plain")
        );
    }

    #[test]
    fn nested_multipart_mixed_finds_text_plain_and_counts_attachment() {
        let cfg = cfg(Some("me@example.com"), None);
        let plain_b64 = URL_SAFE_NO_PAD.encode(b"Nested plain body");
        let msg = json!({
            "id": "m3",
            "payload": {
                "mimeType": "multipart/mixed",
                "headers": [{"name": "Subject", "value": "Nested"}],
                "parts": [
                    {
                        "mimeType": "multipart/alternative",
                        "parts": [
                            {"mimeType": "text/plain", "body": {"data": plain_b64}},
                        ],
                    },
                    {
                        "mimeType": "application/pdf",
                        "filename": "report.pdf",
                        "body": {"attachmentId": "abc123"},
                    },
                ],
            },
        });

        let env = gmail_message_to_envelope(&msg, &cfg, &tenant()).expect("envelope");

        assert_eq!(env.text.as_deref(), Some("Nested plain body"));
        assert_eq!(
            env.metadata
                .get("gmail_attachments_dropped")
                .map(String::as_str),
            Some("1")
        );
    }

    #[test]
    fn html_only_message_falls_back_to_snippet_and_notes_source() {
        let cfg = cfg(Some("me@example.com"), None);
        let html_b64 = URL_SAFE_NO_PAD.encode(b"<p>Only html</p>");
        let msg = json!({
            "id": "m4",
            "snippet": "Only html preview",
            "payload": {
                "mimeType": "text/html",
                "headers": [{"name": "Subject", "value": "Html only"}],
                "body": {"data": html_b64},
            },
        });

        let env = gmail_message_to_envelope(&msg, &cfg, &tenant()).expect("envelope");

        assert_eq!(env.text.as_deref(), Some("Only html preview"));
        assert_eq!(
            env.metadata.get("text_source").map(String::as_str),
            Some("snippet")
        );
        assert_eq!(
            env.metadata.get("gmail_html_body").map(String::as_str),
            Some("<p>Only html</p>")
        );
    }

    #[test]
    fn missing_id_returns_none() {
        let cfg = cfg(None, None);
        let msg = json!({"payload": {"mimeType": "text/plain"}});

        assert!(gmail_message_to_envelope(&msg, &cfg, &tenant()).is_none());
    }

    fn http(query: Option<&str>, body_b64: String) -> HttpInV1 {
        HttpInV1 {
            method: "POST".to_string(),
            path: "/gmail/push".to_string(),
            query: query.map(str::to_string),
            headers: Vec::new(),
            body_b64,
            config: None,
            binding_id: None,
            route_hint: None,
        }
    }

    fn valid_push_body() -> String {
        let inner = json!({"emailAddress": "me@example.com", "historyId": "123"});
        let data_b64 = STANDARD.encode(inner.to_string().as_bytes());
        let envelope = json!({"message": {"data": data_b64}, "subscription": "sub"});
        STANDARD.encode(envelope.to_string().as_bytes())
    }

    #[test]
    fn handle_gmail_push_rejects_bad_token_with_403() {
        let cfg = cfg(Some("me@example.com"), Some("expected-token"));
        let http = http(Some("token=wrong-token"), valid_push_body());

        let out = handle_gmail_push(&http, &cfg);
        let parsed: HttpOutV1 = serde_json::from_slice(&out).expect("http out");

        assert_eq!(parsed.status, 403);
    }

    #[test]
    fn handle_gmail_push_rejects_missing_token_with_403() {
        let cfg = cfg(Some("me@example.com"), Some("expected-token"));
        let http = http(None, valid_push_body());

        let out = handle_gmail_push(&http, &cfg);
        let parsed: HttpOutV1 = serde_json::from_slice(&out).expect("http out");

        assert_eq!(parsed.status, 403);
    }

    #[test]
    fn handle_gmail_push_rejects_malformed_body_with_400() {
        let cfg = cfg(Some("me@example.com"), Some("expected-token"));
        let http = http(
            Some("token=expected-token"),
            "not valid base64!!".to_string(),
        );

        let out = handle_gmail_push(&http, &cfg);
        let parsed: HttpOutV1 = serde_json::from_slice(&out).expect("http out");

        assert_eq!(parsed.status, 400);
    }

    #[test]
    fn handle_gmail_push_rejects_valid_base64_non_pubsub_json_with_400() {
        let cfg = cfg(Some("me@example.com"), Some("expected-token"));
        let body_b64 = STANDARD.encode(b"{\"not\":\"a pubsub push\"}");
        let http = http(Some("token=expected-token"), body_b64);

        let out = handle_gmail_push(&http, &cfg);
        let parsed: HttpOutV1 = serde_json::from_slice(&out).expect("http out");

        assert_eq!(parsed.status, 400);
    }

    #[test]
    fn extract_email_address_handles_display_name_and_bare_address() {
        assert_eq!(
            extract_email_address("Jane Doe <jane@example.com>"),
            "jane@example.com"
        );
        assert_eq!(
            extract_email_address("bare@example.com"),
            "bare@example.com"
        );
    }
}
