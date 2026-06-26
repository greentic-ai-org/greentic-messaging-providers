//! Bot-framework Teams setup wizard backend (the `setup_status` state machine the
//! `greentic-teams-setup-v4` web component drives). Dispatched here from
//! `handle_webhook` for `/v1/messaging/setup/messaging-teams/...` requests.
//!
//! Each step runs a real executor: device-code OAuth (`graph_admin_consent`),
//! operator-supplied or Graph-created bot identity (`bot_app_identity`), Bot
//! Framework endpoint registration consent
//! (`microsoft_bot_channel_registration_consent`), Bot Framework endpoint
//! registration, Teams app publish (in-component zip → Graph app catalog) and
//! per-user install, and a runtime observation of the first inbound activity
//! (`first_bot_framework_post`).

use crate::bindings::greentic::http::http_client as client;
use crate::bindings::greentic::state::state_store;
use serde_json::{Map, Value, json};
use urlencoding::encode;

/// Microsoft Graph Command Line Tools public client (device-code capable).
const GRAPH_CLIENT_ID_DEFAULT: &str = "14d82eec-204b-4c2f-b7e8-296a70dab67e";
const GRAPH_SCOPES: &str = "https://graph.microsoft.com/Application.ReadWrite.All https://graph.microsoft.com/AppCatalog.ReadWrite.All https://graph.microsoft.com/TeamsAppInstallation.ReadWriteForUser https://graph.microsoft.com/User.Read offline_access";
const AZURE_MANAGEMENT_SCOPES: &str =
    "https://management.azure.com/user_impersonation offline_access";

const STEP_IDS: [&str; 7] = [
    "graph_admin_consent",
    "bot_app_identity",
    "microsoft_bot_channel_registration_consent",
    "bot_framework_endpoint_registration",
    "teams_app_publish",
    "teams_app_user_install",
    "first_bot_framework_post",
];

fn step_label(id: &str) -> &'static str {
    match id {
        "graph_admin_consent" => "Authorize Microsoft Graph setup access",
        "bot_app_identity" => "Create or reuse the Bot Framework app identity",
        "microsoft_bot_channel_registration_consent" => {
            "Authorize Microsoft Teams bot channel registration"
        }
        "bot_framework_endpoint_registration" => "Register the Bot Framework endpoint",
        "teams_app_publish" => "Publish the Teams app",
        "teams_app_user_install" => "Install the Teams app for the current user",
        "first_bot_framework_post" => "Wait for the first Teams message",
        _ => "",
    }
}

/// `path` looks like `/v1/messaging/setup/messaging-teams/{tenant}[/{action...}]`.
/// Returns `(tenant, action)` where `action` is the remaining sub-path (may be empty).
fn parse_path(path: &str) -> Option<(String, String)> {
    let rest = path.split("/setup/messaging-teams/").nth(1)?;
    let rest = rest.trim_start_matches('/');
    let mut parts = rest.splitn(2, '/');
    let tenant = parts.next().unwrap_or("").to_string();
    if tenant.is_empty() {
        return None;
    }
    let action = parts.next().unwrap_or("").trim_end_matches('/').to_string();
    Some((tenant, action))
}

fn state_key(tenant: &str) -> String {
    format!("messaging.teams.setup.{tenant}")
}

fn default_state() -> Value {
    json!({
        "done": [],
        "values": {
            "config": {
                "bot_display_name": "Greentic Bot",
                "public_base_url": "https://runtime.example.test"
            },
            "last_setup_result": Value::Null
        }
    })
}

fn load_state(tenant: &str) -> Value {
    match state_store::read(&state_key(tenant), None) {
        Ok(bytes) if !bytes.is_empty() => {
            serde_json::from_slice(&bytes).unwrap_or_else(|_| default_state())
        }
        _ => default_state(),
    }
}

fn save_state(tenant: &str, state: &Value) {
    if let Ok(bytes) = serde_json::to_vec(state) {
        let _ = state_store::write(&state_key(tenant), &bytes, None);
    }
}

fn done_ids(state: &Value) -> Vec<String> {
    state
        .get("done")
        .and_then(Value::as_array)
        .map(|items| {
            items
                .iter()
                .filter_map(|v| v.as_str().map(str::to_string))
                .collect()
        })
        .unwrap_or_default()
}

fn is_done(state: &Value, id: &str) -> bool {
    done_ids(state).iter().any(|d| d == id)
}

fn mark_done(state: &mut Value, id: &str) {
    if !is_done(state, id)
        && let Some(arr) = state.get_mut("done").and_then(Value::as_array_mut)
    {
        arr.push(Value::String(id.to_string()));
    }
}

fn done_count(state: &Value) -> usize {
    STEP_IDS.iter().filter(|id| is_done(state, id)).count()
}

fn values_mut(state: &mut Value) -> &mut Map<String, Value> {
    state
        .as_object_mut()
        .and_then(|m| m.get_mut("values"))
        .and_then(Value::as_object_mut)
        .expect("state.values is an object")
}

fn config_mut(state: &mut Value) -> &mut Map<String, Value> {
    values_mut(state)
        .entry("config")
        .or_insert_with(|| json!({}))
        .as_object_mut()
        .expect("values.config is an object")
}

fn set_result(state: &mut Value, step: &str, ok: bool, result: Value, next: &str) {
    values_mut(state).insert(
        "last_setup_result".to_string(),
        json!({ "step": step, "ok": ok, "result": result, "next": next }),
    );
}

fn merge_config(state: &mut Value, incoming: &Value) {
    if let Some(map) = incoming.as_object() {
        let config = config_mut(state);
        for (k, v) in map {
            // Empty form fields mean "no change" — don't clobber existing/default values.
            if v.as_str() == Some("") {
                continue;
            }
            config.insert(k.clone(), v.clone());
        }
    }
}

/// Run the executor for the current (first not-done) step.
fn advance(state: &mut Value) -> &'static str {
    match done_count(state) {
        // graph_admin_consent: start the device code on first Continue, then poll.
        0 => graph_poll(state),
        // bot_app_identity: create/reuse the Entra app + mint its secret.
        1 => bot_app_step(state),
        // microsoft_bot_channel_registration_consent: device-code login for Azure management.
        2 => management_poll(state),
        // bot_framework_endpoint_registration: POST to the registration service if configured.
        3 => bot_framework_registration_step(state),
        // teams_app_publish: build + upload the real Teams app package to the catalog.
        4 => teams_publish_step(state),
        // teams_app_user_install: install the published app for the signed-in user.
        5 => teams_install_step(state),
        // first_bot_framework_post: do NOT self-complete — it resolves when a real
        // inbound activity is observed (see record_activity + the GET handler).
        6 => "send a message to the bot in Teams to finish setup",
        _ => "setup complete",
    }
}

