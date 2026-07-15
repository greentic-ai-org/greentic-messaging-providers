//! Step 3 (Bot Framework Connector variant): reply into a Teams conversation
//! via `{serviceUrl}/v3/conversations/{id}/activities`.
//!
//! The bot pack (Bot Framework) routes here instead of Microsoft Graph channel
//! sends. `handle_send` selects this path when the outbound carries a Bot
//! Framework conversation context (service_url + conversation_id) and the
//! config has a bot identity.

use provider_common::helpers::json_bytes;
use serde_json::{Value, json};

use crate::PROVIDER_TYPE;
use crate::auth::acquire_bot_token;
use crate::bindings::greentic::http::http_client as client;
use crate::config::ProviderConfig;

pub(crate) fn bot_connector_send(
    cfg: &ProviderConfig,
    service_url: &str,
    conversation_id: &str,
    text: &str,
    content_type: &str,
    card: Option<&Value>,
    status: &str,
) -> Vec<u8> {
    let token = match acquire_bot_token(cfg) {
        Ok(token) => token,
        Err(err) => return json_bytes(&json!({"ok": false, "error": err})),
    };
    let request = client::Request {
        method: "POST".into(),
        url: connector_url(service_url, conversation_id),
        headers: vec![
            ("Content-Type".into(), "application/json".into()),
            ("Authorization".into(), format!("Bearer {token}")),
        ],
        body: Some(
            serde_json::to_vec(&connector_activity(text, content_type, card))
                .unwrap_or_else(|_| b"{}".to_vec()),
        ),
    };
    let resp = match client::send(&request, None, None) {
        Ok(resp) => resp,
        Err(err) => {
            crate::wlog(&format!(
                "bot connector POST to {service_url} transport error: {}",
                err.message
            ));
            return json_bytes(&json!({
                "ok": false,
                "error": format!("transport error: {}", err.message),
                "retryable": true
            }));
        }
    };
    if resp.status < 200 || resp.status >= 300 {
        let body_json: Value = resp
            .body
            .as_ref()
            .and_then(|b| serde_json::from_slice(b).ok())
            .unwrap_or(Value::Null);
        let (error, retryable) = classify_connector_error(resp.status, &body_json);
        crate::wlog(&format!(
            "bot connector POST to {service_url} → status {}: {error}",
            resp.status
        ));
        return json_bytes(&json!({
            "ok": false,
            "error": error,
            "retryable": retryable,
            "response": body_json,
        }));
    }
    let body_json: Value = resp
        .body
        .and_then(|b| serde_json::from_slice(&b).ok())
        .unwrap_or(Value::Null);
    let message_id = body_json
        .get("id")
        .and_then(Value::as_str)
        .map(ToOwned::to_owned)
        .unwrap_or_else(|| "bot-activity".to_string());
    json_bytes(&json!({
        "ok": true,
        "status": status,
        "provider_type": PROVIDER_TYPE,
        "message_id": message_id,
        "provider_message_id": format!("teams:{message_id}"),
        "response": body_json,
    }))
}

pub(crate) fn connector_url(service_url: &str, conversation_id: &str) -> String {
    format!(
        "{}/v3/conversations/{}/activities",
        service_url.trim_end_matches('/'),
        conversation_id
    )
}

fn connector_activity(text: &str, content_type: &str, card: Option<&Value>) -> Value {
    if let Some(card) = card {
        return json!({
            "type": "message",
            "attachments": [{
                "contentType": "application/vnd.microsoft.card.adaptive",
                "content": card
            }]
        });
    }
    if content_type.eq_ignore_ascii_case("html") {
        json!({"type": "message", "textFormat": "xml", "text": text})
    } else {
        json!({"type": "message", "text": text})
    }
}

fn classify_connector_error(status: u16, body: &Value) -> (String, bool) {
    let detail = body
        .get("error")
        .and_then(|err| err.get("message"))
        .and_then(Value::as_str)
        .unwrap_or_default();
    let prefix = match status {
        401 => "expired or invalid Bot Framework token",
        403 => "bot not authorized for this conversation",
        404 => "unknown Bot Framework conversation or service url",
        429 => "Bot Framework rate limit exceeded",
        _ => "Bot Framework request failed",
    };
    let message = if detail.is_empty() {
        format!("{prefix} (status {status})")
    } else {
        format!("{prefix} (status {status}): {detail}")
    };
    (message, status == 429 || status >= 500)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn connector_url_joins_and_trims() {
        assert_eq!(
            connector_url("https://smba.trafficmanager.net/teams/", "conv:1"),
            "https://smba.trafficmanager.net/teams/v3/conversations/conv:1/activities"
        );
    }

    #[test]
    fn connector_activity_text_and_card() {
        let text = connector_activity("hi", "text", None);
        assert_eq!(text["type"], "message");
        assert_eq!(text["text"], "hi");
        assert!(text.get("attachments").is_none());

        let html = connector_activity("<b>hi</b>", "html", None);
        assert_eq!(html["textFormat"], "xml");

        let card_json = json!({"type": "AdaptiveCard"});
        let card = connector_activity("fallback", "text", Some(&card_json));
        assert_eq!(
            card["attachments"][0]["contentType"],
            "application/vnd.microsoft.card.adaptive"
        );
        assert_eq!(card["attachments"][0]["content"]["type"], "AdaptiveCard");
    }

    #[test]
    fn classify_connector_error_marks_retryable() {
        assert!(!classify_connector_error(401, &Value::Null).1);
        assert!(classify_connector_error(429, &Value::Null).1);
        assert!(classify_connector_error(503, &Value::Null).1);
    }
}
