//! Setup-time Teams channel provisioning through Microsoft Graph.

use provider_common::helpers::json_bytes;
use serde_json::{Value, json};

use crate::auth::acquire_graph_token;
use crate::bindings::greentic::http::http_client as client;
use crate::config::{ProviderConfig, ProviderConfigOut, load_config};

pub(crate) fn ensure_channel(input_json: &[u8]) -> Vec<u8> {
    let parsed: Value = match serde_json::from_slice(input_json) {
        Ok(val) => val,
        Err(err) => {
            return json_bytes(&json!({"ok": false, "error": format!("invalid json: {err}")}));
        }
    };

    let cfg = match load_config(&parsed) {
        Ok(cfg) => cfg,
        Err(err) => return json_bytes(&json!({"ok": false, "error": err})),
    };
    if !cfg.enabled {
        return json_bytes(&json!({"ok": false, "error": "provider disabled by config"}));
    }

    let team_id = match string_field(&parsed, "team_id").or_else(|| cfg.team_id.clone()) {
        Some(value) => value,
        None => return json_bytes(&json!({"ok": false, "error": "team_id required"})),
    };
    let desired_name = match string_field(&parsed, "desired_channel_name")
        .or_else(|| string_field(&parsed, "channel_name"))
        .or_else(|| cfg.desired_channel_name.clone())
        .or_else(|| cfg.channel_name.clone())
    {
        Some(value) => value,
        None => {
            if let Some(channel_id) = cfg.channel_id {
                return json_bytes(&json!({
                    "ok": true,
                    "created": false,
                    "team_id": team_id,
                    "channel_id": channel_id,
                    "channel_name": cfg.channel_name,
                    "reason": "channel_id already configured and no desired_channel_name provided"
                }));
            }
            return json_bytes(&json!({"ok": false, "error": "desired_channel_name required"}));
        }
    };

    let token = match acquire_graph_token(&cfg) {
        Ok(token) => token,
        Err(err) => return json_bytes(&json!({"ok": false, "error": err})),
    };

    match ensure_channel_with_token(&cfg, &token, &team_id, &desired_name) {
        Ok(result) => json_bytes(&json!({
            "ok": true,
            "created": result.created,
            "team_id": team_id,
            "channel_id": result.channel_id,
            "channel_name": result.channel_name,
        })),
        Err(err) => json_bytes(&json!({"ok": false, "error": err})),
    }
}

struct EnsureChannelResult {
    created: bool,
    channel_id: String,
    channel_name: String,
}

pub(crate) fn maybe_ensure_channel_config(config: &mut ProviderConfigOut) -> Result<bool, String> {
    let Some(team_id) = config
        .team_id
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
    else {
        return Ok(false);
    };
    let Some(desired_name) = config
        .desired_channel_name
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
    else {
        return Ok(false);
    };
    if config
        .channel_name
        .as_deref()
        .is_some_and(|name| name.trim().eq_ignore_ascii_case(&desired_name))
    {
        return Ok(false);
    }

    let provider_config = ProviderConfig {
        enabled: config.enabled,
        public_base_url: config.public_base_url.clone(),
        setup_mode: config.setup_mode.clone(),
        tenant_id: config.tenant_id.clone(),
        client_id: config.client_id.clone(),
        refresh_token: config.refresh_token.clone(),
        client_secret: config.client_secret.clone(),
        access_token: config.access_token.clone(),
        graph_base_url: config.graph_base_url.clone(),
        auth_base_url: config.auth_base_url.clone(),
        token_scope: config.token_scope.clone(),
        team_id: config.team_id.clone(),
        team_name: config.team_name.clone(),
        channel_id: config.channel_id.clone(),
        channel_name: config.channel_name.clone(),
        desired_channel_name: config.desired_channel_name.clone(),
        chat_id: config.chat_id.clone(),
        user_id: config.user_id.clone(),
        ms_bot_app_id: config.ms_bot_app_id.clone(),
        ms_bot_app_password: config.ms_bot_app_password.clone(),
        bot_display_name: config.bot_display_name.clone(),
        messaging_endpoint: config.messaging_endpoint.clone(),
        default_service_url: None,
    };
    let token = acquire_graph_token(&provider_config)?;
    let result = ensure_channel_with_token(&provider_config, &token, &team_id, &desired_name)?;
    config.channel_id = Some(result.channel_id);
    config.channel_name = Some(result.channel_name);
    Ok(result.created)
}

