//! Slack app manifest webhook wiring (`setup_webhook`).
//!
//! Creates or updates Slack app manifests for setup. The registration op reuses
//! a known app with the requested manifest name when available, otherwise it
//! creates a new Slack app from a manifest. The webhook op updates an existing
//! app's callback URLs.

use provider_common::helpers::json_bytes;
use serde_json::{Value, json};

use crate::bindings::greentic::http::http_client as client;
use crate::config::put_secret_string;
use crate::{
    DEFAULT_APP_ID_KEY, DEFAULT_CONFIG_ACCESS_TOKEN_KEY, DEFAULT_CONFIG_REFRESH_TOKEN_KEY,
    DEFAULT_SIGNING_SECRET_KEY,
};

/// Component-side operational log. The runner host forwards WASM stderr to
/// the operator console (see greentic-runner-host's telemetry stream), so
/// these lines surface next to the host's own `[setup-action ...]` logs.
/// Never log token/secret VALUES here — presence/absence only.
fn wlog(msg: &str) {
    eprintln!("[messaging-slack] {msg}");
}

/// Rotate an expired Slack configuration token using the refresh token.
///
/// Calls `tooling.tokens.rotate` with form-urlencoded body. Returns
/// `(new_config_token, new_refresh_token)` on success.
fn rotate_config_token(refresh_token: &str) -> Result<(String, String), String> {
    wlog("calling Slack tooling.tokens.rotate (configuration token expired)");
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
        Err(e) => {
            wlog(&format!(
                "tooling.tokens.rotate transport error: {}",
                e.message
            ));
            return Err(format!("token rotate request failed: {}", e.message));
        }
    };
    if resp_body.get("ok").and_then(Value::as_bool) != Some(true) {
        let err = resp_body
            .get("error")
            .and_then(Value::as_str)
            .unwrap_or("unknown");
        wlog(&format!("tooling.tokens.rotate rejected: {err}"));
        return Err(format!("token rotate failed: {err}"));
    }
    wlog("tooling.tokens.rotate succeeded; persisting rotated token pair");
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
        .or_else(|| parsed.get("app_id"))
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(String::from)
        .or_else(|| secret_string(DEFAULT_APP_ID_KEY));

    // Resolve config access token from current and legacy setup field names.
    let config_token_input = config_access_token_from_input(&parsed)
        .or_else(|| secret_string(DEFAULT_CONFIG_ACCESS_TOKEN_KEY));

    // Resolve refresh token: input field → secrets store fallback.
    let refresh_token = parsed
        .get("slack_configuration_refresh_token")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(String::from)
        .or_else(|| secret_string(DEFAULT_CONFIG_REFRESH_TOKEN_KEY));

    let (app_id, config_token_input) = match (app_id.as_deref(), config_token_input) {
        (Some(a), Some(t)) => (a, t),
        _ => {
            return json_bytes(&json!({
                "ok": false,
                "error": "slack_app_id and slack_configuration_access_token required"
            }));
        }
    };

    // Revision-serve passes a fully-formed `webhook_url`; legacy fills it from
    // `public_base_url` + the ingress path segments.
    let webhook_url = match parsed
        .get("webhook_url")
        .or_else(|| parsed.get("config").and_then(|c| c.get("webhook_url")))
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|s| !s.is_empty())
    {
        Some(url) => {
            if !url.starts_with("https://") {
                return json_bytes(
                    &json!({"ok": false, "error": "webhook_url must be an https:// URL"}),
                );
            }
            url.to_string()
        }
        None => {
            let public_base_url = parsed
                .get("public_base_url")
                .and_then(Value::as_str)
                .unwrap_or("");
            if public_base_url.is_empty() || !public_base_url.starts_with("https://") {
                return json_bytes(&json!({
                    "ok": false,
                    "error": "public_base_url or webhook_url required (https://)"
                }));
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
            format!(
                "{}/v1/messaging/ingress/{}/{}/{}",
                base_origin(public_base_url),
                provider_id,
                tenant,
                team,
            )
        }
    };

    wlog(&format!(
        "setup_webhook: re-pointing app {app_id} Event Subscriptions/Interactivity to {webhook_url}"
    ));

    // Step 1: Export current manifest (with token refresh on auth failure)
    let mut config_token = config_token_input;
    let export_body = match export_manifest(app_id, &config_token) {
        ExportResult::Ok(body) => body,
        ExportResult::AuthError => {
            // Attempt token refresh
            match try_refresh_token(&config_token, refresh_token.as_deref()) {
                Ok(new_token) => {
                    config_token = new_token;
                    match export_manifest(app_id, &config_token) {
                        ExportResult::Ok(body) => body,
                        ExportResult::AuthError => {
                            wlog(
                                "setup_webhook: manifest export still unauthorized after token refresh",
                            );
                            return json_bytes(&json!({
                                "ok": false,
                                "error": "manifest export auth error after token refresh"
                            }));
                        }
                        ExportResult::NotFound => {
                            return json_bytes(&json!({
                                "ok": false,
                                "error": "slack_app_id not found; rerun Slack setup to register an app"
                            }));
                        }
                        ExportResult::Err(e) => return e,
                    }
                }
                Err(e) => return e,
            }
        }
        ExportResult::NotFound => {
            wlog(&format!(
                "setup_webhook: app {app_id} not found/accessible via apps.manifest.export"
            ));
            return json_bytes(&json!({
                "ok": false,
                "error": "slack_app_id not found; rerun Slack setup to register an app"
            }));
        }
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
            wlog(&format!(
                "setup_webhook: apps.manifest.update transport error: {}",
                err.message
            ));
            return json_bytes(
                &json!({"ok": false, "error": format!("manifest update failed: {}", err.message)}),
            );
        }
    };
    let ok = update_body.get("ok").and_then(Value::as_bool) == Some(true);
    if ok {
        wlog(&format!(
            "setup_webhook: app {app_id} URLs updated to {webhook_url}"
        ));
    } else {
        wlog(&format!(
            "setup_webhook: apps.manifest.update rejected: {}",
            slack_error(&update_body).unwrap_or("unknown")
        ));
    }
    json_bytes(&json!({
        "ok": ok,
        "webhook_url": webhook_url,
        "slack_response": update_body,
    }))
}

