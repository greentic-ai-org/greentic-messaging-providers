//! Configuration for the Microsoft Teams Graph provider.

use serde::{Deserialize, Serialize};
use serde_json::Value;

#[cfg(not(test))]
use crate::bindings::greentic::secrets_store::secrets_store;
use crate::{
    DEFAULT_AUTH_BASE_URL, DEFAULT_BOT_APP_ID_KEY, DEFAULT_BOT_APP_PASSWORD_KEY,
    DEFAULT_GRAPH_ACCESS_TOKEN_KEY, DEFAULT_GRAPH_BASE_URL, DEFAULT_GRAPH_CLIENT_ID_KEY,
    DEFAULT_GRAPH_CLIENT_SECRET_KEY, DEFAULT_GRAPH_REFRESH_TOKEN_KEY, DEFAULT_GRAPH_TENANT_ID_KEY,
    DEFAULT_GRAPH_TOKEN_SCOPE,
};
use greentic_types::Destination;

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
#[allow(dead_code)]
pub(crate) struct ProviderConfig {
    #[serde(default = "default_enabled")]
    pub(crate) enabled: bool,

    #[serde(default)]
    pub(crate) public_base_url: Option<String>,

    #[serde(default)]
    pub(crate) setup_mode: Option<String>,

    #[serde(default)]
    pub(crate) tenant_id: String,
    #[serde(default)]
    pub(crate) client_id: String,

    #[serde(default)]
    pub(crate) refresh_token: Option<String>,
    #[serde(default)]
    pub(crate) client_secret: Option<String>,
    #[serde(default)]
    pub(crate) access_token: Option<String>,

    #[serde(default = "default_graph_base_url")]
    pub(crate) graph_base_url: String,
    #[serde(default = "default_auth_base_url")]
    pub(crate) auth_base_url: String,
    #[serde(default = "default_token_scope")]
    pub(crate) token_scope: String,

    #[serde(default)]
    pub(crate) team_id: Option<String>,
    #[serde(default)]
    pub(crate) team_name: Option<String>,
    #[serde(default)]
    pub(crate) channel_id: Option<String>,
    #[serde(default)]
    pub(crate) channel_name: Option<String>,
    #[serde(default)]
    pub(crate) desired_channel_name: Option<String>,
    #[serde(default)]
    pub(crate) chat_id: Option<String>,
    #[serde(default)]
    pub(crate) user_id: Option<String>,

    #[serde(default)]
    pub(crate) ms_bot_app_id: Option<String>,
    #[serde(default)]
    pub(crate) ms_bot_app_password: Option<String>,
    #[serde(default)]
    pub(crate) bot_display_name: Option<String>,
    #[serde(default)]
    pub(crate) messaging_endpoint: Option<String>,
    #[serde(default)]
    pub(crate) default_service_url: Option<String>,

    /// Dev-only escape hatch: when `true`, JWT validation errors are logged
    /// but the request is allowed through. Default: `false` (strict).
    #[serde(default)]
    pub(crate) skip_jwt_validation: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct ProviderConfigOut {
    pub(crate) enabled: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) public_base_url: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) setup_mode: Option<String>,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub(crate) tenant_id: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub(crate) client_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) refresh_token: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) client_secret: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) access_token: Option<String>,
    pub(crate) graph_base_url: String,
    pub(crate) auth_base_url: String,
    pub(crate) token_scope: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) team_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) team_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) channel_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) channel_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) desired_channel_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) chat_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) user_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) ms_bot_app_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) ms_bot_app_password: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) bot_display_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) messaging_endpoint: Option<String>,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) skip_jwt_validation: Option<bool>,
}

fn default_enabled() -> bool {
    true
}

fn default_graph_base_url() -> String {
    DEFAULT_GRAPH_BASE_URL.to_string()
}

fn default_auth_base_url() -> String {
    DEFAULT_AUTH_BASE_URL.to_string()
}

fn default_token_scope() -> String {
    DEFAULT_GRAPH_TOKEN_SCOPE.to_string()
}