fn ensure_channel_with_token(
    cfg: &ProviderConfig,
    token: &str,
    team_id: &str,
    desired_name: &str,
) -> Result<EnsureChannelResult, String> {
    let list_url = channels_url(cfg, team_id);
    let channels = graph_get_json(&list_url, token)?;
    if let Some((channel_id, channel_name)) = find_channel_by_name(&channels, desired_name) {
        return Ok(EnsureChannelResult {
            created: false,
            channel_id,
            channel_name,
        });
    }

    let body = channel_create_body(desired_name);
    let created = graph_post_json(&list_url, token, &body)?;
    let channel_id = created
        .get("id")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| "Graph channel create response missing id".to_string())?;
    let channel_name = created
        .get("displayName")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or(desired_name);

    Ok(EnsureChannelResult {
        created: true,
        channel_id: channel_id.to_string(),
        channel_name: channel_name.to_string(),
    })
}

fn channels_url(cfg: &ProviderConfig, team_id: &str) -> String {
    format!(
        "{}/teams/{}/channels",
        cfg.graph_base_url.trim_end_matches('/'),
        team_id
    )
}

fn channel_create_body(display_name: &str) -> Value {
    json!({
        "displayName": display_name,
        "description": format!("Created by Greentic setup for {display_name}"),
        "membershipType": "standard"
    })
}

fn find_channel_by_name(channels: &Value, desired_name: &str) -> Option<(String, String)> {
    let desired_name = desired_name.trim();
    if desired_name.is_empty() {
        return None;
    }
    channels
        .get("value")
        .and_then(Value::as_array)?
        .iter()
        .find_map(|channel| {
            let name = channel
                .get("displayName")
                .and_then(Value::as_str)
                .map(str::trim)
                .filter(|value| !value.is_empty())?;
            if !name.eq_ignore_ascii_case(desired_name) {
                return None;
            }
            let id = channel
                .get("id")
                .and_then(Value::as_str)
                .map(str::trim)
                .filter(|value| !value.is_empty())?;
            Some((id.to_string(), name.to_string()))
        })
}

fn graph_get_json(url: &str, token: &str) -> Result<Value, String> {
    let request = client::Request {
        method: "GET".into(),
        url: url.to_string(),
        headers: vec![("Authorization".into(), format!("Bearer {token}"))],
        body: None,
    };
    graph_json_request(request)
}

fn graph_post_json(url: &str, token: &str, body: &Value) -> Result<Value, String> {
    let request = client::Request {
        method: "POST".into(),
        url: url.to_string(),
        headers: vec![
            ("Content-Type".into(), "application/json".into()),
            ("Authorization".into(), format!("Bearer {token}")),
        ],
        body: Some(serde_json::to_vec(body).unwrap_or_else(|_| b"{}".to_vec())),
    };
    graph_json_request(request)
}

fn graph_json_request(request: client::Request) -> Result<Value, String> {
    let resp = http_send(&request).map_err(|err| format!("transport error: {}", err.message))?;
    let body = resp.body.unwrap_or_default();
    let body_json = serde_json::from_slice::<Value>(&body).unwrap_or(Value::Null);
    if resp.status < 200 || resp.status >= 300 {
        let detail = body_json
            .get("error")
            .and_then(|err| err.get("message"))
            .and_then(Value::as_str)
            .unwrap_or_default();
        let prefix = match resp.status {
            401 => "expired or invalid Graph token",
            403 => {
                "missing Graph permission or admin consent; Channel.Create is required to create Teams channels"
            }
            404 => "bad Teams team id",
            409 => "Teams channel already exists",
            429 => "Graph rate limit exceeded",
            _ => "Graph request failed",
        };
        if detail.is_empty() {
            return Err(format!("{prefix} (status {})", resp.status));
        }
        return Err(format!("{prefix} (status {}): {detail}", resp.status));
    }
    Ok(body_json)
}

