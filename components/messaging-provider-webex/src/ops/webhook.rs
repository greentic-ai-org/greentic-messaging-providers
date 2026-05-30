//! `setup_webhook` operation for Webex event delivery.
//!
//! This registers Webex -> Greentic callbacks with the Webex Webhooks API.

use provider_common::helpers::json_bytes;
use serde_json::{Value, json};

use crate::DEFAULT_API_BASE;
use crate::bindings::greentic::http::http_client as client;
#[cfg(not(test))]
use crate::config::get_secret_string;
use crate::config::load_config;
#[cfg(not(test))]
use crate::{DEFAULT_TOKEN_KEY, DEFAULT_WEBHOOK_SECRET_KEY};

#[derive(Debug, Clone, PartialEq, Eq)]
struct WebhookSpec {
    name: String,
    target_url: String,
    resource: &'static str,
    event: &'static str,
    filter: Option<String>,
    secret: Option<String>,
}

/// Handle `setup_webhook` op.
pub(crate) fn setup_webhook(input_json: &[u8]) -> Vec<u8> {
    let parsed: Value = match serde_json::from_slice(input_json) {
        Ok(val) => val,
        Err(err) => {
            return json_bytes(&json!({"ok": false, "error": format!("invalid json: {err}")}));
        }
    };

    let cfg = load_config(&parsed).ok();
    let token = match resolve_token(&parsed, cfg.as_ref()) {
        Ok(token) => token,
        Err(err) => return json_bytes(&json!({"ok": false, "error": err})),
    };
    let public_base_url = match resolve_public_base_url(&parsed, cfg.as_ref()) {
        Ok(url) => url,
        Err(err) => return json_bytes(&json!({"ok": false, "error": err})),
    };
    let api_base = resolve_api_base_url(&parsed, cfg.as_ref());
    let tenant = input_string(&parsed, "tenant").unwrap_or_else(|| "default".to_string());
    let channel = input_string(&parsed, "channel")
        .or_else(|| input_string(&parsed, "team"))
        .or_else(|| input_string(&parsed, "provider_instance_id"))
        .unwrap_or_else(|| "default".to_string());
    let instance = sanitize_name_part(&format!("{tenant}-{channel}"));
    let target_url = build_target_url(&public_base_url, &tenant, &channel);
    let room_id = input_string(&parsed, "room_id")
        .or_else(|| input_string(&parsed, "webex_room_id"))
        .or_else(|| cfg.as_ref().and_then(|c| c.default_room_id.clone()));
    let secret = input_string(&parsed, "webhook_secret")
        .or_else(|| cfg.as_ref().and_then(|c| c.webhook_secret.clone()))
        .or_else(resolve_webhook_secret);

    let specs = webhook_specs(
        &instance,
        &target_url,
        room_id.as_deref(),
        secret.as_deref(),
    );
    let existing = match list_webhooks(&api_base, &token) {
        Ok(list) => list,
        Err(err) => {
            return json_bytes(&json!({
                "ok": false,
                "error": format!("list webex webhooks failed: {err}")
            }));
        }
    };

    let mut all_ok = true;
    let mut results = Vec::new();
    for spec in specs {
        let result = reconcile_webhook(&api_base, &token, &existing, &spec);
        if result.get("ok").and_then(Value::as_bool) != Some(true) {
            all_ok = false;
        }
        results.push(result);
    }

    json_bytes(&json!({
        "ok": all_ok,
        "target_url": target_url,
        "webhook_url": target_url,
        "webhooks": results,
    }))
}

fn resolve_token(parsed: &Value, cfg: Option<&crate::ProviderConfig>) -> Result<String, String> {
    input_string(parsed, "bot_token")
        .or_else(|| input_string(parsed, "webex_bot_token"))
        .or_else(|| cfg.and_then(|c| c.bot_token.clone()))
        .or_else(resolve_token_secret)
        .ok_or_else(|| {
            "missing Webex bot token (bot_token, webex_bot_token, or WEBEX_BOT_TOKEN secret)"
                .to_string()
        })
}