pub(crate) fn default_config_out() -> ProviderConfigOut {
    ProviderConfigOut {
        enabled: true,
        public_base_url: None,
        setup_mode: None,
        tenant_id: String::new(),
        client_id: String::new(),
        refresh_token: None,
        client_secret: None,
        access_token: None,
        graph_base_url: default_graph_base_url(),
        auth_base_url: default_auth_base_url(),
        token_scope: default_token_scope(),
        team_id: None,
        team_name: None,
        channel_id: None,
        channel_name: None,
        desired_channel_name: None,
        chat_id: None,
        user_id: None,
        ms_bot_app_id: None,
        ms_bot_app_password: None,
        bot_display_name: None,
        messaging_endpoint: None,
        skip_jwt_validation: None,
    }
}

pub(crate) fn validate_config_out(config: &ProviderConfigOut) -> Result<(), String> {
    if config.setup_mode.as_deref() == Some("bot_framework") {
        if config
            .ms_bot_app_id
            .as_deref()
            .map(str::trim)
            .unwrap_or_default()
            .is_empty()
        {
            return Err("config validation failed: ms_bot_app_id is required".to_string());
        }
    } else {
        if config.tenant_id.trim().is_empty() {
            return Err("config validation failed: tenant_id is required".to_string());
        }
        if config.client_id.trim().is_empty() {
            return Err("config validation failed: client_id is required".to_string());
        }
    }
    if let Some(url) = config.public_base_url.as_deref()
        && !url.trim().is_empty()
        && !(url.starts_with("http://") || url.starts_with("https://"))
    {
        return Err(
            "config validation failed: public_base_url must be an absolute URL".to_string(),
        );
    }
    validate_absolute_url("graph_base_url", &config.graph_base_url)?;
    validate_absolute_url("auth_base_url", &config.auth_base_url)?;
    if config.token_scope.trim().is_empty() {
        return Err("config validation failed: token_scope is required".to_string());
    }
    Ok(())
}

pub(crate) fn validate_provider_config(mut cfg: ProviderConfig) -> Result<ProviderConfig, String> {
    cfg.setup_mode = normalize_optional_string(cfg.setup_mode);
    cfg.tenant_id = cfg.tenant_id.trim().to_string();
    cfg.client_id = cfg.client_id.trim().to_string();
    cfg.graph_base_url = cfg.graph_base_url.trim_end_matches('/').to_string();
    cfg.auth_base_url = cfg.auth_base_url.trim_end_matches('/').to_string();
    cfg.token_scope = cfg.token_scope.trim().to_string();
    cfg.team_name = normalize_optional_string(cfg.team_name);
    cfg.channel_name = normalize_optional_string(cfg.channel_name);
    cfg.desired_channel_name = normalize_optional_string(cfg.desired_channel_name);
    cfg.ms_bot_app_id = normalize_optional_string(cfg.ms_bot_app_id);
    cfg.ms_bot_app_password = normalize_optional_string(cfg.ms_bot_app_password);
    cfg.bot_display_name = normalize_optional_string(cfg.bot_display_name);
    cfg.messaging_endpoint = normalize_optional_string(cfg.messaging_endpoint);
    cfg.default_service_url = normalize_optional_string(cfg.default_service_url);

    if cfg.setup_mode.as_deref() == Some("bot_framework") {
        if cfg.ms_bot_app_id.as_deref().unwrap_or_default().is_empty() {
            return Err("invalid config: ms_bot_app_id cannot be empty".to_string());
        }
    } else {
        if cfg.tenant_id.is_empty() {
            return Err("invalid config: tenant_id cannot be empty".to_string());
        }
        if cfg.client_id.is_empty() {
            return Err("invalid config: client_id cannot be empty".to_string());
        }
    }
    validate_absolute_url("graph_base_url", &cfg.graph_base_url)
        .map_err(|err| err.replace("config validation failed", "invalid config"))?;
    validate_absolute_url("auth_base_url", &cfg.auth_base_url)
        .map_err(|err| err.replace("config validation failed", "invalid config"))?;
    if cfg.token_scope.is_empty() {
        return Err("invalid config: token_scope cannot be empty".to_string());
    }
    Ok(cfg)
}

fn normalize_optional_string(value: Option<String>) -> Option<String> {
    value
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
}

fn validate_absolute_url(field: &str, value: &str) -> Result<(), String> {
    if value.trim().is_empty() || !(value.starts_with("http://") || value.starts_with("https://")) {
        return Err(format!(
            "config validation failed: {field} must be an absolute URL"
        ));
    }
    Ok(())
}