fn public_state(state: &Value, tenant: &str, next_override: Option<&str>) -> Value {
    let items: Vec<Value> = STEP_IDS
        .iter()
        .map(|id| {
            json!({
                "id": id,
                "label": step_label(id),
                "state": if is_done(state, id) { "done" } else { "pending" },
                "detail": Value::Null,
            })
        })
        .collect();
    let all_done = items
        .iter()
        .all(|item| item.get("state").and_then(Value::as_str) == Some("done"));
    let last_step = state
        .get("values")
        .and_then(|v| v.get("last_setup_result"))
        .and_then(|r| r.get("step"))
        .cloned()
        .unwrap_or(Value::Null);
    let next = next_override
        .map(str::to_string)
        .unwrap_or_else(|| "Click Continue setup to start.".to_string());

    let publish_done = is_done(state, "teams_app_publish");
    let add_to_teams_url = setup_link_value(state, "add_to_teams_url");
    let open_bot_chat_url = setup_link_value(state, "open_bot_chat_url");

    json!({
        "ok": true,
        "setup_status": {
            "ok": all_done,
            "blocked": Value::Null,
            "items": items,
            "last_step": last_step,
            "next": next,
            "selected": { "env": "dev", "provider_id": "messaging-teams", "tenant": tenant, "team": "default" },
        },
        "teams_app": {
            "ok": publish_done,
            "add_to_teams_url": add_to_teams_url.clone(),
            "open_bot_chat_url": open_bot_chat_url.clone(),
        },
        "add_to_teams_url": add_to_teams_url,
        "open_bot_chat_url": open_bot_chat_url,
        "values": state.get("values").cloned().unwrap_or_else(|| json!({})),
    })
}

fn setup_link_value(state: &Value, key: &str) -> Value {
    let values = state.get("values");
    for path in [
        &["config", key][..],
        &["last_teams_app_publish", key][..],
        &["last_teams_app_install", key][..],
        &["last_setup_result", "result", key][..],
    ] {
        if let Some(value) = value_at_path(values, path)
            && value.as_str().is_some_and(|s| !s.trim().is_empty())
        {
            return value.clone();
        }
    }
    Value::Null
}

fn value_at_path<'a>(root: Option<&'a Value>, path: &[&str]) -> Option<&'a Value> {
    let mut current = root?;
    for segment in path {
        current = current.get(*segment)?;
    }
    Some(current)
}

// ── graph_admin_consent: Microsoft device-code OAuth ────────────────────────

fn cfg_str(state: &Value, key: &str) -> String {
    // Prefer values.config (setup answers); fall back to values top-level where
    // derived tokens such as azure_management_access_token are stored.
    let values = state.get("values");
    values
        .and_then(|v| v.get("config"))
        .and_then(|c| c.get(key))
        .or_else(|| values.and_then(|v| v.get(key)))
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_string()
}

fn graph_authority(state: &Value) -> String {
    let tenant = cfg_str(state, "azure_auth_tenant");
    let tenant = if tenant.trim().is_empty() {
        "organizations".to_string()
    } else {
        tenant
    };
    format!("https://login.microsoftonline.com/{tenant}")
}

fn graph_client_id(state: &Value) -> String {
    let id = cfg_str(state, "graph_setup_client_id");
    if id.trim().is_empty() {
        GRAPH_CLIENT_ID_DEFAULT.to_string()
    } else {
        id
    }
}

fn management_client_id(state: &Value) -> String {
    let id = cfg_str(state, "azure_setup_client_id");
    if id.trim().is_empty() {
        GRAPH_CLIENT_ID_DEFAULT.to_string()
    } else {
        id
    }
}

/// POST an `x-www-form-urlencoded` body and parse the JSON response. The body is
/// parsed regardless of HTTP status — the token endpoint returns 400 with an
/// `{"error": ...}` JSON body for `authorization_pending`, which is not a failure.
fn http_post_form(url: &str, form: &str) -> Result<Value, String> {
    let request = client::Request {
        method: "POST".into(),
        url: url.to_string(),
        headers: vec![(
            "Content-Type".into(),
            "application/x-www-form-urlencoded".into(),
        )],
        body: Some(form.as_bytes().to_vec()),
    };
    let resp = client::send(&request, None, None)
        .map_err(|e| format!("transport error: {}", e.message))?;
    let body = resp.body.unwrap_or_default();
    serde_json::from_slice(&body).map_err(|e| format!("invalid response from {url}: {e}"))
}

/// Mirror the device-login state the web component renders (user code + verify URL).
fn set_graph_pending(state: &mut Value, dc: &Value) {
    let user_code = dc
        .get("user_code")
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_string();
    let verification_uri = dc
        .get("verification_uri")
        .or_else(|| dc.get("verification_url"))
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_string();
    let expires_in = dc.get("expires_in").and_then(Value::as_i64).unwrap_or(900);
    let interval = dc.get("interval").and_then(Value::as_i64).unwrap_or(5);
    let message = dc
        .get("message")
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_string();
    let response = json!({
        "user_code": user_code,
        "verification_uri": verification_uri,
        "expires_in": expires_in,
        "interval": interval,
        "message": message,
    });
    {
        let config = config_mut(state);
        config.insert("oauth_kind".into(), json!("graph"));
        config.insert("oauth_user_code".into(), json!(user_code));
        config.insert("oauth_verification_uri".into(), json!(verification_uri));
    }
    values_mut(state).insert(
        "last_oauth".into(),
        json!({ "kind": "graph", "response": response }),
    );
    values_mut(state).insert(
        "oauth_device".into(),
        json!({ "device_code": dc.get("device_code").cloned().unwrap_or(Value::Null), "interval": interval, "expires_in": expires_in }),
    );
    set_result(
        state,
        "graph_admin_consent",
        false,
        json!({
            "ok": false,
            "pending_device_login": true,
            "login": { "user_code": user_code, "userCode": user_code, "url": verification_uri, "expiresIn": expires_in, "interval": interval },
            "body": response,
        }),
        "authorize in the opened browser, then wait for setup to continue",
    );
}

/// Start (or restart) the device-code flow. Returns the `next` message.
fn graph_start(state: &mut Value) -> &'static str {
    let url = format!("{}/oauth2/v2.0/devicecode", graph_authority(state));
    let form = format!(
        "client_id={}&scope={}",
        encode(&graph_client_id(state)),
        encode(GRAPH_SCOPES)
    );
    match http_post_form(&url, &form) {
        Ok(dc) if dc.get("device_code").is_some() => {
            set_graph_pending(state, &dc);
            "authorize in the opened browser, then wait for setup to continue"
        }
        Ok(err) => {
            let msg = err
                .get("error_description")
                .or_else(|| err.get("error"))
                .and_then(Value::as_str)
                .unwrap_or("device-code request failed")
                .to_string();
            set_result(
                state,
                "graph_admin_consent",
                false,
                json!({ "ok": false, "error": msg }),
                "device-code request failed",
            );
            "device-code request failed"
        }
        Err(_) => "device-code request failed",
    }
}