fn http_send(request: &client::Request) -> Result<client::Response, client::HostError> {
    #[cfg(test)]
    {
        http_send_test(request)
    }
    #[cfg(not(test))]
    {
        client::send(request, None, None)
    }
}

#[cfg(test)]
type HttpSendMock = dyn Fn(&client::Request) -> Result<client::Response, client::HostError>;

#[cfg(test)]
thread_local! {
    static HTTP_SEND_MOCK: std::cell::RefCell<Option<Box<HttpSendMock>>> =
        std::cell::RefCell::new(None);
}

#[cfg(test)]
pub(crate) fn with_http_send_mock<F, R>(
    mock: impl Fn(&client::Request) -> Result<client::Response, client::HostError> + 'static,
    f: F,
) -> R
where
    F: FnOnce() -> R,
{
    HTTP_SEND_MOCK.with(|cell| *cell.borrow_mut() = Some(Box::new(mock)));
    let out = f();
    HTTP_SEND_MOCK.with(|cell| *cell.borrow_mut() = None);
    out
}

#[cfg(test)]
fn http_send_test(request: &client::Request) -> Result<client::Response, client::HostError> {
    HTTP_SEND_MOCK.with(|cell| match &*cell.borrow() {
        Some(mock) => mock(request),
        None => Err(client::HostError {
            code: "unconfigured".into(),
            message: "http_send_test mock not set".into(),
        }),
    })
}

fn string_field(value: &Value, key: &str) -> Option<String> {
    value
        .get(key)
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cfg() -> ProviderConfig {
        ProviderConfig {
            enabled: true,
            public_base_url: None,
            setup_mode: Some("graph_channel".to_string()),
            tenant_id: "tenant".to_string(),
            client_id: "client".to_string(),
            refresh_token: Some("refresh".to_string()),
            client_secret: None,
            access_token: Some("token".to_string()),
            graph_base_url: "https://graph.microsoft.com/v1.0/".to_string(),
            auth_base_url: "https://login.microsoftonline.com".to_string(),
            token_scope: "https://graph.microsoft.com/.default".to_string(),
            team_id: Some("team".to_string()),
            team_name: Some("Greentic AI Ltd".to_string()),
            channel_id: Some("general".to_string()),
            channel_name: Some("General".to_string()),
            desired_channel_name: Some("hr onboarding".to_string()),
            chat_id: None,
            user_id: None,
            ms_bot_app_id: None,
            ms_bot_app_password: None,
            bot_display_name: None,
            messaging_endpoint: None,
            default_service_url: None,
        }
    }

    #[test]
    fn channel_url_uses_team_id() {
        assert_eq!(
            channels_url(&cfg(), "team-id"),
            "https://graph.microsoft.com/v1.0/teams/team-id/channels"
        );
    }

    #[test]
    fn create_body_requests_standard_channel() {
        assert_eq!(
            channel_create_body("hr onboarding"),
            json!({
                "displayName": "hr onboarding",
                "description": "Created by Greentic setup for hr onboarding",
                "membershipType": "standard"
            })
        );
    }

    #[test]
    fn exact_existing_channel_match_wins_before_create() {
        let channels = json!({
            "value": [
                {"id": "general", "displayName": "General"},
                {"id": "hr", "displayName": "HR Onboarding"}
            ]
        });

        assert_eq!(
            find_channel_by_name(&channels, "hr onboarding"),
            Some(("hr".to_string(), "HR Onboarding".to_string()))
        );
    }

    #[test]
    fn channel_name_is_not_unique_identifier() {
        let channels = json!({
            "value": [
                {"id": "general", "displayName": "General"}
            ]
        });

        assert_eq!(find_channel_by_name(&channels, "hr onboarding"), None);
    }
}