#[cfg(test)]
pub(crate) fn parse_config_bytes(bytes: &[u8]) -> Result<ProviderConfig, String> {
    let cfg = serde_json::from_slice::<ProviderConfig>(bytes)
        .map_err(|e| format!("invalid config: {e}"))?;
    validate_provider_config(cfg)
}

pub(crate) fn parse_config_value(val: &Value) -> Result<ProviderConfig, String> {
    let cfg = serde_json::from_value::<ProviderConfig>(normalize_config_value(val))
        .map_err(|e| format!("invalid config: {e}"))?;
    validate_provider_config(cfg)
}

fn normalize_config_value(val: &Value) -> Value {
    let mut normalized = val.clone();
    let Some(obj) = normalized.as_object_mut() else {
        return normalized;
    };

    for (field, secret_key) in [
        ("tenant_id", DEFAULT_GRAPH_TENANT_ID_KEY),
        ("client_id", DEFAULT_GRAPH_CLIENT_ID_KEY),
        ("refresh_token", DEFAULT_GRAPH_REFRESH_TOKEN_KEY),
        ("access_token", DEFAULT_GRAPH_ACCESS_TOKEN_KEY),
        ("client_secret", DEFAULT_GRAPH_CLIENT_SECRET_KEY),
    ] {
        let missing_or_blank = obj
            .get(field)
            .and_then(Value::as_str)
            .map(str::trim)
            .map(str::is_empty)
            .unwrap_or(true);
        if missing_or_blank && let Some(value) = optional_secret(secret_key) {
            obj.insert(field.to_string(), Value::String(value));
        }
    }

    normalized
}

pub(crate) fn load_config(input: &Value) -> Result<ProviderConfig, String> {
    if let Some(cfg) = input.get("config") {
        return parse_config_value(cfg);
    }

    let mut partial = serde_json::Map::new();
    for key in [
        "enabled",
        "public_base_url",
        "setup_mode",
        "tenant_id",
        "client_id",
        "refresh_token",
        "client_secret",
        "access_token",
        "graph_base_url",
        "auth_base_url",
        "token_scope",
        "team_id",
        "team_name",
        "channel_id",
        "channel_name",
        "desired_channel_name",
        "chat_id",
        "user_id",
        "ms_bot_app_id",
        "ms_bot_app_password",
        "bot_display_name",
        "messaging_endpoint",
        "default_service_url",
        "skip_jwt_validation",
    ] {
        if let Some(v) = input.get(key) {
            partial.insert(key.to_string(), v.clone());
        }
    }
    if !partial.is_empty() {
        return parse_config_value(&Value::Object(partial));
    }

    load_config_from_secrets()
}

pub(crate) fn get_secret(key: &str) -> Result<String, String> {
    #[cfg(test)]
    {
        Err(format!("missing secret: {key}"))
    }
    #[cfg(not(test))]
    match secrets_store::get(key) {
        Ok(Some(bytes)) => String::from_utf8(bytes).map_err(|_| format!("secret {key} not utf-8")),
        Ok(None) => Err(format!("missing secret: {key}")),
        Err(e) => Err(format!("secret store error: {e:?}")),
    }
}

pub(crate) fn get_secret_any_case(uppercase: &str) -> Result<String, String> {
    get_secret(uppercase).or_else(|_| get_secret(&uppercase.to_ascii_lowercase()))
}

fn optional_secret(key: &str) -> Option<String> {
    get_secret_any_case(key)
        .ok()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
}