/// Handle `setup_app_registration` op — creates a Slack app and returns
/// Slack-generated OAuth credentials for the subsequent setup authorization step.
pub(crate) fn setup_app_registration(input_json: &[u8]) -> Vec<u8> {
    let parsed: Value = match serde_json::from_slice(input_json) {
        Ok(val) => val,
        Err(err) => {
            return json_bytes(&json!({"ok": false, "error": format!("invalid json: {err}")}));
        }
    };
    let config_token_input = config_access_token_from_input(&parsed)
        .or_else(|| secret_string(DEFAULT_CONFIG_ACCESS_TOKEN_KEY));
    let refresh_token = parsed
        .get("slack_configuration_refresh_token")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(String::from)
        .or_else(|| secret_string(DEFAULT_CONFIG_REFRESH_TOKEN_KEY));
    let Some(mut config_token) = config_token_input else {
        return json_bytes(&json!({
            "ok": false,
            "error": "slack_configuration_access_token is required to create the Slack app"
        }));
    };
    put_secret_string(DEFAULT_CONFIG_ACCESS_TOKEN_KEY, &config_token);
    if let Some(refresh_token) = refresh_token.as_deref() {
        put_secret_string(DEFAULT_CONFIG_REFRESH_TOKEN_KEY, refresh_token);
    }
    let public_base_url = parsed
        .get("public_base_url")
        .and_then(Value::as_str)
        .unwrap_or("");
    if public_base_url.is_empty() || !public_base_url.starts_with("https://") {
        wlog("setup_app_registration: rejected — public_base_url missing or not https://");
        return json_bytes(
            &json!({"ok": false, "error": "public_base_url must be an https:// URL"}),
        );
    }

    wlog(&format!(
        "setup_app_registration: starting (public_base_url={public_base_url}, refresh_token={})",
        if refresh_token.is_some() {
            "present"
        } else {
            "absent"
        }
    ));

    let desired_manifest = registration_manifest(&parsed, public_base_url);
    let registration = match register_or_reuse_app(
        &parsed,
        &desired_manifest,
        &mut config_token,
        refresh_token.as_deref(),
    ) {
        Ok(registration) => registration,
        Err(err) => return err,
    };
    let body = registration.response;
    if body.get("ok").and_then(Value::as_bool) != Some(true) {
        let err = slack_error(&body).unwrap_or("unknown");
        wlog(&format!(
            "setup_app_registration: manifest create/update rejected by Slack: {err}"
        ));
        return json_bytes(
            &json!({"ok": false, "error": format!("manifest create error: {err}"), "slack_response": body}),
        );
    }

    let app_id = registration.app_id.or_else(|| {
        first_string(
            &body,
            &[
                &["app_id"],
                &["app", "id"],
                &["app", "app_id"],
                &["manifest", "app_id"],
            ],
        )
    });
    let signing_secret = first_string(
        &body,
        &[
            &["credentials", "signing_secret"],
            &["app", "credentials", "signing_secret"],
            &["app", "signing_secret"],
            &["signing_secret"],
        ],
    )
    .or_else(|| first_string(&parsed, &[&["signing_secret"], &["slack_signing_secret"]]))
    .or_else(|| secret_string(DEFAULT_SIGNING_SECRET_KEY));
    // The signing secret is only returned by `apps.manifest.create`; the reuse
    // (`apps.manifest.update`) path never returns credentials, and Slack offers
    // no API to re-fetch it afterward. So treat it as optional: complete the
    // registration and flag it as missing rather than hard-failing. Inbound
    // signature verification stays unavailable until the operator supplies the
    // secret (from the app's Basic Information page) — a clearer, deferred state
    // than blocking setup on an app that was otherwise created/reused fine.
    let signing_secret_missing = signing_secret.is_none();
    if let Some(app_id) = app_id.as_deref() {
        put_secret_string(DEFAULT_APP_ID_KEY, app_id);
    }
    if let Some(signing_secret) = signing_secret.as_deref() {
        put_secret_string(DEFAULT_SIGNING_SECRET_KEY, signing_secret);
    }
    wlog(&format!(
        "setup_app_registration: done — app {} {} (signing_secret {})",
        app_id.as_deref().unwrap_or("<unknown>"),
        if registration.reused {
            "reused"
        } else {
            "created"
        },
        if signing_secret_missing {
            "MISSING — operator must supply it from Basic Information"
        } else {
            "captured"
        }
    ));
    json_bytes(&json!({
        "ok": true,
        "app_id": app_id,
        "slack_app_id": app_id,
        "slack_signing_secret": signing_secret,
        "signing_secret_missing": signing_secret_missing,
        "warning": if signing_secret_missing {
            Value::String(
                "Slack did not return a signing secret (existing app reused). Provide \
                 slack_signing_secret from the app's Basic Information page to enable \
                 inbound request signature verification."
                    .to_string(),
            )
        } else {
            Value::Null
        },
        "manifest": registration.manifest,
        "registration_action": if registration.reused { "reused" } else { "created" },
        "reused_existing_app": registration.reused,
        "slack_response": body,
    }))
}