/// Poll the token endpoint once for the in-flight device code. Marks
/// graph_admin_consent done on success; keeps waiting on authorization_pending.
fn graph_poll(state: &mut Value) -> &'static str {
    // Already authorized — repeated polls (the wizard polls on a loop) must not
    // re-issue a fresh device code, or the step never settles to done.
    let have_token = state
        .get("values")
        .and_then(|v| v.get("graph_access_token"))
        .and_then(Value::as_str)
        .is_some_and(|t| !t.trim().is_empty());
    if have_token || is_done(state, "graph_admin_consent") {
        return "click Continue to continue setup";
    }
    let device_code = state
        .get("values")
        .and_then(|v| v.get("oauth_device"))
        .and_then(|d| d.get("device_code"))
        .and_then(Value::as_str)
        .map(str::to_string);
    let Some(device_code) = device_code else {
        return graph_start(state);
    };
    let url = format!("{}/oauth2/v2.0/token", graph_authority(state));
    let form = format!(
        "grant_type=urn:ietf:params:oauth:grant-type:device_code&client_id={}&device_code={}",
        encode(&graph_client_id(state)),
        encode(&device_code)
    );
    match http_post_form(&url, &form) {
        Ok(resp) => {
            if let Some(token) = resp.get("access_token").and_then(Value::as_str) {
                values_mut(state).insert("graph_access_token".into(), json!(token));
                let mut oauth = state
                    .get("values")
                    .and_then(|v| v.get("oauth"))
                    .cloned()
                    .unwrap_or_else(|| json!({}));
                if let Some(map) = oauth.as_object_mut() {
                    map.insert(
                        "graph".to_string(),
                        json!({
                            "ok": true,
                            "token_store_key": "graph_access_token",
                        }),
                    );
                }
                values_mut(state).insert("oauth".into(), oauth);
                values_mut(state).insert("oauth_device".into(), Value::Null);
                {
                    let config = config_mut(state);
                    config.remove("oauth_user_code");
                    config.remove("oauth_verification_uri");
                }
                mark_done(state, "graph_admin_consent");
                set_result(
                    state,
                    "graph_admin_consent",
                    true,
                    json!({ "ok": true }),
                    "click Continue to continue setup",
                );
                "click Continue to continue setup"
            } else {
                let error = resp.get("error").and_then(Value::as_str).unwrap_or("");
                if error == "authorization_pending" || error == "slow_down" {
                    "authorize in the opened browser, then wait for setup to continue"
                } else {
                    // expired_token / authorization_declined / bad_verification_code → restart.
                    values_mut(state).insert("oauth_device".into(), Value::Null);
                    let msg = resp
                        .get("error_description")
                        .and_then(Value::as_str)
                        .unwrap_or("device authorization failed");
                    set_result(
                        state,
                        "graph_admin_consent",
                        false,
                        json!({ "ok": false, "error": msg }),
                        "device authorization failed; click Continue to retry",
                    );
                    "device authorization failed; click Continue to retry"
                }
            }
        }
        Err(_) => "authorize in the opened browser, then wait for setup to continue",
    }
}

// ── microsoft_bot_channel_registration_consent: Azure management OAuth ──────

fn set_management_pending(state: &mut Value, dc: &Value) {
    let user_code = dc
        .get("user_code")
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_string();
    let verification_uri = dc
        .get("verification_uri")
        .or_else(|| dc.get("verification_url"))
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_string();
    let expires_in = dc.get("expires_in").and_then(Value::as_i64).unwrap_or(900);
    let interval = dc.get("interval").and_then(Value::as_i64).unwrap_or(5);
    let message = dc
        .get("message")
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_string();
    let response = json!({
        "user_code": user_code,
        "verification_uri": verification_uri,
        "expires_in": expires_in,
        "interval": interval,
        "message": message,
    });
    {
        let config = config_mut(state);
        config.insert("oauth_kind".into(), json!("management"));
        config.insert("azure_management_user_code".into(), json!(user_code));
        config.insert("oauth_verification_uri".into(), json!(verification_uri));
    }
    values_mut(state).insert(
        "last_oauth".into(),
        json!({ "kind": "management", "response": response }),
    );
    values_mut(state).insert(
        "azure_management_device".into(),
        json!({ "device_code": dc.get("device_code").cloned().unwrap_or(Value::Null), "interval": interval, "expires_in": expires_in }),
    );
    set_result(
        state,
        "microsoft_bot_channel_registration_consent",
        false,
        json!({
            "ok": false,
            "pending_device_login": true,
            "login": { "user_code": user_code, "userCode": user_code, "url": verification_uri, "expiresIn": expires_in, "interval": interval },
            "body": response,
        }),
        "authorize Microsoft bot channel registration, then wait for setup to continue",
    );
}

fn management_start(state: &mut Value) -> &'static str {
    let url = format!("{}/oauth2/v2.0/devicecode", graph_authority(state));
    let form = format!(
        "client_id={}&scope={}",
        encode(&management_client_id(state)),
        encode(AZURE_MANAGEMENT_SCOPES)
    );
    match http_post_form(&url, &form) {
        Ok(dc) if dc.get("device_code").is_some() => {
            set_management_pending(state, &dc);
            "authorize Microsoft bot channel registration, then wait for setup to continue"
        }
        Ok(err) => {
            let msg = err
                .get("error_description")
                .or_else(|| err.get("error"))
                .and_then(Value::as_str)
                .unwrap_or("Azure management device-code request failed")
                .to_string();
            set_result(
                state,
                "microsoft_bot_channel_registration_consent",
                false,
                json!({ "ok": false, "error": msg }),
                "Azure management device-code request failed",
            );
            "Azure management device-code request failed"
        }
        Err(_) => "Azure management device-code request failed",
    }
}

fn management_poll(state: &mut Value) -> &'static str {
    let existing_token = state
        .get("values")
        .and_then(|v| v.get("azure_management_access_token"))
        .and_then(Value::as_str)
        .filter(|token| !token.trim().is_empty())
        .map(str::to_string)
        .or_else(|| {
            let token = cfg_str(state, "azure_management_access_token");
            if token.trim().is_empty() {
                None
            } else {
                Some(token)
            }
        });
    if existing_token.is_some() {
        mark_done(state, "microsoft_bot_channel_registration_consent");
        set_result(
            state,
            "microsoft_bot_channel_registration_consent",
            true,
            json!({ "ok": true, "action": "supplied" }),
            "click Continue to continue setup",
        );
        return "click Continue to continue setup";
    }

    let device_code = state
        .get("values")
        .and_then(|v| v.get("azure_management_device"))
        .and_then(|d| d.get("device_code"))
        .and_then(Value::as_str)
        .map(str::to_string);
    let Some(device_code) = device_code else {
        return management_start(state);
    };
    let url = format!("{}/oauth2/v2.0/token", graph_authority(state));
    let form = format!(
        "grant_type=urn:ietf:params:oauth:grant-type:device_code&client_id={}&device_code={}",
        encode(&management_client_id(state)),
        encode(&device_code)
    );
    match http_post_form(&url, &form) {
        Ok(resp) => {
            if let Some(token) = resp.get("access_token").and_then(Value::as_str) {
                values_mut(state).insert("azure_management_access_token".into(), json!(token));
                let mut oauth = state
                    .get("values")
                    .and_then(|v| v.get("oauth"))
                    .cloned()
                    .unwrap_or_else(|| json!({}));
                if let Some(map) = oauth.as_object_mut() {
                    map.insert(
                        "management".to_string(),
                        json!({
                            "ok": true,
                            "token_store_key": "azure_management_access_token",
                        }),
                    );
                }
                values_mut(state).insert("oauth".into(), oauth);
                values_mut(state).insert("azure_management_device".into(), Value::Null);
                {
                    let config = config_mut(state);
                    config.remove("azure_management_user_code");
                    config.remove("oauth_verification_uri");
                    config.insert("oauth_kind".into(), json!("graph"));
                }
                mark_done(state, "microsoft_bot_channel_registration_consent");
                set_result(
                    state,
                    "microsoft_bot_channel_registration_consent",
                    true,
                    json!({ "ok": true }),
                    "click Continue to continue setup",
                );
                "click Continue to continue setup"
            } else {
                let error = resp.get("error").and_then(Value::as_str).unwrap_or("");
                if error == "authorization_pending" || error == "slow_down" {
                    "authorize Microsoft bot channel registration, then wait for setup to continue"
                } else {
                    values_mut(state).insert("azure_management_device".into(), Value::Null);
                    let msg = resp
                        .get("error_description")
                        .and_then(Value::as_str)
                        .unwrap_or("Azure management device authorization failed");
                    set_result(
                        state,
                        "microsoft_bot_channel_registration_consent",
                        false,
                        json!({ "ok": false, "error": msg }),
                        "Azure management authorization failed; click Continue to retry",
                    );
                    "Azure management authorization failed; click Continue to retry"
                }
            }
        }
        Err(_) => "authorize Microsoft bot channel registration, then wait for setup to continue",
    }
}