#[cfg(not(test))]
fn resolve_token_secret() -> Option<String> {
    get_secret_string(DEFAULT_TOKEN_KEY).ok()
}

#[cfg(test)]
fn resolve_token_secret() -> Option<String> {
    None
}

#[cfg(not(test))]
fn resolve_webhook_secret() -> Option<String> {
    get_secret_string(DEFAULT_WEBHOOK_SECRET_KEY).ok()
}

#[cfg(test)]
fn resolve_webhook_secret() -> Option<String> {
    None
}

fn resolve_public_base_url(
    parsed: &Value,
    cfg: Option<&crate::ProviderConfig>,
) -> Result<String, String> {
    let public_base_url = input_string(parsed, "public_base_url")
        .or_else(|| cfg.map(|c| c.public_base_url.clone()))
        .map(|value| value.trim().trim_end_matches('/').to_string())
        .filter(|value| !value.is_empty() && value != "https://invalid.local")
        .ok_or_else(|| "missing public_base_url for Webex webhook setup".to_string())?;
    if !public_base_url.starts_with("https://") {
        return Err("public_base_url must be an https:// URL for Webex webhooks".to_string());
    }
    Ok(public_base_url)
}

fn resolve_api_base_url(parsed: &Value, cfg: Option<&crate::ProviderConfig>) -> String {
    input_string(parsed, "api_base_url")
        .or_else(|| cfg.and_then(|c| c.api_base_url.clone()))
        .map(|value| value.trim().trim_end_matches('/').to_string())
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| DEFAULT_API_BASE.to_string())
}

fn input_string(parsed: &Value, key: &str) -> Option<String> {
    parsed
        .get(key)
        .or_else(|| parsed.get("config").and_then(|cfg| cfg.get(key)))
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(ToOwned::to_owned)
}

fn build_target_url(public_base_url: &str, tenant: &str, channel: &str) -> String {
    format!(
        "{}/v1/messaging/ingress/messaging-webex/{}/{}",
        public_base_url.trim_end_matches('/'),
        url_segment(tenant),
        url_segment(channel),
    )
}

fn webhook_specs(
    instance: &str,
    target_url: &str,
    room_id: Option<&str>,
    secret: Option<&str>,
) -> Vec<WebhookSpec> {
    let messages_filter = room_id
        .map(|room| format!("roomId={room}"))
        .unwrap_or_else(|| "mentionedPeople=me".to_string());
    let actions_filter = room_id.map(|room| format!("roomId={room}"));
    vec![
        WebhookSpec {
            name: format!("greentic-webex-{instance}-messages-created"),
            target_url: target_url.to_string(),
            resource: "messages",
            event: "created",
            filter: Some(messages_filter),
            secret: secret.map(ToOwned::to_owned),
        },
        WebhookSpec {
            name: format!("greentic-webex-{instance}-attachment-actions-created"),
            target_url: target_url.to_string(),
            resource: "attachmentActions",
            event: "created",
            filter: actions_filter,
            secret: secret.map(ToOwned::to_owned),
        },
    ]
}

fn list_webhooks(api_base: &str, token: &str) -> Result<Vec<Value>, String> {
    let resp = client::send(
        &client::Request {
            method: "GET".to_string(),
            url: format!("{api_base}/webhooks"),
            headers: vec![("Authorization".to_string(), format!("Bearer {token}"))],
            body: None,
        },
        None,
        None,
    )
    .map_err(|e| format!("http error: {}", e.message))?;
    let body_bytes = resp.body.unwrap_or_default();
    let body: Value = serde_json::from_slice(&body_bytes).unwrap_or(Value::Null);
    if !(200..300).contains(&(resp.status as u32)) {
        return Err(format!(
            "status {} body={}",
            resp.status,
            String::from_utf8_lossy(&body_bytes)
        ));
    }
    Ok(body
        .get("items")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default())
}