struct RegistrationResult {
    response: Value,
    manifest: Value,
    app_id: Option<String>,
    reused: bool,
}

fn register_or_reuse_app(
    parsed: &Value,
    desired_manifest: &Value,
    config_token: &mut String,
    refresh_token: Option<&str>,
) -> Result<RegistrationResult, Vec<u8>> {
    if let Some(existing_app_id) = existing_app_id(parsed)
        && let Some(export_body) =
            export_manifest_with_refresh(&existing_app_id, config_token, refresh_token)?
        && let Some(mut existing_manifest) = export_body.get("manifest").cloned()
        && same_manifest_name(&existing_manifest, desired_manifest)
    {
        wlog(&format!(
            "register_or_reuse_app: reusing existing app {existing_app_id} via apps.manifest.update"
        ));
        merge_registration_manifest(&mut existing_manifest, desired_manifest);
        let update_body = update_manifest(config_token, &existing_app_id, &existing_manifest);
        let update_body = if slack_error(&update_body).is_some_and(is_auth_error) {
            *config_token = try_refresh_token(config_token, refresh_token)?;
            update_manifest(config_token, &existing_app_id, &existing_manifest)
        } else {
            update_body
        };
        if update_body.get("ok").and_then(Value::as_bool) != Some(true) {
            let err = slack_error(&update_body).unwrap_or("unknown");
            wlog(&format!(
                "register_or_reuse_app: apps.manifest.update for {existing_app_id} rejected: {err}"
            ));
            return Err(json_bytes(&json!({
                "ok": false,
                "error": format!("manifest update error: {err}"),
                "slack_response": update_body,
            })));
        }
        return Ok(RegistrationResult {
            response: update_body,
            manifest: existing_manifest,
            app_id: Some(existing_app_id),
            reused: true,
        });
    }

    wlog(
        "register_or_reuse_app: no reusable app found; creating a new one via apps.manifest.create",
    );
    let body = create_manifest_with_refresh(config_token, refresh_token, desired_manifest)?;
    Ok(RegistrationResult {
        response: body,
        manifest: desired_manifest.clone(),
        app_id: None,
        reused: false,
    })
}

