//! `setup_webhook` operation — manages Webex webhook registrations.
//!
//! Reconciles two webhook types (`messages:created` and
//! `attachmentActions:created`) against the configured `public_base_url`. If a
//! matching webhook already exists with the correct target URL the call is a
//! no-op; otherwise it is updated or created.

use provider_common::helpers::json_bytes;
use serde_json::{Value, json};

use crate::bindings::greentic::http::http_client as client;

/// Handle `setup_webhook` op — manages Webex webhook registrations.
///
/// Creates or updates two webhooks for messages and attachment actions.
///
/// Input JSON:
/// ```json
/// {
///   "bot_token": "...",
///   "public_base_url": "https://example.ngrok-free.app",
///   "api_base_url": "https://webexapis.com/v1",  // optional
///   "provider_id": "messaging-webex",
///   "tenant": "demo",
///   "team": "default"
/// }
/// ```
pub(crate) fn setup_webhook(input_json: &[u8]) -> Vec<u8> {
    let parsed: Value = match serde_json::from_slice(input_json) {
        Ok(val) => val,
        Err(err) => {
            return json_bytes(&json!({"ok": false, "error": format!("invalid json: {err}")}));
        }
    };

    let bot_token = parsed
        .get("bot_token")
        .or_else(|| parsed.get("webex_bot_token"))
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|s| !s.is_empty());
    let bot_token = match bot_token {
        Some(t) => t,
        None => {
            return json_bytes(
                &json!({"ok": false, "error": "bot_token or webex_bot_token required"}),
            );
        }
    };

    let public_base_url = parsed
        .get("public_base_url")
        .and_then(Value::as_str)
        .unwrap_or("");
    if public_base_url.is_empty() || !public_base_url.starts_with("https://") {
        return json_bytes(
            &json!({"ok": false, "error": "public_base_url must be an https:// URL"}),
        );
    }

    let api_base = parsed
        .get("api_base_url")
        .and_then(Value::as_str)
        .filter(|s| !s.is_empty())
        .unwrap_or("https://webexapis.com/v1");
    let provider_id = parsed
        .get("provider_id")
        .and_then(Value::as_str)
        .unwrap_or("messaging-webex");
    let tenant = parsed
        .get("tenant")
        .and_then(Value::as_str)
        .unwrap_or("default");
    let team = parsed
        .get("team")
        .and_then(Value::as_str)
        .unwrap_or("default");

    let webhook_url = format!(
        "{}/v1/messaging/ingress/{}/{}/{}",
        public_base_url.trim_end_matches('/'),
        provider_id,
        tenant,
        team,
    );

    let base_name = format!("greentic:{tenant}:{team}:webex");

    // List existing webhooks
    let existing = match list_webhooks(api_base, bot_token) {
        Ok(list) => list,
        Err(err) => {
            return json_bytes(
                &json!({"ok": false, "error": format!("list webhooks failed: {err}")}),
            );
        }
    };

    // Reconcile two webhook types
    let webhook_specs = [
        (&base_name, "messages", "created"),
        (
            &format!("{base_name}:cards"),
            "attachmentActions",
            "created",
        ),
    ];
    let mut results = Vec::new();
    let mut all_ok = true;
    for (name, resource, event) in webhook_specs {
        let result = reconcile_webhook(
            api_base,
            bot_token,
            &existing,
            name,
            &webhook_url,
            resource,
            event,
        );
        let ok = result.get("ok").and_then(Value::as_bool).unwrap_or(false);
        if !ok {
            all_ok = false;
        }
        results.push(json!({
            "resource": resource,
            "event": event,
            "name": name,
            "result": result,
        }));
    }

    json_bytes(&json!({
        "ok": all_ok,
        "webhook_url": webhook_url,
        "webhooks": results,
    }))
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
    let body: Value = serde_json::from_slice(&resp.body.unwrap_or_default()).unwrap_or(Value::Null);
    Ok(body
        .get("items")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default())
}

fn reconcile_webhook(
    api_base: &str,
    token: &str,
    existing: &[Value],
    name: &str,
    target_url: &str,
    resource: &str,
    event: &str,
) -> Value {
    // Find existing webhook by name
    let found = existing.iter().find(|wh| {
        wh.get("name")
            .and_then(Value::as_str)
            .map(|n| n == name)
            .unwrap_or(false)
    });

    match found {
        Some(wh) => {
            let current_url = wh.get("targetUrl").and_then(Value::as_str).unwrap_or("");
            let webhook_id = wh.get("id").and_then(Value::as_str).unwrap_or("");
            if current_url == target_url {
                json!({"ok": true, "action": "noop", "webhook_id": webhook_id})
            } else {
                update_webhook(api_base, token, webhook_id, name, target_url)
            }
        }
        None => create_webhook(api_base, token, name, target_url, resource, event),
    }
}

fn create_webhook(
    api_base: &str,
    token: &str,
    name: &str,
    target_url: &str,
    resource: &str,
    event: &str,
) -> Value {
    let body = json!({
        "name": name,
        "targetUrl": target_url,
        "resource": resource,
        "event": event,
    });
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
        Err(e) => return json!({"ok": false, "error": format!("create failed: {}", e.message)}),
    };
    let resp_body: Value =
        serde_json::from_slice(&resp.body.unwrap_or_default()).unwrap_or(Value::Null);
    let ok = (200..300).contains(&(resp.status as u32));
    let webhook_id = resp_body.get("id").and_then(Value::as_str).unwrap_or("");
    json!({
        "ok": ok,
        "action": "create",
        "webhook_id": webhook_id,
        "http_status": resp.status,
    })
}

fn update_webhook(
    api_base: &str,
    token: &str,
    webhook_id: &str,
    name: &str,
    target_url: &str,
) -> Value {
    let body = json!({
        "name": name,
        "targetUrl": target_url,
    });
    let resp = match client::send(
        &client::Request {
            method: "PUT".to_string(),
            url: format!("{api_base}/webhooks/{webhook_id}"),
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
        Err(e) => return json!({"ok": false, "error": format!("update failed: {}", e.message)}),
    };
    let ok = (200..300).contains(&(resp.status as u32));
    json!({
        "ok": ok,
        "action": "update",
        "webhook_id": webhook_id,
        "http_status": resp.status,
    })
}