// ── bot_app_identity: Microsoft Graph application (the bot identity) ─────────

const GRAPH_BASE: &str = "https://graph.microsoft.com/v1.0";

/// Authenticated Graph call. Returns `(status, json_body)`; the body is parsed
/// best-effort (Graph errors carry a JSON `{"error": {...}}`).
fn graph_request(
    token: &str,
    method: &str,
    url: &str,
    body: Option<&Value>,
) -> Result<(u16, Value), String> {
    let mut headers = vec![("Authorization".into(), format!("Bearer {token}"))];
    let body_bytes = match body {
        Some(b) => {
            headers.push(("Content-Type".into(), "application/json".into()));
            Some(serde_json::to_vec(b).map_err(|e| e.to_string())?)
        }
        None => None,
    };
    let request = client::Request {
        method: method.into(),
        url: url.to_string(),
        headers,
        body: body_bytes,
    };
    let resp = client::send(&request, None, None)
        .map_err(|e| format!("transport error: {}", e.message))?;
    let status = resp.status as u16;
    let bytes = resp.body.unwrap_or_default();
    let json = if bytes.is_empty() {
        Value::Null
    } else {
        serde_json::from_slice(&bytes).unwrap_or(Value::Null)
    };
    Ok((status, json))
}

fn graph_error_message(body: &Value, fallback: &str) -> String {
    body.get("error")
        .and_then(|e| e.get("message"))
        .and_then(Value::as_str)
        .unwrap_or(fallback)
        .to_string()
}

fn bot_app_fail(state: &mut Value, msg: &str) -> &'static str {
    set_result(
        state,
        "bot_app_identity",
        false,
        json!({ "ok": false, "error": msg }),
        msg,
    );
    "bot app setup failed; click Continue to retry"
}

/// Create or reuse the Entra application used by Bot Framework, then mint a secret.
fn bot_app_step(state: &mut Value) -> &'static str {
    // If the operator supplied bot credentials, use them as-is (no Graph app creation).
    let supplied_id = cfg_str(state, "bot_app_id");
    let supplied_pw = cfg_str(state, "bot_app_password");
    if !supplied_id.trim().is_empty() && !supplied_pw.trim().is_empty() {
        mark_done(state, "bot_app_identity");
        set_result(
            state,
            "bot_app_identity",
            true,
            json!({ "ok": true, "action": "supplied", "bot_app_id": supplied_id, "app_id": supplied_id }),
            "click Continue to continue setup",
        );
        return "click Continue to continue setup";
    }
    let token = match state
        .get("values")
        .and_then(|v| v.get("graph_access_token"))
        .and_then(Value::as_str)
    {
        Some(t) if !t.is_empty() => t.to_string(),
        _ => return bot_app_fail(state, "Microsoft Graph authorization is required first"),
    };
    let display_name = {
        let d = cfg_str(state, "bot_display_name");
        if d.trim().is_empty() {
            "Greentic Bot".to_string()
        } else {
            d
        }
    };

    // Reuse an existing application with the same display name when present.
    let find_url = format!(
        "{GRAPH_BASE}/applications?$filter=displayName eq '{}'&$select=id,appId",
        encode(&display_name.replace('\'', "''"))
    );
    let existing = match graph_request(&token, "GET", &find_url, None) {
        Ok((s, body)) if s < 300 => body
            .get("value")
            .and_then(Value::as_array)
            .and_then(|a| a.first())
            .map(|app| {
                (
                    app.get("id").and_then(Value::as_str).map(str::to_string),
                    app.get("appId").and_then(Value::as_str).map(str::to_string),
                )
            }),
        _ => None,
    };

    let (object_id, app_id, action) = match existing {
        Some((Some(oid), Some(aid))) => (oid, aid, "keep"),
        _ => {
            let create_body =
                json!({ "displayName": display_name, "signInAudience": "AzureADMyOrg" });
            match graph_request(
                &token,
                "POST",
                &format!("{GRAPH_BASE}/applications"),
                Some(&create_body),
            ) {
                Ok((s, body)) if s < 300 => {
                    match (
                        body.get("id").and_then(Value::as_str).map(str::to_string),
                        body.get("appId")
                            .and_then(Value::as_str)
                            .map(str::to_string),
                    ) {
                        (Some(oid), Some(aid)) => (oid, aid, "create"),
                        _ => {
                            return bot_app_fail(
                                state,
                                "Graph application response missing id/appId",
                            );
                        }
                    }
                }
                Ok((_, body)) => {
                    return bot_app_fail(
                        state,
                        &graph_error_message(&body, "failed to create Graph application"),
                    );
                }
                Err(e) => return bot_app_fail(state, &e),
            }
        }
    };

    // Mint a fresh client secret on the application.
    let pw_body =
        json!({ "passwordCredential": { "displayName": "Greentic Teams bot setup secret" } });
    let secret = match graph_request(
        &token,
        "POST",
        &format!("{GRAPH_BASE}/applications/{object_id}/addPassword"),
        Some(&pw_body),
    ) {
        Ok((s, body)) if s < 300 => body
            .get("secretText")
            .and_then(Value::as_str)
            .map(str::to_string),
        Ok((_, body)) => {
            return bot_app_fail(
                state,
                &graph_error_message(&body, "failed to add application password"),
            );
        }
        Err(e) => return bot_app_fail(state, &e),
    };

    {
        let config = config_mut(state);
        config.insert("bot_app_id".into(), json!(app_id));
        if let Some(sec) = &secret {
            config.insert("bot_app_password".into(), json!(sec));
        }
    }
    mark_done(state, "bot_app_identity");
    set_result(
        state,
        "bot_app_identity",
        true,
        json!({ "ok": true, "action": action, "bot_app_id": app_id, "app_id": app_id, "secret_action": if secret.is_some() { "generated_secret" } else { "reused" } }),
        "click Continue to continue setup",
    );
    "click Continue to continue setup"
}

// ── teams_app_publish / teams_app_user_install: real Graph calls ────────────

fn graph_token(state: &Value) -> Option<String> {
    state
        .get("values")
        .and_then(|v| v.get("graph_access_token"))
        .and_then(Value::as_str)
        .filter(|t| !t.is_empty())
        .map(str::to_string)
}

/// Graph call with a raw body + explicit content type (used for the zip upload).
fn graph_request_bytes(
    token: &str,
    method: &str,
    url: &str,
    content_type: &str,
    body: Vec<u8>,
) -> Result<(u16, Value), String> {
    let headers = vec![
        ("Authorization".into(), format!("Bearer {token}")),
        ("Content-Type".into(), content_type.to_string()),
    ];
    let request = client::Request {
        method: method.into(),
        url: url.to_string(),
        headers,
        body: Some(body),
    };
    let resp = client::send(&request, None, None)
        .map_err(|e| format!("transport error: {}", e.message))?;
    let status = resp.status as u16;
    let bytes = resp.body.unwrap_or_default();
    let json = if bytes.is_empty() {
        Value::Null
    } else {
        serde_json::from_slice(&bytes).unwrap_or(Value::Null)
    };
    Ok((status, json))
}

fn step_fail(state: &mut Value, step: &str, msg: &str) -> &'static str {
    set_result(
        state,
        step,
        false,
        json!({ "ok": false, "error": msg }),
        msg,
    );
    "step failed; click Continue to retry"
}