fn create_manifest_with_refresh(
    config_token: &mut String,
    refresh_token: Option<&str>,
    manifest: &Value,
) -> Result<Value, Vec<u8>> {
    let mut body = create_manifest(config_token, manifest);
    if slack_error(&body).is_some_and(is_auth_error) {
        *config_token = try_refresh_token(config_token, refresh_token)?;
        body = create_manifest(config_token, manifest);
    }
    Ok(body)
}

fn export_manifest_with_refresh(
    app_id: &str,
    config_token: &mut String,
    refresh_token: Option<&str>,
) -> Result<Option<Value>, Vec<u8>> {
    match export_manifest(app_id, config_token) {
        ExportResult::Ok(body) => Ok(Some(body)),
        ExportResult::AuthError => {
            *config_token = try_refresh_token(config_token, refresh_token)?;
            match export_manifest(app_id, config_token) {
                ExportResult::Ok(body) => Ok(Some(body)),
                ExportResult::AuthError => Err(json_bytes(&json!({
                    "ok": false,
                    "error": "manifest export auth error after token refresh"
                }))),
                ExportResult::Err(err) => Err(err),
                ExportResult::NotFound => Ok(None),
            }
        }
        ExportResult::Err(err) => Err(err),
        ExportResult::NotFound => Ok(None),
    }
}

/// Result of an `apps.manifest.export` call.
enum ExportResult {
    /// Successful export with the parsed response body.
    Ok(Value),
    /// Auth error (token expired or invalid) — caller should attempt refresh.
    AuthError,
    /// Existing app id is invalid or no longer accessible; callers may create.
    NotFound,
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
            wlog(&format!(
                "apps.manifest.export for {app_id} transport error: {}",
                err.message
            ));
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
    wlog(&format!(
        "apps.manifest.export for {app_id} rejected: {err}"
    ));
    if err == "invalid_auth" || err == "token_expired" || err == "token_revoked" {
        return ExportResult::AuthError;
    }
    if matches!(err, "invalid_app_id" | "app_not_found") {
        return ExportResult::NotFound;
    }
    ExportResult::Err(json_bytes(
        &json!({"ok": false, "error": format!("manifest export error: {err}")}),
    ))
}

