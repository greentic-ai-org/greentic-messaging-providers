//! Slack app manifest webhook wiring (`setup_webhook`).
//!
//! Updates a Slack app's `event_subscriptions.request_url` and
//! `interactivity.request_url` so that Slack delivers events to the operator's
//! ingress endpoint. Uses Slack's `apps.manifest.export` and
//! `apps.manifest.update` APIs with a configuration token.

use provider_common::helpers::json_bytes;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;

use crate::bindings::greentic::http::http_client as client;
use crate::config::get_secret_string;
use crate::{
    DEFAULT_APP_ID_KEY, DEFAULT_CLIENT_ID_KEY, DEFAULT_CLIENT_SECRET_KEY,
    DEFAULT_CONFIG_ACCESS_TOKEN_KEY, DEFAULT_CONFIG_REFRESH_TOKEN_KEY, DEFAULT_SIGNING_SECRET_KEY,
};

const DEFAULT_INSTANCE_BUNDLE_ID: &str = "greentic";

/// Rotate an expired Slack configuration token using the refresh token.
///
/// Calls `tooling.tokens.rotate` with form-urlencoded body. Returns
/// `(new_config_token, new_refresh_token)` on success.
fn rotate_config_token(refresh_token: &str) -> Result<(String, String), String> {
    let body = format!("refresh_token={}", urlencoding(refresh_token));
    let resp = client::send(
        &client::Request {
            method: "POST".to_string(),
            url: "https://slack.com/api/tooling.tokens.rotate".to_string(),
            headers: vec![(
                "Content-Type".to_string(),
                "application/x-www-form-urlencoded".to_string(),
            )],
            body: Some(body.into_bytes()),
        },
        None,
        None,
    );
    let resp_body: Value = match resp {
        Ok(r) => serde_json::from_slice(&r.body.unwrap_or_default()).unwrap_or(Value::Null),
        Err(e) => return Err(format!("token rotate request failed: {}", e.message)),
    };
    if resp_body.get("ok").and_then(Value::as_bool) != Some(true) {
        let err = resp_body
            .get("error")
            .and_then(Value::as_str)
            .unwrap_or("unknown");
        return Err(format!("token rotate failed: {err}"));
    }
    let new_token = resp_body
        .get("token")
        .and_then(Value::as_str)
        .ok_or("rotate response missing token field")?
        .to_string();
    let new_refresh = resp_body
        .get("refresh_token")
        .and_then(Value::as_str)
        .ok_or("rotate response missing refresh_token field")?
        .to_string();
    Ok((new_token, new_refresh))
}

/// Minimal percent-encoding for form-urlencoded values.
fn urlencoding(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    for b in input.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(b as char);
            }
            _ => {
                out.push('%');
                out.push(char::from(HEX[(b >> 4) as usize]));
                out.push(char::from(HEX[(b & 0x0F) as usize]));
            }
        }
    }
    out
}

const HEX: [u8; 16] = *b"0123456789ABCDEF";

