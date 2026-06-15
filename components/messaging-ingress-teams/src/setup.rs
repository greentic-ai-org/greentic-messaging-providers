//! Bot-framework Teams setup wizard backend (the `setup_status` state machine the
//! `greentic-teams-setup-v4` web component drives). Dispatched here from
//! `handle_webhook` for `/v1/messaging/setup/messaging-teams/...` requests.
//!
//! Each step runs a real executor: device-code OAuth (`graph_admin_consent`),
//! operator-supplied or Graph-created bot identity (`bot_app_identity`), Bot
//! Framework endpoint registration, Teams app publish (in-component zip → Graph
//! app catalog) and per-user install, and a runtime observation of the first
//! inbound activity (`first_bot_framework_post`).

use crate::bindings::greentic::http::http_client as client;
use crate::bindings::greentic::state::state_store;
use serde_json::{Map, Value, json};
use urlencoding::encode;

/// Microsoft Graph Command Line Tools public client (device-code capable).
const GRAPH_CLIENT_ID_DEFAULT: &str = "14d82eec-204b-4c2f-b7e8-296a70dab67e";
const GRAPH_SCOPES: &str = "https://graph.microsoft.com/Application.ReadWrite.All https://graph.microsoft.com/AppCatalog.ReadWrite.All https://graph.microsoft.com/TeamsAppInstallation.ReadWriteForUser https://graph.microsoft.com/User.Read offline_access";

const STEP_IDS: [&str; 6] = [
    "graph_admin_consent",
    "bot_app_identity",
    "bot_framework_endpoint_registration",
    "teams_app_publish",
    "teams_app_user_install",
    "first_bot_framework_post",
];

fn step_label(id: &str) -> &'static str {
    match id {
        "graph_admin_consent" => "Authorize Microsoft Graph setup access",
        "bot_app_identity" => "Create or reuse the Bot Framework app identity",
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
                "public_base_url": "https://runtime.example.test",
                "bot_framework_registration_url": "https://runtime.example.test/v1/setup/bot-framework/registration"
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
    if !is_done(state, id) {
        if let Some(arr) = state.get_mut("done").and_then(Value::as_array_mut) {
            arr.push(Value::String(id.to_string()));
        }
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
        // bot_framework_endpoint_registration: POST to the registration service if configured.
        2 => bot_framework_registration_step(state),
        // teams_app_publish: build + upload the real Teams app package to the catalog.
        3 => teams_publish_step(state),
        // teams_app_user_install: install the published app for the signed-in user.
        4 => teams_install_step(state),
        // first_bot_framework_post: do NOT self-complete — it resolves when a real
        // inbound activity is observed (see record_activity + the GET handler).
        5 => "send a message to the bot in Teams to finish setup",
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
    let values = state.get("values");
    // Surface the real Teams deep links produced by the publish/install steps.
    let add_to_teams_url = values
        .and_then(|v| v.get("last_teams_app_publish"))
        .and_then(|p| p.get("add_to_teams_url"))
        .cloned()
        .unwrap_or(Value::Null);
    let open_bot_chat_url = values
        .and_then(|v| v.get("last_teams_app_install"))
        .and_then(|i| i.get("open_bot_chat_url"))
        .cloned()
        .unwrap_or(Value::Null);

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
            "add_to_teams_url": add_to_teams_url,
            "open_bot_chat_url": open_bot_chat_url,
        },
        "values": state.get("values").cloned().unwrap_or_else(|| json!({})),
    })
}

// ── graph_admin_consent: Microsoft device-code OAuth ────────────────────────

fn cfg_str(state: &Value, key: &str) -> String {
    state
        .get("values")
        .and_then(|v| v.get("config"))
        .and_then(|c| c.get(key))
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
                json!({ "displayName": display_name, "signInAudience": "AzureADMultipleOrgs" });
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
    let package = crate::teams_pkg::build_package(&bot_app_id, &bot_app_id, &app_name);
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
    config_mut(state).insert("teams_app_id".into(), json!(catalog_id));
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
    {
        if !uid.is_empty() {
            return Some(uid.to_string());
        }
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

fn http_post_json(url: &str, body: &Value) -> Result<(u16, Value), String> {
    let bytes = serde_json::to_vec(body).map_err(|e| e.to_string())?;
    let request = client::Request {
        method: "POST".into(),
        url: url.to_string(),
        headers: vec![("Content-Type".into(), "application/json".into())],
        body: Some(bytes),
    };
    let resp = client::send(&request, None, None)
        .map_err(|e| format!("transport error: {}", e.message))?;
    let status = resp.status as u16;
    let b = resp.body.unwrap_or_default();
    let json = if b.is_empty() {
        Value::Null
    } else {
        serde_json::from_slice(&b).unwrap_or(Value::Null)
    };
    Ok((status, json))
}

/// Register the messaging endpoint with the Bot Framework service if a registration
/// URL is configured; otherwise treat it as registered manually in Azure Bot Service.
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
    let reg_url = cfg_str(state, "bot_framework_registration_url");
    // Empty or the placeholder default → manual mode: don't POST anywhere, just report
    // the endpoint the operator should set as their Azure Bot messaging endpoint.
    let is_manual = reg_url.trim().is_empty()
        || reg_url.contains("example.test")
        || reg_url.contains("example.com");
    if is_manual {
        let result = json!({ "ok": true, "action": "manual", "target_messaging_endpoint": messaging_endpoint, "note": "Set this URL as your Azure Bot Service messaging endpoint. (No registration service configured.)" });
        mark_done(state, "bot_framework_endpoint_registration");
        set_result(
            state,
            "bot_framework_endpoint_registration",
            true,
            result,
            "click Continue to continue setup",
        );
        return "click Continue to continue setup";
    }
    let body = json!({
        "provider_id": "messaging-teams",
        "bot_app_id": cfg_str(state, "bot_app_id"),
        "bot_app_password": cfg_str(state, "bot_app_password"),
        "messaging_endpoint": messaging_endpoint,
        "channel": "msteams",
        "tenant": tenant,
        "team": team,
    });
    match http_post_json(&reg_url, &body) {
        Ok((s, _)) if s < 300 => {
            let result = json!({ "ok": true, "action": "update", "target_messaging_endpoint": messaging_endpoint });
            mark_done(state, "bot_framework_endpoint_registration");
            set_result(
                state,
                "bot_framework_endpoint_registration",
                true,
                result,
                "click Continue to continue setup",
            );
            "click Continue to continue setup"
        }
        Ok((status, body)) => step_fail(
            state,
            "bot_framework_endpoint_registration",
            &format!(
                "Bot Framework registration failed (HTTP {status}): {}",
                graph_error_message(&body, "unknown error")
            ),
        ),
        Err(e) => step_fail(state, "bot_framework_endpoint_registration", &e),
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
            let next = if a.ends_with("/start") {
                graph_start(&mut state)
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