/// Attempt to refresh the configuration token and persist the new tokens.
///
/// Returns the new config token on success, or a serialized error response.
fn try_refresh_token(_config_token: &str, refresh_token: Option<&str>) -> Result<String, Vec<u8>> {
    let refresh_token = refresh_token.ok_or_else(|| {
        json_bytes(&json!({
            "ok": false,
            "error": "configuration token expired and no refresh token available; \
                      generate a new token pair at api.slack.com/apps"
        }))
    })?;
    match rotate_config_token(refresh_token) {
        Ok((new_token, new_refresh)) => {
            put_secret_string(DEFAULT_CONFIG_ACCESS_TOKEN_KEY, &new_token);
            put_secret_string(DEFAULT_CONFIG_REFRESH_TOKEN_KEY, &new_refresh);
            Ok(new_token)
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
    for key in [
        "slack_configuration_access_token",
        "slack_configuration_token",
    ] {
        if let Some(value) = parsed
            .get(key)
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(String::from)
        {
            return Some(value);
        }
    }
    None
}

fn existing_app_id(parsed: &Value) -> Option<String> {
    first_string(parsed, &[&["slack_app_id"], &["app_id"]])
        .or_else(|| secret_string(DEFAULT_APP_ID_KEY))
}

/// Reduce a public base URL to its origin (`scheme://host[:port]`), dropping any
/// path/query. Prevents a base that carries a path — e.g. a webchat UI URL
/// (`https://x.ngrok-free.app/v1/web/webchat/demo`) mistakenly used as the base —
/// from producing a malformed, doubled ingress URL like
/// `…/v1/web/webchat/demo/v1/messaging/ingress/messaging-slack/…`, which makes
/// Slack's Request-URL verification fail with `challenge_failed`.
fn base_origin(public_base_url: &str) -> String {
    let trimmed = public_base_url.trim().trim_end_matches('/');
    if let Some(scheme_end) = trimmed.find("://") {
        let host_start = scheme_end + 3;
        if let Some(slash) = trimmed[host_start..].find('/') {
            return trimmed[..host_start + slash].to_string();
        }
    }
    trimmed.to_string()
}

fn registration_manifest(parsed: &Value, public_base_url: &str) -> Value {
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
    let name = parsed
        .get("slack_app_name")
        .or_else(|| parsed.get("app_name"))
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .unwrap_or("Greentic Slack");
    let base = base_origin(public_base_url);
    let base = base.as_str();
    let ingress_url = format!("{base}/v1/messaging/ingress/{provider_id}/{tenant}/{team}");
    // The OAuth *callback* (developer app-install) is served by the greentic-setup
    // server, which is a different host than the messaging `public_base_url` (the
    // runtime, which serves webhook ingress). Register the setup callback when
    // provided; otherwise fall back to the runtime base for back-compat.
    let callback_base = parsed
        .get("oauth_callback_base_url")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| value.starts_with("https://"))
        .map(|value| value.trim_end_matches('/'))
        .unwrap_or(base);
    let callback_url = format!("{callback_base}/oauth/callback/slack");

    json!({
        "display_information": {
            "name": name,
        },
        "features": {
            "bot_user": {
                "display_name": name,
                "always_online": false,
            },
            "app_home": {
                "messages_tab_enabled": true,
                "messages_tab_read_only_enabled": false,
            },
        },
        "oauth_config": {
            "redirect_urls": [callback_url],
            "scopes": {
                "bot": ["chat:write", "channels:read", "channels:history", "channels:join", "im:history", "im:write", "app_mentions:read"],
            },
        },
        "settings": {
            "event_subscriptions": {
                "request_url": ingress_url,
                "bot_events": ["message.im", "app_mention", "message.channels"],
            },
            "interactivity": {
                "is_enabled": true,
                "request_url": ingress_url,
            },
            // Enterprise Grid workspaces cannot install non-org-deployable apps.
            "org_deploy_enabled": true,
            "socket_mode_enabled": false,
            "token_rotation_enabled": false,
        },
    })
}

fn create_manifest(config_token: &str, manifest: &Value) -> Value {
    wlog("calling Slack apps.manifest.create");
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
            body: Some(
                serde_json::to_vec(&json!({"manifest": manifest.to_string()})).unwrap_or_default(),
            ),
        },
        None,
        None,
    );
    match resp {
        Ok(resp) => serde_json::from_slice(&resp.body.unwrap_or_default()).unwrap_or(Value::Null),
        Err(err) => {
            wlog(&format!(
                "apps.manifest.create transport error: {}",
                err.message
            ));
            json!({"ok": false, "error": format!("manifest create failed: {}", err.message)})
        }
    }
}

fn update_manifest(config_token: &str, app_id: &str, manifest: &Value) -> Value {
    wlog(&format!("calling Slack apps.manifest.update for {app_id}"));
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
                serde_json::to_vec(&json!({"app_id": app_id, "manifest": manifest.to_string()}))
                    .unwrap_or_default(),
            ),
        },
        None,
        None,
    );
    match resp {
        Ok(resp) => serde_json::from_slice(&resp.body.unwrap_or_default()).unwrap_or(Value::Null),
        Err(err) => {
            wlog(&format!(
                "apps.manifest.update for {app_id} transport error: {}",
                err.message
            ));
            json!({"ok": false, "error": format!("manifest update failed: {}", err.message)})
        }
    }
}

fn first_string(value: &Value, paths: &[&[&str]]) -> Option<String> {
    paths.iter().find_map(|path| {
        let mut current = value;
        for key in *path {
            current = current.get(*key)?;
        }
        current
            .as_str()
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(ToString::to_string)
    })
}

fn slack_error(value: &Value) -> Option<&str> {
    value.get("error").and_then(Value::as_str)
}

fn is_auth_error(err: &str) -> bool {
    matches!(err, "invalid_auth" | "token_expired" | "token_revoked")
}

fn manifest_name(manifest: &Value) -> Option<&str> {
    manifest
        .get("display_information")
        .and_then(|value| value.get("name"))
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
}