/// Handle `setup_webhook` op — updates Slack app manifest with webhook URLs.
///
/// Input JSON:
/// ```json
/// {
///   "slack_app_id": "A07XXXXXX",
///   "slack_configuration_access_token": "xoxe.xoxp-...",
///   "slack_configuration_refresh_token": "xoxe-...",
///   "public_base_url": "https://example.ngrok-free.app",
///   "provider_id": "messaging-slack",
///   "tenant": "demo",
///   "team": "default"
/// }
/// ```
///
/// Flow: resolve tokens → export manifest → (refresh on auth error) → update URLs → push manifest
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
        .filter(|s| !s.is_empty())
        .map(String::from)
        .or_else(|| get_secret_string(DEFAULT_APP_ID_KEY).ok());

    // Resolve config access token from the current input/secret names only.
    let config_token_input = config_access_token_from_input(&parsed)
        .or_else(|| get_secret_string(DEFAULT_CONFIG_ACCESS_TOKEN_KEY).ok());

    // Resolve refresh token: input field → secrets store fallback.
    let refresh_token = parsed
        .get("slack_configuration_refresh_token")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(String::from)
        .or_else(|| get_secret_string(DEFAULT_CONFIG_REFRESH_TOKEN_KEY).ok());

    let config_token_input = match config_token_input {
        Some(token) => token,
        None => {
            return json_bytes(&json!({
                "ok": false,
                "error": "slack_configuration_access_token required"
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

    let input = SetupWebhookInput {
        public_base_url: public_base_url.trim_end_matches('/').to_string(),
        provider_id: parsed
            .get("provider_id")
            .and_then(Value::as_str)
            .unwrap_or("messaging-slack")
            .to_string(),
        tenant: parsed
            .get("tenant")
            .and_then(Value::as_str)
            .unwrap_or("default")
            .to_string(),
        team: parsed
            .get("team")
            .and_then(Value::as_str)
            .unwrap_or("default")
            .to_string(),
        bundle_id: parsed
            .get("bundle_id")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(ToOwned::to_owned),
        bundle_digest: parsed
            .get("bundle_digest")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(ToOwned::to_owned),
    };

    if let Some(app_id) = app_id.as_deref() {
        update_existing_app(&input, app_id, config_token_input, refresh_token.as_deref())
    } else {
        create_slack_app(&input, config_token_input, refresh_token.as_deref())
    }
}

#[derive(Debug)]
struct SetupWebhookInput {
    public_base_url: String,
    provider_id: String,
    tenant: String,
    team: String,
    bundle_id: Option<String>,
    bundle_digest: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
struct SetupAction {
    id: String,
    kind: String,
    label: String,
    provider_id: String,
    tenant: String,
    team: String,
    authorize_url: String,
    callback_path: String,
    status: String,
}

fn update_existing_app(
    input: &SetupWebhookInput,
    app_id: &str,
    config_token_input: String,
    refresh_token: Option<&str>,
) -> Vec<u8> {
    let webhook_url = build_webhook_url(input);
    let mut secrets_set = BTreeMap::new();

    let mut config_token = config_token_input;
    let export_body = match export_manifest(app_id, &config_token) {
        ExportResult::Ok(body) => body,
        ExportResult::AuthError => match try_refresh_token(refresh_token) {
            Ok(refresh) => {
                config_token = refresh.access_token;
                secrets_set.extend(refresh.secrets_set);
                match export_manifest(app_id, &config_token) {
                    ExportResult::Ok(body) => body,
                    ExportResult::AuthError => {
                        return json_bytes(&json!({
                            "ok": false,
                            "error": "manifest export auth error after token refresh"
                        }));
                    }
                    ExportResult::Err(e) => return e,
                }
            }
            Err(e) => return e,
        },
        ExportResult::Err(e) => return e,
    };

    let mut manifest = match export_body.get("manifest").cloned() {
        Some(m) => m,
        None => {
            return json_bytes(
                &json!({"ok": false, "error": "manifest export response missing manifest field"}),
            );
        }
    };

    update_manifest_urls(&mut manifest, &webhook_url);
    update_manifest_metadata(&mut manifest);

    let update_body = match update_manifest(app_id, &config_token, &manifest) {
        ManifestApiResult::Ok(body) => body,
        ManifestApiResult::AuthError => {
            return json_bytes(&json!({
                "ok": false,
                "error": "manifest update auth error"
            }));
        }
        ManifestApiResult::Err(e) => return e,
    };
    let ok = update_body.get("ok").and_then(Value::as_bool) == Some(true);
    let mut out = json!({
        "ok": ok,
        "status": if ok { "ready" } else { "error" },
        "app_status": "updated",
        "slack_app_id": app_id,
        "webhook_url": webhook_url,
        "setup_actions": [],
        "slack_response": update_body,
    });
    attach_secrets_patch(&mut out, secrets_set);
    json_bytes(&out)
}

fn create_slack_app(
    input: &SetupWebhookInput,
    config_token_input: String,
    refresh_token: Option<&str>,
) -> Vec<u8> {
    let manifest = build_slack_manifest(input);
    let mut secrets_set = BTreeMap::new();
    let mut config_token = config_token_input;
    let create_body = match create_manifest(&config_token, &manifest) {
        ManifestApiResult::Ok(body) => body,
        ManifestApiResult::AuthError => match try_refresh_token(refresh_token) {
            Ok(refresh) => {
                config_token = refresh.access_token;
                secrets_set.extend(refresh.secrets_set);
                match create_manifest(&config_token, &manifest) {
                    ManifestApiResult::Ok(body) => body,
                    ManifestApiResult::AuthError => {
                        return json_bytes(&json!({
                            "ok": false,
                            "error": "manifest create auth error after token refresh"
                        }));
                    }
                    ManifestApiResult::Err(e) => return e,
                }
            }
            Err(e) => return e,
        },
        ManifestApiResult::Err(e) => return e,
    };

    let app_id = match string_at_paths(&create_body, &["app_id", "app.id"]) {
        Some(value) => value,
        _ => {
            return json_bytes(&json!({
                "ok": false,
                "error": "manifest create response missing app_id"
            }));
        }
    };

    secrets_set.insert(DEFAULT_APP_ID_KEY.to_string(), app_id.clone());
    collect_optional_secret(
        &mut secrets_set,
        DEFAULT_CLIENT_ID_KEY,
        string_at_paths(&create_body, &["client_id", "credentials.client_id"]),
    );
    collect_optional_secret(
        &mut secrets_set,
        DEFAULT_CLIENT_SECRET_KEY,
        string_at_paths(
            &create_body,
            &["client_secret", "credentials.client_secret"],
        ),
    );
    collect_optional_secret(
        &mut secrets_set,
        DEFAULT_SIGNING_SECRET_KEY,
        string_at_paths(
            &create_body,
            &["signing_secret", "credentials.signing_secret"],
        ),
    );
    let instance_key = derive_instance_key(
        input
            .bundle_id
            .as_deref()
            .unwrap_or(DEFAULT_INSTANCE_BUNDLE_ID),
        input.bundle_digest.as_deref(),
        &input.provider_id,
        &input.tenant,
        &input.team,
    );
    secrets_set.insert("SLACK_INSTANCE_KEY".to_string(), instance_key);

    let client_id = string_at_paths(&create_body, &["client_id", "credentials.client_id"]);
    let callback_path = "/oauth/callback/slack".to_string();
    let setup_actions = match client_id {
        Some(client_id) => vec![SetupAction {
            id: format!("slack-install-{}-{}", input.tenant, input.team),
            kind: "oauth_install_button".to_string(),
            label: "Add to Slack".to_string(),
            provider_id: input.provider_id.clone(),
            tenant: input.tenant.clone(),
            team: input.team.clone(),
            authorize_url: format!(
                "https://slack.com/oauth/v2/authorize?client_id={client_id}&scope=chat:write,app_mentions:read,channels:read,channels:history,im:history,im:read,im:write,users:read"
            ),
            callback_path,
            status: "pending".to_string(),
        }],
        None => Vec::new(),
    };

    let mut out = json!({
        "ok": true,
        "status": "install_required",
        "app_status": "created",
        "slack_app_id": app_id,
        "webhook_url": build_webhook_url(input),
        "setup_actions": setup_actions,
        "slack_response": create_body,
    });
    attach_secrets_patch(&mut out, secrets_set);
    json_bytes(&out)
}

enum ManifestApiResult {
    Ok(Value),
    AuthError,
    Err(Vec<u8>),
}

/// Result of an `apps.manifest.export` call.
enum ExportResult {
    /// Successful export with the parsed response body.
    Ok(Value),
    /// Auth error (token expired or invalid) — caller should attempt refresh.
    AuthError,
    /// Other error — already serialized as JSON bytes for immediate return.
    Err(Vec<u8>),
}

/// Call `apps.manifest.export` and classify the result.
fn export_manifest(app_id: &str, config_token: &str) -> ExportResult {
    let resp = client::send(
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
    let body: Value = match resp {
        Ok(r) => serde_json::from_slice(&r.body.unwrap_or_default()).unwrap_or(Value::Null),
        Err(err) => {
            return ExportResult::Err(json_bytes(
                &json!({"ok": false, "error": format!("manifest export failed: {}", err.message)}),
            ));
        }
    };
    if body.get("ok").and_then(Value::as_bool) == Some(true) {
        return ExportResult::Ok(body);
    }
    let err = body
        .get("error")
        .and_then(Value::as_str)
        .unwrap_or("unknown");
    if err == "invalid_auth" || err == "token_expired" || err == "token_revoked" {
        return ExportResult::AuthError;
    }
    ExportResult::Err(json_bytes(
        &json!({"ok": false, "error": format!("manifest export error: {err}")}),
    ))
}

fn update_manifest(app_id: &str, config_token: &str, manifest: &Value) -> ManifestApiResult {
    let resp = client::send(
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
    parse_manifest_api_response(resp, "manifest update")
}

fn create_manifest(config_token: &str, manifest: &Value) -> ManifestApiResult {
    let resp = client::send(
        &client::Request {
            method: "POST".to_string(),
            url: "https://slack.com/api/apps.manifest.create".to_string(),
            headers: vec![
                (
                    "Authorization".to_string(),
                    format!("Bearer {config_token}"),
                ),
                ("Content-Type".to_string(), "application/json".to_string()),
            ],
            body: Some(serde_json::to_vec(&json!({"manifest": manifest})).unwrap_or_default()),
        },
        None,
        None,
    );
    parse_manifest_api_response(resp, "manifest create")
}

fn parse_manifest_api_response(
    resp: Result<client::Response, client::HostError>,
    action: &str,
) -> ManifestApiResult {
    let body: Value = match resp {
        Ok(r) => serde_json::from_slice(&r.body.unwrap_or_default()).unwrap_or(Value::Null),
        Err(err) => {
            return ManifestApiResult::Err(json_bytes(
                &json!({"ok": false, "error": format!("{action} failed: {}", err.message)}),
            ));
        }
    };
    if body.get("ok").and_then(Value::as_bool) == Some(true) {
        return ManifestApiResult::Ok(body);
    }
    let err = body
        .get("error")
        .and_then(Value::as_str)
        .unwrap_or("unknown");
    if err == "invalid_auth" || err == "token_expired" || err == "token_revoked" {
        return ManifestApiResult::AuthError;
    }
    ManifestApiResult::Err(json_bytes(&json!({
        "ok": false,
        "error": format!("{action} error: {err}"),
        "slack_response": body,
    })))
}

#[derive(Debug)]
struct RefreshResult {
    access_token: String,
    secrets_set: BTreeMap<String, String>,
}

/// Attempt to refresh the configuration token.
///
/// Returns the new config token on success, or a serialized error response.
fn try_refresh_token(refresh_token: Option<&str>) -> Result<RefreshResult, Vec<u8>> {
    let refresh_token = refresh_token.ok_or_else(|| {
        json_bytes(&json!({
            "ok": false,
            "error": "configuration token expired and no refresh token available; \
                      generate a new token pair at api.slack.com/apps"
        }))
    })?;
    match rotate_config_token(refresh_token) {
        Ok((new_token, new_refresh)) => {
            let mut secrets_set = BTreeMap::new();
            secrets_set.insert(
                DEFAULT_CONFIG_ACCESS_TOKEN_KEY.to_string(),
                new_token.clone(),
            );
            secrets_set.insert(DEFAULT_CONFIG_REFRESH_TOKEN_KEY.to_string(), new_refresh);
            Ok(RefreshResult {
                access_token: new_token,
                secrets_set,
            })
        }
        Err(err) => Err(json_bytes(&json!({
            "ok": false,
            "error": format!(
                "configuration token expired and refresh failed: {err}; \
                 generate a new token pair at api.slack.com/apps"
            )
        }))),
    }
}

fn config_access_token_from_input(parsed: &Value) -> Option<String> {
    parsed
        .get("slack_configuration_access_token")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(String::from)
}

/// Update Slack manifest JSON with webhook URLs for event subscriptions
/// and interactivity.
fn update_manifest_urls(manifest: &mut Value, webhook_url: &str) {
    let manifest_obj = manifest.as_object_mut();
    let Some(manifest_obj) = manifest_obj else {
        return;
    };

    let features = manifest_obj
        .entry("features")
        .or_insert_with(|| json!({}))
        .as_object_mut();
    if let Some(features) = features {
        let app_home = features.entry("app_home").or_insert_with(|| json!({}));
        if let Some(obj) = app_home.as_object_mut() {
            obj.insert("messages_tab_enabled".to_string(), json!(true));
            obj.insert("messages_tab_read_only_enabled".to_string(), json!(false));
        }
    }

    let oauth_config = manifest_obj
        .entry("oauth_config")
        .or_insert_with(|| json!({}))
        .as_object_mut();
    if let Some(oauth_config) = oauth_config {
        let scopes = oauth_config.entry("scopes").or_insert_with(|| json!({}));
        if let Some(scopes_obj) = scopes.as_object_mut() {
            let bot_scopes = scopes_obj.entry("bot").or_insert_with(|| json!([]));
            push_unique_string(bot_scopes, "im:history");
        }
    }

    let settings = manifest_obj
        .entry("settings")
        .or_insert_with(|| json!({}))
        .as_object_mut()
        .map(|s| s as &mut serde_json::Map<String, Value>);
    let Some(settings) = settings else { return };

    // event_subscriptions.request_url
    let event_subs = settings
        .entry("event_subscriptions")
        .or_insert_with(|| json!({}));
    if let Some(obj) = event_subs.as_object_mut() {
        obj.insert("request_url".to_string(), json!(webhook_url));
        let bot_events = obj.entry("bot_events").or_insert_with(|| json!([]));
        push_unique_string(bot_events, "message.im");
    }

    // interactivity.request_url + is_enabled
    let interactivity = settings.entry("interactivity").or_insert_with(|| json!({}));
    if let Some(obj) = interactivity.as_object_mut() {
        obj.insert("request_url".to_string(), json!(webhook_url));
        obj.insert("is_enabled".to_string(), json!(true));
    }
}

fn build_slack_manifest(input: &SetupWebhookInput) -> Value {
    let webhook_url = build_webhook_url(input);
    let instance_key = derive_instance_key(
        input
            .bundle_id
            .as_deref()
            .unwrap_or(DEFAULT_INSTANCE_BUNDLE_ID),
        input.bundle_digest.as_deref(),
        &input.provider_id,
        &input.tenant,
        &input.team,
    );

    json!({
        "_metadata": {
            "major_version": 1,
            "minor_version": 0,
        },
        "display_information": {
            "name": format!("Greentic {}", input.provider_id),
            "description": format!("Greentic managed Slack app: {instance_key}"),
        },
        "features": {
            "app_home": {
                "home_tab_enabled": true,
                "messages_tab_enabled": true,
                "messages_tab_read_only_enabled": false,
            },
            "bot_user": {
                "display_name": "Greentic Bot",
                "always_online": true,
            },
        },
        "oauth_config": {
            "scopes": {
                "bot": [
                    "chat:write",
                    "app_mentions:read",
                    "channels:read",
                    "channels:history",
                    "im:history",
                    "im:read",
                    "im:write",
                    "users:read",
                ],
            },
            "redirect_urls": [
                format!("{}/oauth/callback/slack", input.public_base_url),
            ],
        },
        "settings": {
            "event_subscriptions": {
                "request_url": webhook_url,
                "bot_events": [
                    "message.im",
                    "app_mention",
                ],
            },
            "interactivity": {
                "is_enabled": true,
                "request_url": webhook_url,
            },
            "org_deploy_enabled": false,
        },
    })
}

fn build_webhook_url(input: &SetupWebhookInput) -> String {
    format!(
        "{}/v1/messaging/ingress/{}/{}/{}",
        input.public_base_url.trim_end_matches('/'),
        input.provider_id,
        input.tenant,
        input.team,
    )
}

fn update_manifest_metadata(manifest: &mut Value) {
    let Some(manifest_obj) = manifest.as_object_mut() else {
        return;
    };
    let metadata = manifest_obj.entry("_metadata").or_insert_with(|| json!({}));
    if let Some(metadata_obj) = metadata.as_object_mut() {
        metadata_obj.insert("major_version".to_string(), json!(1));
        metadata_obj.insert("minor_version".to_string(), json!(0));
    }
}

fn derive_instance_key(
    bundle_id: &str,
    bundle_digest: Option<&str>,
    provider_id: &str,
    tenant: &str,
    team: &str,
) -> String {
    let mut input = String::new();
    input.push_str(bundle_id);
    if let Some(digest) = bundle_digest {
        input.push_str(digest);
    }
    input.push_str(provider_id);
    input.push_str(tenant);
    input.push_str(team);

    let digest = Sha256::digest(input.as_bytes());
    let mut hex = String::with_capacity(digest.len() * 2);
    for byte in digest {
        hex.push_str(&format!("{byte:02x}"));
    }
    format!("gt-slack-{}", &hex[..16])
}

fn string_at_paths(value: &Value, paths: &[&str]) -> Option<String> {
    paths.iter().find_map(|path| {
        let mut cursor = value;
        for segment in path.split('.') {
            cursor = cursor.get(segment)?;
        }
        cursor
            .as_str()
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(ToOwned::to_owned)
    })
}

fn collect_optional_secret(
    secrets_set: &mut BTreeMap<String, String>,
    key: &str,
    value: Option<String>,
) {
    if let Some(value) = value.filter(|value| !value.trim().is_empty()) {
        secrets_set.insert(key.to_string(), value);
    }
}

fn attach_secrets_patch(out: &mut Value, secrets_set: BTreeMap<String, String>) {
    if !secrets_set.is_empty() {
        out["secrets_patch"] = json!({
            "set": secrets_set,
            "delete": [],
        });
    }
}

fn push_unique_string(value: &mut Value, item: &str) {
    let Value::Array(items) = value else {
        *value = json!([item]);
        return;
    };
    if !items.iter().any(|value| value.as_str() == Some(item)) {
        items.push(Value::String(item.to_string()));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn urlencoding_encodes_special_chars() {
        assert_eq!(urlencoding("hello world"), "hello%20world");
        assert_eq!(urlencoding("xoxe.xoxp-123"), "xoxe.xoxp-123");
        assert_eq!(urlencoding("a=b&c=d"), "a%3Db%26c%3Dd");
        assert_eq!(urlencoding("token+value"), "token%2Bvalue");
    }

    #[test]
    fn setup_webhook_validates_input_before_network() {
        let invalid: Value = serde_json::from_slice(&setup_webhook(b"{")).expect("json");
        assert_eq!(invalid["ok"], false);
        assert!(
            invalid["error"]
                .as_str()
                .unwrap_or_default()
                .contains("invalid json")
        );
    }

    #[test]
    fn update_manifest_urls_creates_event_and_interactivity_settings() {
        let mut manifest = json!({});

        update_manifest_urls(&mut manifest, "https://chat.example.com/hook");

        assert_eq!(
            manifest["features"]["app_home"]["messages_tab_enabled"],
            true
        );
        assert_eq!(
            manifest["features"]["app_home"]["messages_tab_read_only_enabled"],
            false
        );
        assert_eq!(
            manifest["oauth_config"]["scopes"]["bot"]
                .as_array()
                .expect("bot scopes")
                .iter()
                .any(|scope| scope.as_str() == Some("im:history")),
            true
        );
        assert_eq!(
            manifest["settings"]["event_subscriptions"]["request_url"],
            "https://chat.example.com/hook"
        );
        assert_eq!(
            manifest["settings"]["event_subscriptions"]["bot_events"]
                .as_array()
                .expect("bot events")
                .iter()
                .any(|event| event.as_str() == Some("message.im")),
            true
        );
        assert_eq!(
            manifest["settings"]["interactivity"]["request_url"],
            "https://chat.example.com/hook"
        );
        assert_eq!(manifest["settings"]["interactivity"]["is_enabled"], true);
    }

    #[test]
    fn build_slack_manifest_includes_webhook_oauth_and_metadata() {
        let input = SetupWebhookInput {
            public_base_url: "https://chat.example.com".to_string(),
            provider_id: "messaging-slack".to_string(),
            tenant: "tenant-a".to_string(),
            team: "team-a".to_string(),
            bundle_id: Some("bundle-a".to_string()),
            bundle_digest: Some("sha256:abc".to_string()),
        };

        let manifest = build_slack_manifest(&input);

        assert_eq!(
            manifest["settings"]["event_subscriptions"]["request_url"],
            "https://chat.example.com/v1/messaging/ingress/messaging-slack/tenant-a/team-a"
        );
        assert_eq!(
            manifest["oauth_config"]["redirect_urls"][0],
            "https://chat.example.com/oauth/callback/slack"
        );
        assert_eq!(manifest["_metadata"]["major_version"], 1);
        assert_eq!(manifest["_metadata"]["minor_version"], 0);
        assert_eq!(manifest["_metadata"].as_object().unwrap().len(), 2);
        assert!(
            manifest["oauth_config"]["scopes"]["bot"]
                .as_array()
                .expect("bot scopes")
                .iter()
                .any(|scope| scope.as_str() == Some("app_mentions:read"))
        );
    }

    #[test]
    fn derive_instance_key_is_stable_and_scoped() {
        let key_a = derive_instance_key(
            "bundle-a",
            Some("sha256:abc"),
            "messaging-slack",
            "tenant-a",
            "team-a",
        );
        let key_b = derive_instance_key(
            "bundle-a",
            Some("sha256:abc"),
            "messaging-slack",
            "tenant-a",
            "team-a",
        );
        let key_c = derive_instance_key(
            "bundle-a",
            Some("sha256:def"),
            "messaging-slack",
            "tenant-a",
            "team-a",
        );

        assert_eq!(key_a, key_b);
        assert_ne!(key_a, key_c);
        assert!(key_a.starts_with("gt-slack-"));
    }

    #[test]
    fn string_at_paths_reads_top_level_and_nested_values() {
        let body = json!({
            "app": { "id": "A123" },
            "credentials": { "client_id": "123.456" }
        });

        assert_eq!(
            string_at_paths(&body, &["app_id", "app.id"]).as_deref(),
            Some("A123")
        );
        assert_eq!(
            string_at_paths(&body, &["client_id", "credentials.client_id"]).as_deref(),
            Some("123.456")
        );
        assert_eq!(string_at_paths(&body, &["missing"]), None);
    }

    #[test]
    fn try_refresh_token_reports_missing_refresh_without_network() {
        let err = try_refresh_token(None).expect_err("missing refresh token");
        let body: Value = serde_json::from_slice(&err).expect("json");

        assert_eq!(body["ok"], false);
        assert!(
            body["error"]
                .as_str()
                .unwrap_or_default()
                .contains("no refresh token available")
        );
    }

    #[test]
    fn setup_webhook_accepts_access_token_field_name() {
        let invalid_url: Value = serde_json::from_slice(&setup_webhook(
            br#"{
                "slack_app_id": "A123",
                "slack_configuration_access_token": "xoxe-access",
                "slack_configuration_refresh_token": "xoxe-refresh",
                "public_base_url": "http://example.com"
            }"#,
        ))
        .expect("json");

        assert_eq!(invalid_url["ok"], false);
        assert!(
            invalid_url["error"]
                .as_str()
                .unwrap_or_default()
                .contains("public_base_url")
        );
    }

    #[test]
    fn config_access_token_parser_ignores_legacy_configuration_token_field_name() {
        let parsed = json!({
            "slack_configuration_token": "xoxe-legacy",
            "slack_configuration_refresh_token": "xoxe-refresh"
        });

        assert_eq!(config_access_token_from_input(&parsed), None);
    }
}