fn load_config_from_secrets() -> Result<ProviderConfig, String> {
    let tenant_id = get_secret_any_case(DEFAULT_GRAPH_TENANT_ID_KEY).map_err(|e| {
        format!(
            "config required: tenant_id not found (tried {} and {}): {e}",
            DEFAULT_GRAPH_TENANT_ID_KEY,
            DEFAULT_GRAPH_TENANT_ID_KEY.to_ascii_lowercase()
        )
    })?;
    let client_id = get_secret_any_case(DEFAULT_GRAPH_CLIENT_ID_KEY).map_err(|e| {
        format!(
            "config required: client_id not found (tried {} and {}): {e}",
            DEFAULT_GRAPH_CLIENT_ID_KEY,
            DEFAULT_GRAPH_CLIENT_ID_KEY.to_ascii_lowercase()
        )
    })?;

    Ok(ProviderConfig {
        enabled: true,
        public_base_url: None,
        setup_mode: None,
        tenant_id,
        client_id,
        refresh_token: optional_secret(DEFAULT_GRAPH_REFRESH_TOKEN_KEY),
        client_secret: optional_secret(DEFAULT_GRAPH_CLIENT_SECRET_KEY),
        access_token: optional_secret(DEFAULT_GRAPH_ACCESS_TOKEN_KEY),
        graph_base_url: default_graph_base_url(),
        auth_base_url: default_auth_base_url(),
        token_scope: default_token_scope(),
        team_id: None,
        team_name: None,
        channel_id: None,
        channel_name: None,
        desired_channel_name: None,
        chat_id: None,
        user_id: None,
        ms_bot_app_id: optional_secret(DEFAULT_BOT_APP_ID_KEY),
        ms_bot_app_password: optional_secret(DEFAULT_BOT_APP_PASSWORD_KEY),
        bot_display_name: None,
        messaging_endpoint: None,
        default_service_url: None,
        skip_jwt_validation: None,
    })
}

pub(crate) fn default_channel_destination(cfg: &ProviderConfig) -> Option<Destination> {
    let team = cfg.team_id.as_ref()?;
    let channel = cfg.channel_id.as_ref()?;
    let team = team.trim();
    let channel = channel.trim();
    if team.is_empty() || channel.is_empty() {
        return None;
    }
    Some(Destination {
        id: format!("{team}:{channel}"),
        kind: Some("channel".into()),
    })
}

pub(crate) fn default_chat_destination(cfg: &ProviderConfig) -> Option<Destination> {
    let chat = cfg.chat_id.as_ref()?.trim();
    if chat.is_empty() {
        return None;
    }
    Some(Destination {
        id: chat.to_string(),
        kind: Some("chat".into()),
    })
}

#[allow(dead_code)]
pub(crate) fn get_service_url(activity: &Value, cfg: &ProviderConfig) -> Option<String> {
    activity
        .get("serviceUrl")
        .and_then(Value::as_str)
        .filter(|s| !s.is_empty())
        .map(|s| s.to_string())
        .or_else(|| cfg.default_service_url.clone())
}

pub(crate) fn get_conversation_id(activity: &Value) -> Option<String> {
    activity
        .get("conversation")
        .and_then(|c| c.get("id"))
        .and_then(Value::as_str)
        .filter(|s| !s.is_empty())
        .map(|s| s.to_string())
}