fn same_manifest_name(existing: &Value, desired: &Value) -> bool {
    match (manifest_name(existing), manifest_name(desired)) {
        (Some(existing), Some(desired)) => existing == desired,
        _ => false,
    }
}

#[cfg(not(test))]
fn secret_string(key: &str) -> Option<String> {
    crate::config::get_secret_string(key).ok()
}

#[cfg(test)]
fn secret_string(_key: &str) -> Option<String> {
    None
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
        push_unique_string(bot_events, "app_mention");
        push_unique_string(bot_events, "message.channels");
    }

    // interactivity.request_url + is_enabled
    let interactivity = settings.entry("interactivity").or_insert_with(|| json!({}));
    if let Some(obj) = interactivity.as_object_mut() {
        obj.insert("request_url".to_string(), json!(webhook_url));
        obj.insert("is_enabled".to_string(), json!(true));
    }
}

fn merge_registration_manifest(existing: &mut Value, desired: &Value) {
    let Some(existing_obj) = existing.as_object_mut() else {
        *existing = desired.clone();
        return;
    };
    let Some(desired_obj) = desired.as_object() else {
        return;
    };

    merge_object(existing_obj, desired_obj, "display_information");
    merge_object(existing_obj, desired_obj, "features");
    merge_object(existing_obj, desired_obj, "settings");
    merge_oauth_config(existing_obj, desired_obj);
}

fn merge_object(
    existing_obj: &mut serde_json::Map<String, Value>,
    desired_obj: &serde_json::Map<String, Value>,
    key: &str,
) {
    let Some(desired_value) = desired_obj.get(key) else {
        return;
    };
    let existing_value = existing_obj
        .entry(key.to_string())
        .or_insert_with(|| json!({}));
    merge_value(existing_value, desired_value);
}

fn merge_oauth_config(
    existing_obj: &mut serde_json::Map<String, Value>,
    desired_obj: &serde_json::Map<String, Value>,
) {
    let Some(desired_oauth) = desired_obj.get("oauth_config") else {
        return;
    };
    let existing_oauth = existing_obj
        .entry("oauth_config".to_string())
        .or_insert_with(|| json!({}));
    merge_value(existing_oauth, desired_oauth);
}

fn merge_value(existing: &mut Value, desired: &Value) {
    match (existing, desired) {
        (Value::Object(existing_obj), Value::Object(desired_obj)) => {
            for (key, desired_value) in desired_obj {
                let existing_value = existing_obj
                    .entry(key.clone())
                    .or_insert_with(|| empty_like(desired_value));
                merge_value(existing_value, desired_value);
            }
        }
        (Value::Array(existing_items), Value::Array(desired_items)) => {
            for desired_item in desired_items {
                if !existing_items.iter().any(|item| item == desired_item) {
                    existing_items.push(desired_item.clone());
                }
            }
        }
        (existing_value, desired_value) => {
            *existing_value = desired_value.clone();
        }
    }
}

