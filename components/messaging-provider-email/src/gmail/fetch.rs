//! Gmail API fetch layer: `users.history.list` + `users.messages.get`.
//!
//! Consumed by the Gmail ingress path added in a later task; only the
//! request builders and the authenticated fetch calls live here.
#![allow(dead_code)]

use crate::bindings::greentic::http::http_client as client;
use crate::config::ProviderConfig;
use serde_json::Value;
use urlencoding::encode as url_encode;

const GMAIL_API_BASE: &str = "https://gmail.googleapis.com/gmail/v1";

/// Builds the `users.history.list` URL for a given mailbox user and
/// starting history id, requesting only `messageAdded` history types.
pub(crate) fn history_url(user: &str, start_history_id: &str) -> String {
    format!(
        "{}/users/{}/history?startHistoryId={}&historyTypes=messageAdded",
        GMAIL_API_BASE,
        url_encode(user),
        url_encode(start_history_id)
    )
}

/// Builds the `users.messages.get` URL (full format) for a mailbox user
/// and message id.
pub(crate) fn message_url(user: &str, id: &str) -> String {
    format!(
        "{}/users/{}/messages/{}?format=full",
        GMAIL_API_BASE,
        url_encode(user),
        url_encode(id)
    )
}

/// Lists new message ids from Gmail history since `start_history_id`,
/// deduped and in first-seen order.
pub(crate) fn list_history(
    token: &str,
    cfg: &ProviderConfig,
    start_history_id: &str,
) -> Result<Vec<String>, String> {
    let user = gmail_user(cfg)?;
    let url = history_url(&user, start_history_id);
    let body = gmail_get(token, &url)?;
    Ok(extract_added_message_ids(&body))
}

/// Fetches a single Gmail message in full format.
pub(crate) fn get_message(token: &str, cfg: &ProviderConfig, id: &str) -> Result<Value, String> {
    let user = gmail_user(cfg)?;
    let url = message_url(&user, id);
    gmail_get(token, &url)
}

fn gmail_user(cfg: &ProviderConfig) -> Result<String, String> {
    cfg.gmail_user
        .as_deref()
        .filter(|s| !s.is_empty())
        .map(str::to_string)
        .ok_or_else(|| "missing gmail_user".to_string())
}

/// Extracts `history[].messagesAdded[].message.id` from a `history.list`
/// response body, deduping while preserving first-seen order.
fn extract_added_message_ids(history_response: &Value) -> Vec<String> {
    let mut seen = std::collections::HashSet::new();
    let mut ids = Vec::new();
    let Some(history) = history_response.get("history").and_then(Value::as_array) else {
        return ids;
    };
    for entry in history {
        let Some(added) = entry.get("messagesAdded").and_then(Value::as_array) else {
            continue;
        };
        for item in added {
            let Some(id) = item
                .get("message")
                .and_then(|message| message.get("id"))
                .and_then(Value::as_str)
            else {
                continue;
            };
            if seen.insert(id.to_string()) {
                ids.push(id.to_string());
            }
        }
    }
    ids
}

fn gmail_get(token: &str, url: &str) -> Result<Value, String> {
    let request = client::Request {
        method: "GET".into(),
        url: url.to_string(),
        headers: vec![("Authorization".into(), format!("Bearer {token}"))],
        body: None,
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
        _ => return Ok(Value::Null),
    };
    serde_json::from_slice(&body).map_err(|e| format!("gmail response decode failed: {e}"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn history_url_builds_expected_query() {
        let url = history_url("me@example.com", "123456");

        assert_eq!(
            url,
            "https://gmail.googleapis.com/gmail/v1/users/me%40example.com/history?startHistoryId=123456&historyTypes=messageAdded"
        );
    }

    #[test]
    fn history_url_encodes_special_characters_in_user_and_history_id() {
        let url = history_url("me", "h/1 2");

        assert_eq!(
            url,
            "https://gmail.googleapis.com/gmail/v1/users/me/history?startHistoryId=h%2F1%202&historyTypes=messageAdded"
        );
    }

    #[test]
    fn message_url_builds_expected_query() {
        let url = message_url("me@example.com", "18abc123");

        assert_eq!(
            url,
            "https://gmail.googleapis.com/gmail/v1/users/me%40example.com/messages/18abc123?format=full"
        );
    }

    fn config(user: Option<&str>) -> ProviderConfig {
        let mut value = json!({
            "public_base_url": "https://mail.example.com",
            "host": "smtp.example.com",
            "username": "mailer",
            "from_address": "bot@example.com",
            "kind": "gmail"
        });
        if let Some(user) = user {
            value["gmail_user"] = json!(user);
        }
        serde_json::from_value(value).expect("config")
    }

    #[test]
    fn list_history_requires_gmail_user() {
        let cfg = config(None);

        let err = list_history("token", &cfg, "1")
            .err()
            .expect("expected missing gmail_user error");

        assert_eq!(err, "missing gmail_user");
    }

    #[test]
    fn get_message_requires_gmail_user() {
        let cfg = config(None);

        let err = get_message("token", &cfg, "18abc")
            .err()
            .expect("expected missing gmail_user error");

        assert_eq!(err, "missing gmail_user");
    }

    #[test]
    fn get_message_requires_non_empty_gmail_user() {
        let cfg = config(Some(""));

        let err = get_message("token", &cfg, "18abc")
            .err()
            .expect("expected missing gmail_user error");

        assert_eq!(err, "missing gmail_user");
    }

    #[test]
    fn extract_added_message_ids_dedups_preserving_first_seen_order() {
        let response = json!({
            "history": [
                {
                    "id": "1",
                    "messagesAdded": [
                        {"message": {"id": "m1", "threadId": "t1"}},
                        {"message": {"id": "m2", "threadId": "t1"}}
                    ]
                },
                {
                    "id": "2",
                    "messagesAdded": [
                        {"message": {"id": "m2", "threadId": "t1"}},
                        {"message": {"id": "m3", "threadId": "t2"}}
                    ]
                }
            ]
        });

        let ids = extract_added_message_ids(&response);

        assert_eq!(ids, vec!["m1", "m2", "m3"]);
    }

    #[test]
    fn extract_added_message_ids_handles_missing_history_key() {
        let ids = extract_added_message_ids(&json!({}));

        assert!(ids.is_empty());
    }

    #[test]
    fn extract_added_message_ids_ignores_entries_without_messages_added() {
        let response = json!({
            "history": [
                {"id": "1", "labelsAdded": []},
                {"id": "2", "messagesAdded": [{"message": {"id": "m9"}}]}
            ]
        });

        let ids = extract_added_message_ids(&response);

        assert_eq!(ids, vec!["m9"]);
    }
}