/// Build + upload the Teams app package to the tenant app catalog (real Graph call).
fn teams_publish_step(state: &mut Value) -> &'static str {
    let Some(token) = graph_token(state) else {
        return step_fail(
            state,
            "teams_app_publish",
            "Microsoft Graph authorization is required (complete step 1)",
        );
    };
    let bot_app_id = cfg_str(state, "bot_app_id");
    if bot_app_id.trim().is_empty() {
        return step_fail(
            state,
            "teams_app_publish",
            "bot_app_id is required before publishing",
        );
    }
    // Use the bot app id as the stable Teams manifest id (a valid GUID).
    let app_name = cfg_str(state, "bot_display_name");
    let teams_app_version = cfg_str(state, "teams_app_version");
    let package =
        crate::teams_pkg::build_package(&bot_app_id, &bot_app_id, &teams_app_version, &app_name);
    let url = format!("{GRAPH_BASE}/appCatalogs/teamsApps");
    match graph_request_bytes(&token, "POST", &url, "application/zip", package) {
        Ok((s, body)) if s < 300 => {
            let catalog_id = body
                .get("id")
                .and_then(Value::as_str)
                .unwrap_or(&bot_app_id)
                .to_string();
            finalize_publish(state, &catalog_id, "publish")
        }
        // App already published in this tenant — resolve its catalog id and proceed.
        Ok((409, _)) => match lookup_existing_teams_app(&token, &bot_app_id) {
            Some(catalog_id) => finalize_publish(state, &catalog_id, "exists"),
            None => step_fail(
                state,
                "teams_app_publish",
                "App already exists but its catalog id could not be resolved (externalId lookup failed)",
            ),
        },
        Ok((status, body)) => step_fail(
            state,
            "teams_app_publish",
            &format!(
                "Graph publish failed (HTTP {status}): {}",
                graph_error_message(&body, "unknown error")
            ),
        ),
        Err(e) => step_fail(state, "teams_app_publish", &e),
    }
}

fn finalize_publish(state: &mut Value, catalog_id: &str, action: &str) -> &'static str {
    let add_url =
        format!("https://teams.microsoft.com/l/app/{catalog_id}?source=app-details-dialog");
    {
        let config = config_mut(state);
        config.insert("teams_app_id".into(), json!(catalog_id));
        config.insert("add_to_teams_url".into(), json!(add_url));
    }
    let result = json!({ "ok": true, "action": action, "add_to_teams_url": add_url, "catalog_app_id": catalog_id });
    values_mut(state).insert("last_teams_app_publish".into(), result.clone());
    mark_done(state, "teams_app_publish");
    set_result(
        state,
        "teams_app_publish",
        true,
        result,
        "open Add to Teams, install the app, then continue",
    );
    "open Add to Teams, install the app, then continue"
}

/// Find an already-published Teams app by its manifest externalId (we use bot_app_id).
fn lookup_existing_teams_app(token: &str, external_id: &str) -> Option<String> {
    let url = format!(
        "{GRAPH_BASE}/appCatalogs/teamsApps?$filter=externalId eq '{}'",
        encode(external_id)
    );
    match graph_request(token, "GET", &url, None) {
        Ok((s, body)) if s < 300 => body
            .get("value")
            .and_then(Value::as_array)
            .and_then(|a| a.first())
            .and_then(|app| app.get("id"))
            .and_then(Value::as_str)
            .map(str::to_string),
        _ => None,
    }
}

fn graph_user_id(state: &mut Value, token: &str) -> Option<String> {
    if let Some(uid) = state
        .get("values")
        .and_then(|v| v.get("user_id"))
        .and_then(Value::as_str)
        && !uid.is_empty()
    {
        return Some(uid.to_string());
    }
    match graph_request(token, "GET", &format!("{GRAPH_BASE}/me"), None) {
        Ok((s, body)) if s < 300 => {
            let uid = body.get("id").and_then(Value::as_str)?.to_string();
            values_mut(state).insert("user_id".into(), json!(uid));
            Some(uid)
        }
        _ => None,
    }
}

/// Install the published Teams app for the signed-in user (real Graph call).
fn teams_install_step(state: &mut Value) -> &'static str {
    let Some(token) = graph_token(state) else {
        return step_fail(
            state,
            "teams_app_user_install",
            "Microsoft Graph authorization is required (complete step 1)",
        );
    };
    let teams_app_id = cfg_str(state, "teams_app_id");
    if teams_app_id.trim().is_empty() {
        return step_fail(
            state,
            "teams_app_user_install",
            "teams_app_id missing — publish the app first",
        );
    }
    let bot_app_id = cfg_str(state, "bot_app_id");
    let Some(user_id) = graph_user_id(state, &token) else {
        return step_fail(
            state,
            "teams_app_user_install",
            "could not resolve the signed-in user (GET /me failed)",
        );
    };
    let body = json!({
        "teamsApp@odata.bind": format!("https://graph.microsoft.com/v1.0/appCatalogs/teamsApps/{teams_app_id}")
    });
    let url = format!("{GRAPH_BASE}/users/{user_id}/teamwork/installedApps");
    match graph_request(&token, "POST", &url, Some(&body)) {
        // 409 = already installed for this user, which is fine.
        Ok((s, _)) if s < 300 || s == 409 => {
            let chat_url = format!(
                "https://teams.microsoft.com/l/chat/0/0?users=28:{bot_app_id}&message=hello"
            );
            config_mut(state).insert("open_bot_chat_url".into(), json!(chat_url));
            let result = json!({ "ok": true, "action": "install", "open_bot_chat_url": chat_url, "installed_for": user_id });
            values_mut(state).insert("last_teams_app_install".into(), result.clone());
            mark_done(state, "teams_app_user_install");
            set_result(
                state,
                "teams_app_user_install",
                true,
                result,
                "open the bot chat and send hello",
            );
            "open the bot chat and send hello"
        }
        Ok((status, body)) => step_fail(
            state,
            "teams_app_user_install",
            &format!(
                "Graph install failed (HTTP {status}): {}",
                graph_error_message(&body, "unknown error")
            ),
        ),
        Err(e) => step_fail(state, "teams_app_user_install", &e),
    }
}

/// Register the messaging endpoint with the provider-owned setup operation.
fn bot_framework_registration_step(state: &mut Value) -> &'static str {
    let public_base = cfg_str(state, "public_base_url");
    let team = {
        let t = cfg_str(state, "team");
        if t.trim().is_empty() {
            "default".to_string()
        } else {
            t
        }
    };
    let tenant = state
        .get("values")
        .and_then(|v| v.get("tenant"))
        .and_then(Value::as_str)
        .unwrap_or("demo")
        .to_string();
    let messaging_endpoint = format!(
        "{}/v1/messaging/ingress/messaging-teams/{}/{}",
        public_base.trim_end_matches('/'),
        tenant,
        team
    );
    let body = json!({
        "provider_id": "messaging-teams",
        "bot_app_id": cfg_str(state, "bot_app_id"),
        "bot_app_password": cfg_str(state, "bot_app_password"),
        "messaging_endpoint": messaging_endpoint,
        "public_base_url": public_base,
        "bot_display_name": cfg_str(state, "bot_display_name"),
        "azure_management_access_token": cfg_str(state, "azure_management_access_token"),
        "azure_auth_tenant": cfg_str(state, "azure_auth_tenant"),
        "azure_subscription_id": cfg_str(state, "azure_subscription_id"),
        "azure_resource_group": cfg_str(state, "azure_resource_group"),
        "azure_resource_group_location": cfg_str(state, "azure_resource_group_location"),
        "azure_location": cfg_str(state, "azure_location"),
        "azure_bot_name": cfg_str(state, "azure_bot_name"),
        "channel": "msteams",
        "tenant": tenant,
        "team": team,
    });
    let result = crate::handle_bot_framework_registration_body(&body);
    if result.get("ok").and_then(Value::as_bool) == Some(true) {
        mark_done(state, "bot_framework_endpoint_registration");
        set_result(
            state,
            "bot_framework_endpoint_registration",
            true,
            result,
            "click Continue to continue setup",
        );
        "click Continue to continue setup"
    } else {
        step_fail(
            state,
            "bot_framework_endpoint_registration",
            result
                .get("error")
                .and_then(Value::as_str)
                .unwrap_or("Bot Framework registration failed"),
        )
    }
}