fn empty_like(value: &Value) -> Value {
    match value {
        Value::Object(_) => json!({}),
        Value::Array(_) => json!([]),
        _ => Value::Null,
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
        assert!(
            manifest["oauth_config"]["scopes"]["bot"]
                .as_array()
                .expect("bot scopes")
                .iter()
                .any(|scope| scope.as_str() == Some("im:history"))
        );
        assert_eq!(
            manifest["settings"]["event_subscriptions"]["request_url"],
            "https://chat.example.com/hook"
        );
        assert!(
            manifest["settings"]["event_subscriptions"]["bot_events"]
                .as_array()
                .expect("bot events")
                .iter()
                .any(|event| event.as_str() == Some("message.im"))
        );
        assert_eq!(
            manifest["settings"]["interactivity"]["request_url"],
            "https://chat.example.com/hook"
        );
        assert_eq!(manifest["settings"]["interactivity"]["is_enabled"], true);
    }

    #[test]
    fn registration_manifest_builds_callback_and_ingress_urls() {
        let manifest = registration_manifest(
            &json!({
                "provider_id": "messaging-slack",
                "tenant": "demo",
                "team": "support",
                "slack_app_name": "Greentic Demo"
            }),
            "https://example.com/",
        );

        assert_eq!(manifest["display_information"]["name"], "Greentic Demo");
        assert_eq!(
            manifest["oauth_config"]["redirect_urls"][0],
            "https://example.com/oauth/callback/slack"
        );
        assert_eq!(
            manifest["settings"]["event_subscriptions"]["request_url"],
            "https://example.com/v1/messaging/ingress/messaging-slack/demo/support"
        );
        assert!(
            manifest["oauth_config"]["scopes"]["bot"]
                .as_array()
                .expect("bot scopes")
                .iter()
                .any(|scope| scope.as_str() == Some("chat:write"))
        );
    }

    #[test]
    fn manifest_name_matching_requires_same_display_name() {
        let existing = json!({"display_information": {"name": "Greentic Demo"}});
        let same = json!({"display_information": {"name": "Greentic Demo"}});
        let different = json!({"display_information": {"name": "Other App"}});

        assert!(same_manifest_name(&existing, &same));
        assert!(!same_manifest_name(&existing, &different));
    }

    #[test]
    fn merge_registration_manifest_adds_missing_oauth_scopes_without_dropping_existing() {
        let desired = registration_manifest(
            &json!({
                "provider_id": "messaging-slack",
                "tenant": "demo",
                "team": "support",
                "slack_app_name": "Greentic Demo"
            }),
            "https://example.com/",
        );
        let mut existing = json!({
            "display_information": {"name": "Greentic Demo"},
            "oauth_config": {
                "redirect_urls": ["https://old.example/oauth/callback/slack"],
                "scopes": {"bot": ["files:read", "chat:write"]}
            },
            "settings": {
                "event_subscriptions": {
                    "request_url": "https://old.example/events",
                    "bot_events": ["app_mention"]
                }
            }
        });

        merge_registration_manifest(&mut existing, &desired);

        let bot_scopes = existing["oauth_config"]["scopes"]["bot"]
            .as_array()
            .expect("bot scopes");
        for scope in [
            "files:read",
            "chat:write",
            "channels:read",
            "channels:history",
            "channels:join",
            "im:history",
            "im:write",
        ] {
            assert!(
                bot_scopes.iter().any(|value| value.as_str() == Some(scope)),
                "missing scope {scope}"
            );
        }
        let redirect_urls = existing["oauth_config"]["redirect_urls"]
            .as_array()
            .expect("redirect urls");
        assert!(
            redirect_urls
                .iter()
                .any(|value| value.as_str() == Some("https://old.example/oauth/callback/slack"))
        );
        assert!(
            redirect_urls
                .iter()
                .any(|value| value.as_str() == Some("https://example.com/oauth/callback/slack"))
        );
        assert_eq!(
            existing["settings"]["event_subscriptions"]["request_url"],
            "https://example.com/v1/messaging/ingress/messaging-slack/demo/support"
        );
        assert!(
            existing["settings"]["event_subscriptions"]["bot_events"]
                .as_array()
                .expect("bot events")
                .iter()
                .any(|value| value.as_str() == Some("message.im"))
        );
        assert!(
            existing["settings"]["event_subscriptions"]["bot_events"]
                .as_array()
                .expect("bot events")
                .iter()
                .any(|value| value.as_str() == Some("app_mention"))
        );
    }

    #[test]
    fn try_refresh_token_reports_missing_refresh_without_network() {
        let err = try_refresh_token("expired", None).expect_err("missing refresh token");
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
    fn setup_webhook_accepts_legacy_app_id_field_name() {
        let invalid_url: Value = serde_json::from_slice(&setup_webhook(
            br#"{
                "app_id": "A123",
                "slack_configuration_access_token": "xoxe-access",
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
    fn setup_webhook_fails_when_registration_values_are_missing() {
        let out: Value = serde_json::from_slice(&setup_webhook(
            br#"{
                "public_base_url": "https://example.com"
            }"#,
        ))
        .expect("json");

        assert_eq!(out["ok"], false);
        assert_ne!(out["skipped"], true);
        assert!(
            out["error"]
                .as_str()
                .unwrap_or_default()
                .contains("slack_app_id")
        );
    }

    #[test]
    fn config_access_token_parser_accepts_legacy_configuration_token_field_name() {
        let parsed = json!({
            "slack_configuration_token": "xoxe-legacy",
            "slack_configuration_refresh_token": "xoxe-refresh"
        });

        assert_eq!(
            config_access_token_from_input(&parsed).as_deref(),
            Some("xoxe-legacy")
        );
    }
}
