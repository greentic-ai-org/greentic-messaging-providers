//! Gmail Pub/Sub push notification parsing and inbound verification.
//!
//! Consumed by `gmail::envelope::handle_gmail_push`.

use base64::{Engine, engine::general_purpose::STANDARD};
use greentic_types::messaging::universal_dto::HttpInV1;
use serde::Deserialize;
use urlencoding::decode as url_decode;

/// A decoded Gmail Pub/Sub push notification.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct PushNotification {
    pub email_address: String,
    pub history_id: String,
}

#[derive(Deserialize)]
struct PubsubPushEnvelope {
    message: PubsubMessage,
}

#[derive(Deserialize)]
struct PubsubMessage {
    data: String,
}

#[derive(Deserialize)]
struct GmailPushPayload {
    #[serde(rename = "emailAddress")]
    email_address: String,
    #[serde(rename = "historyId")]
    history_id: HistoryId,
}

#[derive(Deserialize)]
#[serde(untagged)]
enum HistoryId {
    Number(u64),
    Text(String),
}

impl HistoryId {
    fn into_string(self) -> String {
        match self {
            HistoryId::Number(value) => value.to_string(),
            HistoryId::Text(value) => value,
        }
    }
}

/// Parses a raw Pub/Sub push HTTP body into a [`PushNotification`].
///
/// Expected shape: `{"message":{"data":"<base64>"},"subscription":"..."}` where
/// the base64-decoded `data` is `{"emailAddress":"...","historyId":...}`
/// (`historyId` may be a JSON number or string).
pub(crate) fn parse_pubsub_push(body: &[u8]) -> Result<PushNotification, String> {
    let envelope: PubsubPushEnvelope = serde_json::from_slice(body)
        .map_err(|err| format!("invalid pubsub push envelope: {err}"))?;
    let decoded = STANDARD
        .decode(envelope.message.data)
        .map_err(|err| format!("invalid pubsub message data base64: {err}"))?;
    let payload: GmailPushPayload = serde_json::from_slice(&decoded)
        .map_err(|err| format!("invalid gmail push payload json: {err}"))?;
    Ok(PushNotification {
        email_address: payload.email_address,
        history_id: payload.history_id.into_string(),
    })
}

fn query_param_value(query: &str, key: &str) -> Option<String> {
    for part in query.split('&') {
        let mut kv = part.splitn(2, '=');
        if let Some(k) = kv.next()
            && k == key
            && let Some(v) = kv.next()
        {
            return url_decode(v).ok().map(|cow| cow.into_owned());
        }
    }
    None
}

fn bearer_token(http: &HttpInV1) -> Option<String> {
    http.headers
        .iter()
        .find(|header| header.name.eq_ignore_ascii_case("authorization"))
        .and_then(|header| header.value.strip_prefix("Bearer "))
        .map(str::to_string)
}

fn constant_time_eq(a: &str, b: &str) -> bool {
    if a.len() != b.len() {
        return false;
    }
    let mut diff: u8 = 0;
    for (x, y) in a.bytes().zip(b.bytes()) {
        diff |= x ^ y;
    }
    diff == 0
}