fn respond(status: u16, body: Value) -> Result<String, String> {
    serde_json::to_string(&json!({
        "response": {
            "status": status,
            "headers": { "content-type": "application/json" },
            "body_json": body,
        }
    }))
    .map_err(|_| "other error: setup serialization failed".to_string())
}

/// Record an inbound Bot Framework activity into the setup state so the
/// `first_bot_framework_post` step can complete (the real runtime observation).
pub fn record_activity(tenant: &str, activity: &Value) {
    let mut state = load_state(tenant);
    let service_url = activity
        .get("serviceUrl")
        .and_then(Value::as_str)
        .unwrap_or("");
    let conv_id = activity
        .get("conversation")
        .and_then(|c| c.get("id"))
        .and_then(Value::as_str)
        .unwrap_or("");
    values_mut(&mut state).insert(
        "last_activity".into(),
        json!({
            "serviceUrl": service_url,
            "conversation": { "id": conv_id },
            "text": activity.get("text").and_then(Value::as_str).unwrap_or(""),
        }),
    );
    save_state(tenant, &state);
}

/// Returns `Some(response)` when `path` is a setup route, else `None`.
pub fn handle(method: &str, path: &str, body_json: &str) -> Option<Result<String, String>> {
    let (tenant, action) = parse_path(path)?;
    let body: Value = serde_json::from_str(body_json).unwrap_or_else(|_| json!({}));
    let mut state = load_state(&tenant);
    values_mut(&mut state).insert("tenant".into(), json!(tenant));

    let snapshot = match (method, action.as_str()) {
        ("GET", "") => {
            // Settle any in-flight device login as the wizard polls GET state (it
            // refreshes every few seconds), so a step advances on authorization
            // even when the UI's own action-flow polling stalls.
            let graph_pending = state
                .get("values")
                .and_then(|v| v.get("oauth_device"))
                .and_then(|d| d.get("device_code"))
                .and_then(Value::as_str)
                .is_some_and(|c| !c.trim().is_empty());
            if graph_pending && !is_done(&state, "graph_admin_consent") {
                graph_poll(&mut state);
                save_state(&tenant, &state);
            }
            let mgmt_pending = state
                .get("values")
                .and_then(|v| v.get("azure_management_device"))
                .and_then(|d| d.get("device_code"))
                .and_then(Value::as_str)
                .is_some_and(|c| !c.trim().is_empty());
            if mgmt_pending && !is_done(&state, "microsoft_bot_channel_registration_consent") {
                management_poll(&mut state);
                save_state(&tenant, &state);
            }
            // first_bot_framework_post completes once a REAL inbound activity has been
            // observed (recorded into values.last_activity by record_activity()).
            let has_activity = state
                .get("values")
                .and_then(|v| v.get("last_activity"))
                .map(|a| !a.is_null())
                .unwrap_or(false);
            if is_done(&state, "teams_app_user_install")
                && !is_done(&state, "first_bot_framework_post")
                && has_activity
            {
                mark_done(&mut state, "first_bot_framework_post");
                set_result(
                    &mut state,
                    "first_bot_framework_post",
                    true,
                    json!({ "ok": true }),
                    "setup complete",
                );
                save_state(&tenant, &state);
            }
            public_state(&state, &tenant, None)
        }
        ("POST", "config") => {
            if let Some(config) = body.get("config") {
                merge_config(&mut state, config);
            }
            save_state(&tenant, &state);
            public_state(&state, &tenant, Some("configuration saved"))
        }
        ("POST", "next") => {
            if let Some(config) = body.get("config") {
                merge_config(&mut state, config);
            }
            let next = advance(&mut state);
            save_state(&tenant, &state);
            public_state(&state, &tenant, Some(next))
        }
        // OAuth device-code: /start (re)issues a device code, /complete polls for the token.
        ("POST", a) if a.starts_with("oauth/") => {
            let is_management = a.starts_with("oauth/management/");
            let next = if a.ends_with("/start") {
                if is_management {
                    management_start(&mut state)
                } else {
                    graph_start(&mut state)
                }
            } else if is_management {
                management_poll(&mut state)
            } else {
                graph_poll(&mut state)
            };
            save_state(&tenant, &state);
            public_state(&state, &tenant, Some(next))
        }
        ("POST", a) if a.starts_with("teams-app/") => {
            let next = advance(&mut state);
            save_state(&tenant, &state);
            public_state(&state, &tenant, Some(next))
        }
        ("GET", a) if a.ends_with("package.zip") => {
            return Some(respond(
                200,
                json!({ "ok": true, "note": "package not yet generated" }),
            ));
        }
        _ => public_state(&state, &tenant, None),
    };

    Some(respond(200, snapshot))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn config_state(config: Value) -> Value {
        let mut state = default_state();
        values_mut(&mut state).insert("config".into(), config);
        state
    }

    #[test]
    fn cfg_str_reads_config_then_falls_back_to_values_top_level() {
        // bot_app_id lives in config; the management token lives at values
        // top-level (where management_poll stores it). Both must resolve.
        let state = json!({"values": {
            "config": {"bot_app_id": "app-123"},
            "azure_management_access_token": "tok-xyz"
        }});
        assert_eq!(cfg_str(&state, "bot_app_id"), "app-123");
        assert_eq!(cfg_str(&state, "azure_management_access_token"), "tok-xyz");
        // config wins when a key exists in both places
        let both = json!({"values": {"config": {"k": "from-config"}, "k": "from-values"}});
        assert_eq!(cfg_str(&both, "k"), "from-config");
    }

    #[test]
    fn graph_poll_does_not_reissue_after_token_acquired() {
        // After success the device code is cleared; a repeated poll must report
        // done, not mint a fresh code (which made the stepper stall forever).
        let mut state = default_state();
        values_mut(&mut state).insert("graph_access_token".into(), json!("tok-abc"));
        let msg = graph_poll(&mut state);
        assert_eq!(msg, "click Continue to continue setup");
        let reissued = state
            .get("values")
            .and_then(|v| v.get("oauth_device"))
            .map(|d| !d.is_null())
            .unwrap_or(false);
        assert!(!reissued, "graph_poll re-issued a code after the token was acquired");
    }

    #[test]
    fn parse_path_extracts_tenant_and_action() {
        assert_eq!(
            parse_path("/v1/messaging/setup/messaging-teams/demo/next"),
            Some(("demo".to_string(), "next".to_string()))
        );
        assert_eq!(
            parse_path("/v1/messaging/setup/messaging-teams/demo/oauth/graph/start/"),
            Some(("demo".to_string(), "oauth/graph/start".to_string()))
        );
        assert_eq!(parse_path("/v1/messaging/setup/messaging-teams/"), None);
        assert_eq!(parse_path("/v1/messaging/other/demo"), None);
    }

    #[test]
    fn setup_state_tracks_done_steps_idempotently() {
        let mut state = default_state();

        mark_done(&mut state, "graph_admin_consent");
        mark_done(&mut state, "graph_admin_consent");

        assert!(is_done(&state, "graph_admin_consent"));
        assert_eq!(done_count(&state), 1);
        assert_eq!(done_ids(&state), vec!["graph_admin_consent"]);
    }

    #[test]
    fn public_state_surfaces_step_status_and_teams_links() {
        let mut state = default_state();
        mark_done(&mut state, "graph_admin_consent");
        mark_done(&mut state, "teams_app_publish");
        values_mut(&mut state).insert(
            "last_teams_app_publish".into(),
            json!({ "add_to_teams_url": "https://teams.microsoft.com/l/app/catalog-id" }),
        );
        values_mut(&mut state).insert(
            "last_teams_app_install".into(),
            json!({ "open_bot_chat_url": "https://teams.microsoft.com/l/chat/0/0" }),
        );

        let view = public_state(&state, "demo", Some("next action"));

        assert_eq!(view["setup_status"]["items"].as_array().unwrap().len(), 7);
        assert_eq!(
            view["setup_status"]["selected"]["tenant"].as_str(),
            Some("demo")
        );
        assert_eq!(view["setup_status"]["next"].as_str(), Some("next action"));
        assert_eq!(view["teams_app"]["ok"].as_bool(), Some(true));
        assert_eq!(
            view["teams_app"]["add_to_teams_url"].as_str(),
            Some("https://teams.microsoft.com/l/app/catalog-id")
        );
        assert_eq!(
            view["add_to_teams_url"].as_str(),
            Some("https://teams.microsoft.com/l/app/catalog-id")
        );
        assert_eq!(
            view["teams_app"]["open_bot_chat_url"].as_str(),
            Some("https://teams.microsoft.com/l/chat/0/0")
        );
        assert_eq!(
            view["open_bot_chat_url"].as_str(),
            Some("https://teams.microsoft.com/l/chat/0/0")
        );
    }

    #[test]
    fn public_state_preserves_final_links_after_later_retryable_block() {
        let mut state = config_state(json!({
            "bot_app_id": "00000000-0000-0000-0000-000000000001"
        }));

        finalize_publish(&mut state, "catalog-id", "publish");
        step_fail(
            &mut state,
            "teams_app_user_install",
            "could not resolve the signed-in user (GET /me failed)",
        );

        let view = public_state(&state, "demo", None);

        assert_eq!(view["setup_status"]["ok"].as_bool(), Some(false));
        assert_eq!(
            view["add_to_teams_url"].as_str(),
            Some("https://teams.microsoft.com/l/app/catalog-id?source=app-details-dialog")
        );
        assert_eq!(
            view["teams_app"]["add_to_teams_url"].as_str(),
            Some("https://teams.microsoft.com/l/app/catalog-id?source=app-details-dialog")
        );
    }

    #[test]
    fn public_state_reports_complete_only_when_all_steps_done() {
        let mut state = default_state();
        for step in STEP_IDS {
            mark_done(&mut state, step);
        }

        let view = public_state(&state, "demo", None);

        assert_eq!(view["setup_status"]["ok"].as_bool(), Some(true));
    }

    #[test]
    fn merge_config_preserves_existing_values_for_empty_inputs() {
        let mut state = config_state(json!({
            "public_base_url": "https://runtime.example.test",
            "bot_display_name": "Greentic Bot"
        }));

        merge_config(
            &mut state,
            &json!({
                "public_base_url": "",
                "bot_display_name": "Ops Bot",
                "team": "support"
            }),
        );

        assert_eq!(
            cfg_str(&state, "public_base_url"),
            "https://runtime.example.test"
        );
        assert_eq!(cfg_str(&state, "bot_display_name"), "Ops Bot");
        assert_eq!(cfg_str(&state, "team"), "support");
    }

    #[test]
    fn oauth_pending_state_keeps_graph_and_management_codes_separate() {
        let mut state = default_state();

        set_graph_pending(
            &mut state,
            &json!({
                "device_code": "graph-device",
                "user_code": "GRAPH-CODE",
                "verification_uri": "https://microsoft.com/devicelogin",
                "expires_in": 600,
                "interval": 4
            }),
        );
        assert_eq!(cfg_str(&state, "oauth_kind"), "graph");
        assert_eq!(cfg_str(&state, "oauth_user_code"), "GRAPH-CODE");
        assert_eq!(
            state["values"]["oauth_device"]["device_code"].as_str(),
            Some("graph-device")
        );

        set_management_pending(
            &mut state,
            &json!({
                "device_code": "management-device",
                "user_code": "MGMT-CODE",
                "verification_url": "https://microsoft.com/devicelogin",
                "expires_in": 900,
                "interval": 5
            }),
        );

        assert_eq!(cfg_str(&state, "oauth_kind"), "management");
        assert_eq!(cfg_str(&state, "oauth_user_code"), "GRAPH-CODE");
        assert_eq!(cfg_str(&state, "azure_management_user_code"), "MGMT-CODE");
        assert_eq!(
            state["values"]["azure_management_device"]["device_code"].as_str(),
            Some("management-device")
        );
        assert_eq!(
            state["values"]["last_setup_result"]["step"].as_str(),
            Some("microsoft_bot_channel_registration_consent")
        );
    }

    #[test]
    fn management_poll_accepts_supplied_token_without_network() {
        let mut state = config_state(json!({
            "azure_management_access_token": "management-token"
        }));

        let next = management_poll(&mut state);

        assert_eq!(next, "click Continue to continue setup");
        assert!(is_done(
            &state,
            "microsoft_bot_channel_registration_consent"
        ));
        assert_eq!(
            state["values"]["last_setup_result"]["result"]["action"].as_str(),
            Some("supplied")
        );
    }

    #[test]
    fn bot_app_step_accepts_supplied_credentials_without_graph() {
        let mut state = config_state(json!({
            "bot_app_id": "00000000-0000-0000-0000-000000000001",
            "bot_app_password": "secret"
        }));

        let next = bot_app_step(&mut state);

        assert_eq!(next, "click Continue to continue setup");
        assert!(is_done(&state, "bot_app_identity"));
        assert_eq!(
            state["values"]["last_setup_result"]["result"]["action"].as_str(),
            Some("supplied")
        );
    }

    #[test]
    fn bot_framework_registration_step_validates_required_config() {
        let mut missing = default_state();
        let next = bot_framework_registration_step(&mut missing);
        assert_eq!(next, "step failed; click Continue to retry");
        assert!(
            state_error(&missing)
                .unwrap()
                .starts_with("missing bot_app_id")
        );

        let mut state = config_state(json!({
            "public_base_url": "https://runtime.example.test",
            "team": "support",
            "bot_app_id": "00000000-0000-0000-0000-000000000001",
            "bot_app_password": "secret",
            "bot_display_name": "Greentic Bot",
            "azure_management_access_token": "management-token",
            "azure_bot_name": "Greentic Teams Bot"
        }));
        values_mut(&mut state).insert("tenant".into(), json!("demo"));

        let next = bot_framework_registration_step(&mut state);

        assert_eq!(next, "click Continue to continue setup");
        assert!(is_done(&state, "bot_framework_endpoint_registration"));
        assert_eq!(
            state["values"]["last_setup_result"]["result"]["target_messaging_endpoint"].as_str(),
            Some("https://runtime.example.test/v1/messaging/ingress/messaging-teams/demo/support")
        );
    }

    #[test]
    fn publish_and_install_helpers_record_links_and_errors() {
        let mut state = config_state(json!({
            "bot_app_id": "00000000-0000-0000-0000-000000000001"
        }));

        let next = finalize_publish(&mut state, "catalog-id", "exists");
        assert_eq!(next, "open Add to Teams, install the app, then continue");
        assert!(is_done(&state, "teams_app_publish"));
        assert_eq!(cfg_str(&state, "teams_app_id"), "catalog-id");
        assert_eq!(
            state["values"]["last_teams_app_publish"]["action"].as_str(),
            Some("exists")
        );

        let mut no_token = default_state();
        let next = teams_install_step(&mut no_token);
        assert_eq!(next, "step failed; click Continue to retry");
        assert_eq!(
            state_error(&no_token).as_deref(),
            Some("Microsoft Graph authorization is required (complete step 1)")
        );

        let mut missing_app = default_state();
        values_mut(&mut missing_app).insert("graph_access_token".into(), json!("graph-token"));
        let next = teams_install_step(&mut missing_app);
        assert_eq!(next, "step failed; click Continue to retry");
        assert_eq!(
            state_error(&missing_app).as_deref(),
            Some("teams_app_id missing — publish the app first")
        );
    }

    #[test]
    fn graph_user_id_reuses_cached_value() {
        let mut state = default_state();
        values_mut(&mut state).insert("user_id".into(), json!("user-123"));

        assert_eq!(
            graph_user_id(&mut state, "unused-token"),
            Some("user-123".to_string())
        );
    }

    #[test]
    fn authority_and_client_id_helpers_use_defaults_and_overrides() {
        let default = default_state();
        assert_eq!(
            graph_authority(&default),
            "https://login.microsoftonline.com/organizations"
        );
        assert_eq!(graph_client_id(&default), GRAPH_CLIENT_ID_DEFAULT);
        assert_eq!(management_client_id(&default), GRAPH_CLIENT_ID_DEFAULT);
        assert_eq!(step_label("unknown"), "");
        assert_eq!(state_key("demo"), "messaging.teams.setup.demo");

        let configured = config_state(json!({
            "azure_auth_tenant": "contoso.onmicrosoft.com",
            "graph_setup_client_id": "graph-client",
            "azure_setup_client_id": "management-client"
        }));
        assert_eq!(
            graph_authority(&configured),
            "https://login.microsoftonline.com/contoso.onmicrosoft.com"
        );
        assert_eq!(graph_client_id(&configured), "graph-client");
        assert_eq!(management_client_id(&configured), "management-client");
    }

    #[test]
    fn advance_routes_each_non_oauth_stage() {
        let mut bot_app = config_state(json!({
            "bot_app_id": "00000000-0000-0000-0000-000000000001",
            "bot_app_password": "secret"
        }));
        mark_done(&mut bot_app, "graph_admin_consent");
        assert_eq!(advance(&mut bot_app), "click Continue to continue setup");
        assert!(is_done(&bot_app, "bot_app_identity"));

        let mut management = config_state(json!({
            "azure_management_access_token": "management-token"
        }));
        mark_done(&mut management, "graph_admin_consent");
        mark_done(&mut management, "bot_app_identity");
        assert_eq!(advance(&mut management), "click Continue to continue setup");
        assert!(is_done(
            &management,
            "microsoft_bot_channel_registration_consent"
        ));

        let mut registration = config_state(json!({
            "public_base_url": "https://runtime.example.test",
            "team": "default",
            "bot_app_id": "00000000-0000-0000-0000-000000000001",
            "bot_app_password": "secret",
            "bot_display_name": "Greentic Bot",
            "azure_management_access_token": "management-token"
        }));
        values_mut(&mut registration).insert("tenant".into(), json!("demo"));
        for step in [
            "graph_admin_consent",
            "bot_app_identity",
            "microsoft_bot_channel_registration_consent",
        ] {
            mark_done(&mut registration, step);
        }
        assert_eq!(
            advance(&mut registration),
            "click Continue to continue setup"
        );
        assert!(is_done(
            &registration,
            "bot_framework_endpoint_registration"
        ));

        let mut publish = default_state();
        for step in [
            "graph_admin_consent",
            "bot_app_identity",
            "microsoft_bot_channel_registration_consent",
            "bot_framework_endpoint_registration",
        ] {
            mark_done(&mut publish, step);
        }
        assert_eq!(
            advance(&mut publish),
            "step failed; click Continue to retry"
        );
        assert_eq!(
            state_error(&publish).as_deref(),
            Some("Microsoft Graph authorization is required (complete step 1)")
        );

        let mut install = default_state();
        for step in [
            "graph_admin_consent",
            "bot_app_identity",
            "microsoft_bot_channel_registration_consent",
            "bot_framework_endpoint_registration",
            "teams_app_publish",
        ] {
            mark_done(&mut install, step);
        }
        assert_eq!(
            advance(&mut install),
            "step failed; click Continue to retry"
        );
        assert_eq!(
            state_error(&install).as_deref(),
            Some("Microsoft Graph authorization is required (complete step 1)")
        );

        mark_done(&mut install, "teams_app_user_install");
        assert_eq!(
            advance(&mut install),
            "send a message to the bot in Teams to finish setup"
        );
        mark_done(&mut install, "first_bot_framework_post");
        assert_eq!(advance(&mut install), "setup complete");
    }

    #[test]
    fn bot_app_step_requires_graph_when_credentials_are_incomplete() {
        let mut state = config_state(json!({
            "bot_app_id": "00000000-0000-0000-0000-000000000001"
        }));

        let next = bot_app_step(&mut state);

        assert_eq!(next, "bot app setup failed; click Continue to retry");
        assert_eq!(
            state_error(&state).as_deref(),
            Some("Microsoft Graph authorization is required first")
        );
    }

    #[test]
    fn publish_step_validates_before_graph_upload() {
        let mut no_token = config_state(json!({
            "bot_app_id": "00000000-0000-0000-0000-000000000001"
        }));
        assert_eq!(
            teams_publish_step(&mut no_token),
            "step failed; click Continue to retry"
        );
        assert_eq!(
            state_error(&no_token).as_deref(),
            Some("Microsoft Graph authorization is required (complete step 1)")
        );

        let mut no_bot_app = default_state();
        values_mut(&mut no_bot_app).insert("graph_access_token".into(), json!("graph-token"));
        assert_eq!(
            teams_publish_step(&mut no_bot_app),
            "step failed; click Continue to retry"
        );
        assert_eq!(
            state_error(&no_bot_app).as_deref(),
            Some("bot_app_id is required before publishing")
        );
    }

    #[test]
    fn graph_error_message_uses_nested_message_or_fallback() {
        assert_eq!(
            graph_error_message(
                &json!({ "error": { "message": "Graph said no" } }),
                "fallback"
            ),
            "Graph said no"
        );
        assert_eq!(graph_error_message(&json!({}), "fallback"), "fallback");
    }

    #[test]
    fn respond_wraps_status_headers_and_body() {
        let body = respond(202, json!({ "ok": true })).unwrap();
        let parsed: Value = serde_json::from_str(&body).unwrap();

        assert_eq!(parsed["response"]["status"].as_u64(), Some(202));
        assert_eq!(
            parsed["response"]["headers"]["content-type"].as_str(),
            Some("application/json")
        );
        assert_eq!(parsed["response"]["body_json"]["ok"].as_bool(), Some(true));
    }

    fn state_error(state: &Value) -> Option<String> {
        state["values"]["last_setup_result"]["result"]["error"]
            .as_str()
            .map(str::to_string)
    }
}
