//! Slack app manifest webhook wiring (`setup_webhook`).
//!
//! Updates a Slack app's `event_subscriptions.request_url` and
//! `interactivity.request_url` so that Slack delivers events to the operator's
//! ingress endpoint. Uses Slack's `apps.manifest.export` and
//! `apps.manifest.update` APIs with a configuration token.

use provider_common::helpers::json_bytes;
use serde_json::{Value, json};

use crate::bindings::greentic::http::http_client as client;

/// Handle `setup_webhook` op — updates Slack app manifest with webhook URLs.
///
/// Input JSON:
/// ```json
/// {
///   "slack_app_id": "A07XXXXXX",
///   "slack_configuration_token": "xoxe.xoxp-...",
///   "public_base_url": "https://example.ngrok-free.app",
///   "provider_id": "messaging-slack",
///   "tenant": "demo",
///   "team": "default"
/// }
/// ```
///
/// Flow: export manifest → update URLs → push manifest
pub(crate) fn setup_webhook(input_json: &[u8]) -> Vec<u8> {
    let parsed: Value = match serde_json::from_slice(input_json) {
        Ok(val) => val,
        Err(err) => {
            return json_bytes(&json!({"ok": false, "error": format!("invalid json: {err}")}));
        }
    };

    let app_id = parsed
        .get("slack_app_id")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|s| !s.is_empty());
    let config_token = parsed
        .get("slack_configuration_token")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|s| !s.is_empty());

    let (app_id, config_token) = match (app_id, config_token) {
        (Some(a), Some(t)) => (a, t),
        _ => {
            return json_bytes(&json!({
                "ok": false,
                "error": "slack_app_id and slack_configuration_token required"
            }));
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

    let provider_id = parsed
        .get("provider_id")
        .and_then(Value::as_str)
        .unwrap_or("messaging-slack");
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

    // Step 1: Export current manifest
    let export_resp = client::send(
        &client::Request {
            method: "POST".to_string(),
            url: "https://slack.com/api/apps.manifest.export".to_string(),
            headers: vec![
                (
                    "Authorization".to_string(),
                    format!("Bearer {config_token}"),
                ),
                ("Content-Type".to_string(), "application/json".to_string()),
            ],
            body: Some(serde_json::to_vec(&json!({"app_id": app_id})).unwrap_or_default()),
        },
        None,
        None,
    );
    let export_body: Value = match export_resp {
        Ok(resp) => {
            let body = resp.body.unwrap_or_default();
            serde_json::from_slice(&body).unwrap_or(Value::Null)
        }
        Err(err) => {
            return json_bytes(
                &json!({"ok": false, "error": format!("manifest export failed: {}", err.message)}),
            );
        }
    };
    if export_body.get("ok").and_then(Value::as_bool) != Some(true) {
        let err = export_body
            .get("error")
            .and_then(Value::as_str)
            .unwrap_or("unknown");
        return json_bytes(&json!({"ok": false, "error": format!("manifest export error: {err}")}));
    }
    let mut manifest = match export_body.get("manifest").cloned() {
        Some(m) => m,
        None => {
            return json_bytes(
                &json!({"ok": false, "error": "manifest export response missing manifest field"}),
            );
        }
    };

    // Step 2: Update manifest URLs in-place
    update_manifest_urls(&mut manifest, &webhook_url);

    // Step 3: Push updated manifest
    let update_resp = client::send(
        &client::Request {
            method: "POST".to_string(),
            url: "https://slack.com/api/apps.manifest.update".to_string(),
            headers: vec![
                (
                    "Authorization".to_string(),
                    format!("Bearer {config_token}"),
                ),
                ("Content-Type".to_string(), "application/json".to_string()),
            ],
            body: Some(
                serde_json::to_vec(&json!({"app_id": app_id, "manifest": manifest}))
                    .unwrap_or_default(),
            ),
        },
        None,
        None,
    );
    let update_body: Value = match update_resp {
        Ok(resp) => {
            let body = resp.body.unwrap_or_default();
            serde_json::from_slice(&body).unwrap_or(Value::Null)
        }
        Err(err) => {
            return json_bytes(
                &json!({"ok": false, "error": format!("manifest update failed: {}", err.message)}),
            );
        }
    };
    let ok = update_body.get("ok").and_then(Value::as_bool) == Some(true);
    json_bytes(&json!({
        "ok": ok,
        "webhook_url": webhook_url,
        "slack_response": update_body,
    }))
}

/// Update Slack manifest JSON with webhook URLs for event subscriptions
/// and interactivity.
fn update_manifest_urls(manifest: &mut Value, webhook_url: &str) {
    let settings = manifest
        .as_object_mut()
        .and_then(|m| {
            m.entry("settings")
                .or_insert_with(|| json!({}))
                .as_object_mut()
        })
        .map(|s| s as &mut serde_json::Map<String, Value>);
    let Some(settings) = settings else { return };

    // event_subscriptions.request_url
    let event_subs = settings
        .entry("event_subscriptions")
        .or_insert_with(|| json!({}));
    if let Some(obj) = event_subs.as_object_mut() {
        obj.insert("request_url".to_string(), json!(webhook_url));
    }

    // interactivity.request_url + is_enabled
    let interactivity = settings.entry("interactivity").or_insert_with(|| json!({}));
    if let Some(obj) = interactivity.as_object_mut() {
        obj.insert("request_url".to_string(), json!(webhook_url));
        obj.insert("is_enabled".to_string(), json!(true));
    }
}