fn reconcile_webhook(api_base: &str, token: &str, existing: &[Value], spec: &WebhookSpec) -> Value {
    let found = existing.iter().find(|wh| same_owned_webhook(wh, spec));
    match found {
        Some(wh) if existing_matches(wh, spec) => json!({
            "ok": true,
            "action": "reuse",
            "webhook_id": wh.get("id").and_then(Value::as_str).unwrap_or_default(),
            "name": spec.name,
            "resource": spec.resource,
            "event": spec.event,
            "filter": spec.filter,
            "targetUrl": spec.target_url,
        }),
        Some(wh) => {
            let webhook_id = wh.get("id").and_then(Value::as_str).unwrap_or_default();
            if !webhook_id.is_empty() {
                let delete = delete_webhook(api_base, token, webhook_id, spec);
                if delete.get("ok").and_then(Value::as_bool) != Some(true) {
                    return delete;
                }
            }
            create_webhook(api_base, token, spec, "replace")
        }
        None => create_webhook(api_base, token, spec, "create"),
    }
}

fn same_owned_webhook(wh: &Value, spec: &WebhookSpec) -> bool {
    wh.get("name").and_then(Value::as_str) == Some(spec.name.as_str())
        && wh.get("resource").and_then(Value::as_str) == Some(spec.resource)
        && wh.get("event").and_then(Value::as_str) == Some(spec.event)
        && value_str(wh, "filter") == spec.filter.as_deref().unwrap_or("")
}

fn existing_matches(wh: &Value, spec: &WebhookSpec) -> bool {
    same_owned_webhook(wh, spec)
        && wh.get("targetUrl").and_then(Value::as_str) == Some(spec.target_url.as_str())
}

fn create_webhook(api_base: &str, token: &str, spec: &WebhookSpec, action: &str) -> Value {
    let body = create_webhook_payload(spec);
    let resp = match client::send(
        &client::Request {
            method: "POST".to_string(),
            url: format!("{api_base}/webhooks"),
            headers: vec![
                ("Authorization".to_string(), format!("Bearer {token}")),
                ("Content-Type".to_string(), "application/json".to_string()),
            ],
            body: Some(serde_json::to_vec(&body).unwrap_or_default()),
        },
        None,
        None,
    ) {
        Ok(r) => r,
        Err(e) => {
            return json!({
                "ok": false,
                "resource": spec.resource,
                "event": spec.event,
                "error": format!("create {}.{} failed: {}", spec.resource, spec.event, e.message)
            });
        }
    };
    response_result(action, spec, resp)
}

fn delete_webhook(api_base: &str, token: &str, webhook_id: &str, spec: &WebhookSpec) -> Value {
    let resp = match client::send(
        &client::Request {
            method: "DELETE".to_string(),
            url: format!("{api_base}/webhooks/{webhook_id}"),
            headers: vec![("Authorization".to_string(), format!("Bearer {token}"))],
            body: None,
        },
        None,
        None,
    ) {
        Ok(r) => r,
        Err(e) => {
            return json!({
                "ok": false,
                "action": "delete",
                "webhook_id": webhook_id,
                "resource": spec.resource,
                "event": spec.event,
                "error": format!("delete {}.{} failed: {}", spec.resource, spec.event, e.message)
            });
        }
    };
    let ok = (200..300).contains(&(resp.status as u32));
    json!({
        "ok": ok,
        "action": "delete",
        "webhook_id": webhook_id,
        "resource": spec.resource,
        "event": spec.event,
        "http_status": resp.status,
        "webex_response": parse_response_body(resp.body),
    })
}

fn create_webhook_payload(spec: &WebhookSpec) -> Value {
    let mut body = json!({
        "name": spec.name,
        "targetUrl": spec.target_url,
        "resource": spec.resource,
        "event": spec.event,
    });
    if let Some(filter) = spec.filter.as_deref().filter(|value| !value.is_empty()) {
        body["filter"] = json!(filter);
    }
    if let Some(secret) = spec.secret.as_deref().filter(|value| !value.is_empty()) {
        body["secret"] = json!(secret);
    }
    body
}