pub(crate) fn get_activity_id(activity: &Value) -> Option<String> {
    activity
        .get("id")
        .and_then(Value::as_str)
        .filter(|s| !s.is_empty())
        .map(|s| s.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn valid_config_out() -> ProviderConfigOut {
        ProviderConfigOut {
            enabled: true,
            public_base_url: Some("https://example.com".to_string()),
            setup_mode: None,
            tenant_id: "tenant".to_string(),
            client_id: "client".to_string(),
            refresh_token: Some("refresh".to_string()),
            client_secret: None,
            access_token: None,
            graph_base_url: DEFAULT_GRAPH_BASE_URL.to_string(),
            auth_base_url: DEFAULT_AUTH_BASE_URL.to_string(),
            token_scope: DEFAULT_GRAPH_TOKEN_SCOPE.to_string(),
            team_id: Some("team-1".to_string()),
            team_name: Some("Team One".to_string()),
            channel_id: Some("channel-1".to_string()),
            channel_name: Some("General".to_string()),
            desired_channel_name: None,
            chat_id: Some("chat-1".to_string()),
            user_id: None,
            ms_bot_app_id: None,
            ms_bot_app_password: None,
            bot_display_name: None,
            messaging_endpoint: None,
            skip_jwt_validation: None,
        }
    }

    #[test]
    fn validate_config_out_rejects_missing_or_relative_values() {
        let mut config = valid_config_out();
        config.tenant_id = String::new();
        assert_eq!(
            validate_config_out(&config),
            Err("config validation failed: tenant_id is required".to_string())
        );

        let mut config = valid_config_out();
        config.public_base_url = Some("/relative".to_string());
        assert_eq!(
            validate_config_out(&config),
            Err("config validation failed: public_base_url must be an absolute URL".to_string())
        );
    }

    #[test]
    fn load_config_supports_nested_and_top_level_inputs() {
        let nested = load_config(&json!({
            "config": {
                "tenant_id": "tenant",
                "client_id": "nested-client",
                "refresh_token": "refresh"
            }
        }))
        .expect("nested config");
        assert_eq!(nested.client_id, "nested-client");

        let top_level = load_config(&json!({
            "tenant_id": "tenant",
            "client_id": "top-level-client",
            "refresh_token": "refresh",
            "team_id": "team-1",
            "team_name": "Team One",
            "channel_id": "channel-1",
            "channel_name": "General"
        }))
        .expect("top-level config");
        assert_eq!(top_level.team_id.as_deref(), Some("team-1"));
        assert_eq!(top_level.team_name.as_deref(), Some("Team One"));
        assert_eq!(top_level.channel_id.as_deref(), Some("channel-1"));
        assert_eq!(top_level.channel_name.as_deref(), Some("General"));
    }

    #[test]
    fn load_config_accepts_legacy_graph_config_without_labels() {
        let cfg = load_config(&json!({
            "tenant_id": "tenant",
            "client_id": "client",
            "refresh_token": "refresh",
            "team_id": "team-1",
            "channel_id": "channel-1"
        }))
        .expect("legacy graph config");

        assert_eq!(cfg.team_id.as_deref(), Some("team-1"));
        assert_eq!(cfg.team_name, None);
        assert_eq!(cfg.channel_id.as_deref(), Some("channel-1"));
        assert_eq!(cfg.channel_name, None);
    }

    #[test]
    fn load_config_accepts_bot_framework_shape() {
        let cfg = load_config(&json!({
            "setup_mode": "bot_framework",
            "ms_bot_app_id": "bot-app-id",
            "ms_bot_app_password": "bot-secret",
            "bot_display_name": "Greentic Bot",
            "messaging_endpoint": "https://example.com/api/messages"
        }))
        .expect("bot framework config");

        assert_eq!(cfg.setup_mode.as_deref(), Some("bot_framework"));
        assert_eq!(cfg.ms_bot_app_id.as_deref(), Some("bot-app-id"));
        assert_eq!(cfg.bot_display_name.as_deref(), Some("Greentic Bot"));
        assert_eq!(
            cfg.messaging_endpoint.as_deref(),
            Some("https://example.com/api/messages")
        );
    }

    #[test]
    fn helper_extractors_keep_legacy_activity_metadata() {
        let cfg = ProviderConfig {
            enabled: true,
            public_base_url: None,
            setup_mode: None,
            tenant_id: "tenant".to_string(),
            client_id: "client".to_string(),
            refresh_token: Some("refresh".to_string()),
            client_secret: None,
            access_token: None,
            graph_base_url: DEFAULT_GRAPH_BASE_URL.to_string(),
            auth_base_url: DEFAULT_AUTH_BASE_URL.to_string(),
            token_scope: DEFAULT_GRAPH_TOKEN_SCOPE.to_string(),
            team_id: Some(" team-1 ".to_string()),
            team_name: Some("Team One".to_string()),
            channel_id: Some(" channel-1 ".to_string()),
            channel_name: Some("General".to_string()),
            desired_channel_name: Some("General".to_string()),
            chat_id: Some(" chat-1 ".to_string()),
            user_id: None,
            ms_bot_app_id: Some("app-id".to_string()),
            ms_bot_app_password: None,
            bot_display_name: None,
            messaging_endpoint: None,
            default_service_url: Some("https://fallback.example.com".to_string()),
            skip_jwt_validation: None,
        };
        let destination = default_channel_destination(&cfg).expect("channel destination");
        assert_eq!(destination.id, "team-1:channel-1");
        let chat = default_chat_destination(&cfg).expect("chat destination");
        assert_eq!(chat.id, "chat-1");

        let activity = json!({
            "serviceUrl": "https://activity.example.com",
            "conversation": { "id": "conv-1" },
            "id": "activity-1"
        });
        assert_eq!(
            get_service_url(&activity, &cfg).as_deref(),
            Some("https://activity.example.com")
        );
        assert_eq!(get_conversation_id(&activity).as_deref(), Some("conv-1"));
        assert_eq!(get_activity_id(&activity).as_deref(), Some("activity-1"));
    }
}