/// Verifies an inbound Pub/Sub push against the tenant's shared verification
/// token. The token may arrive as a `?token=` query parameter or an
/// `Authorization: Bearer <token>` header. Missing or mismatched tokens (or
/// an empty `expected_token`) fail closed and return `false`.
pub(crate) fn verify_push(http: &HttpInV1, expected_token: &str) -> bool {
    if expected_token.is_empty() {
        return false;
    }
    let query_token = http
        .query
        .as_deref()
        .and_then(|query| query_param_value(query, "token"));
    let candidate = query_token.or_else(|| bearer_token(http));
    match candidate {
        Some(token) => constant_time_eq(&token, expected_token),
        None => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use base64::{Engine, engine::general_purpose::STANDARD};
    use greentic_types::messaging::universal_dto::{Header, HttpInV1};
    use serde_json::json;

    fn wrap_push_body(inner: &serde_json::Value) -> Vec<u8> {
        let data_b64 = STANDARD.encode(inner.to_string().as_bytes());
        json!({
            "message": {
                "data": data_b64,
                "messageId": "12345",
            },
            "subscription": "projects/example/subscriptions/gmail-push",
        })
        .to_string()
        .into_bytes()
    }

    fn http(query: Option<&str>, bearer: Option<&str>) -> HttpInV1 {
        let headers = match bearer {
            Some(token) => vec![Header {
                name: "Authorization".to_string(),
                value: format!("Bearer {token}"),
            }],
            None => Vec::new(),
        };
        HttpInV1 {
            method: "POST".to_string(),
            path: "/gmail/push".to_string(),
            query: query.map(str::to_string),
            headers,
            body_b64: String::new(),
            route_hint: None,
            binding_id: None,
            config: None,
        }
    }

    #[test]
    fn parses_sample_push_body_with_string_history_id() {
        let body = wrap_push_body(&json!({
            "emailAddress": "a@b.com",
            "historyId": "123",
        }));

        let notification = parse_pubsub_push(&body).expect("valid push body should parse");

        assert_eq!(
            notification,
            PushNotification {
                email_address: "a@b.com".to_string(),
                history_id: "123".to_string(),
            }
        );
    }

    #[test]
    fn parses_sample_push_body_with_numeric_history_id() {
        let body = wrap_push_body(&json!({
            "emailAddress": "a@b.com",
            "historyId": 123,
        }));

        let notification = parse_pubsub_push(&body).expect("valid push body should parse");

        assert_eq!(
            notification,
            PushNotification {
                email_address: "a@b.com".to_string(),
                history_id: "123".to_string(),
            }
        );
    }

    #[test]
    fn rejects_non_json_body() {
        let result = parse_pubsub_push(b"not json at all");
        assert!(result.is_err());
    }

    #[test]
    fn rejects_missing_message_data() {
        let body = json!({"subscription": "projects/example/subscriptions/gmail-push"})
            .to_string()
            .into_bytes();
        let result = parse_pubsub_push(&body);
        assert!(result.is_err());
    }

    #[test]
    fn rejects_invalid_base64_data() {
        let body = json!({
            "message": {"data": "not-valid-base64!!", "messageId": "1"},
            "subscription": "sub",
        })
        .to_string()
        .into_bytes();
        let result = parse_pubsub_push(&body);
        assert!(result.is_err());
    }

    #[test]
    fn rejects_decoded_data_that_is_not_json() {
        let data_b64 = STANDARD.encode(b"not json inside");
        let body = json!({
            "message": {"data": data_b64, "messageId": "1"},
            "subscription": "sub",
        })
        .to_string()
        .into_bytes();
        let result = parse_pubsub_push(&body);
        assert!(result.is_err());
    }

    #[test]
    fn rejects_decoded_json_missing_required_fields() {
        let body = wrap_push_body(&json!({"emailAddress": "a@b.com"}));
        let result = parse_pubsub_push(&body);
        assert!(result.is_err());
    }

    #[test]
    fn verify_push_true_for_matching_query_token() {
        let http = http(Some("token=shared-secret"), None);
        assert!(verify_push(&http, "shared-secret"));
    }

    #[test]
    fn verify_push_true_for_matching_bearer_token() {
        let http = http(None, Some("shared-secret"));
        assert!(verify_push(&http, "shared-secret"));
    }

    #[test]
    fn verify_push_false_for_wrong_query_token() {
        let http = http(Some("token=wrong"), None);
        assert!(!verify_push(&http, "shared-secret"));
    }

    #[test]
    fn verify_push_false_for_wrong_bearer_token() {
        let http = http(None, Some("wrong"));
        assert!(!verify_push(&http, "shared-secret"));
    }

    #[test]
    fn verify_push_false_for_missing_token() {
        let http = http(None, None);
        assert!(!verify_push(&http, "shared-secret"));
    }

    #[test]
    fn verify_push_false_when_expected_token_is_empty() {
        let http = http(Some("token="), None);
        assert!(!verify_push(&http, ""));
    }
}