fn response_result(action: &str, spec: &WebhookSpec, resp: client::Response) -> Value {
    let status = resp.status;
    let body = parse_response_body(resp.body);
    let ok = (200..300).contains(&(status as u32));
    json!({
        "ok": ok,
        "action": action,
        "webhook_id": body.get("id").and_then(Value::as_str).unwrap_or_default(),
        "name": spec.name,
        "resource": spec.resource,
        "event": spec.event,
        "filter": spec.filter,
        "targetUrl": spec.target_url,
        "http_status": status,
        "webex_response": body,
    })
}

fn parse_response_body(body: Option<Vec<u8>>) -> Value {
    body.and_then(|bytes| serde_json::from_slice(&bytes).ok())
        .unwrap_or(Value::Null)
}

fn value_str<'a>(value: &'a Value, key: &str) -> &'a str {
    value.get(key).and_then(Value::as_str).unwrap_or("")
}

fn sanitize_name_part(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    for ch in input.chars() {
        if ch.is_ascii_alphanumeric() || ch == '-' || ch == '_' {
            out.push(ch);
        } else {
            out.push('-');
        }
    }
    out.trim_matches('-').to_string()
}

fn url_segment(input: &str) -> String {
    input
        .bytes()
        .flat_map(|byte| match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                vec![byte as char]
            }
            _ => format!("%{byte:02X}").chars().collect(),
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builds_messages_created_payload_with_mentioned_people_filter() {
        let specs = webhook_specs(
            "demo-default",
            "https://example.com/hook",
            None,
            Some("secret"),
        );
        let payload = create_webhook_payload(&specs[0]);

        assert_eq!(
            payload["name"],
            "greentic-webex-demo-default-messages-created"
        );
        assert_eq!(payload["targetUrl"], "https://example.com/hook");
        assert_eq!(payload["resource"], "messages");
        assert_eq!(payload["event"], "created");
        assert_eq!(payload["filter"], "mentionedPeople=me");
        assert_eq!(payload["secret"], "secret");
    }

    #[test]
    fn builds_attachment_actions_payload_with_room_filter_when_configured() {
        let specs = webhook_specs(
            "demo-room",
            "https://example.com/hook",
            Some("room-1"),
            None,
        );
        let payload = create_webhook_payload(&specs[1]);

        assert_eq!(
            payload["name"],
            "greentic-webex-demo-room-attachment-actions-created"
        );
        assert_eq!(payload["resource"], "attachmentActions");
        assert_eq!(payload["event"], "created");
        assert_eq!(payload["filter"], "roomId=room-1");
        assert!(payload.get("secret").is_none());
    }

    #[test]
    fn reuses_existing_matching_webhook() {
        let spec = webhook_specs("demo", "https://example.com/hook", None, None).remove(0);
        let existing = json!({
            "id": "wh-1",
            "name": spec.name,
            "resource": spec.resource,
            "event": spec.event,
            "filter": spec.filter,
            "targetUrl": spec.target_url
        });

        assert!(same_owned_webhook(&existing, &spec));
        assert!(existing_matches(&existing, &spec));
    }

    #[test]
    fn missing_token_returns_clear_error_without_network() {
        let out: Value = serde_json::from_slice(&setup_webhook(
            br#"{"public_base_url":"https://example.com"}"#,
        ))
        .expect("json");

        assert_eq!(out["ok"], false);
        assert!(
            out["error"]
                .as_str()
                .unwrap_or_default()
                .contains("bot token")
        );
    }

    #[test]
    fn target_url_uses_standard_provider_ingress_route() {
        assert_eq!(
            build_target_url("https://host.example/", "tenant a", "channel/b"),
            "https://host.example/v1/messaging/ingress/messaging-webex/tenant%20a/channel%2Fb"
        );
    }
}
