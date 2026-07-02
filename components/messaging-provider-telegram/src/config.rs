use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::DEFAULT_API_BASE;

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct ProviderConfig {
    #[serde(default = "default_enabled")]
    pub(crate) enabled: bool,
    // Default so a per-field override (e.g. just `api_base_url`) deserializes; egress never reads it.
    #[serde(default)]
    pub(crate) public_base_url: String,
    #[serde(default)]
    pub(crate) default_chat_id: Option<String>,
    #[serde(default)]
    pub(crate) api_base_url: Option<String>,
    #[serde(default, alias = "telegram_bot_token")]
    pub(crate) bot_token: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct ProviderConfigOut {
    pub(crate) enabled: bool,
    pub(crate) public_base_url: String,
    pub(crate) default_chat_id: Option<String>,
    pub(crate) api_base_url: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) bot_token: Option<String>,
}

fn default_enabled() -> bool {
    true
}

pub(crate) fn default_config_out() -> ProviderConfigOut {
    ProviderConfigOut {
        enabled: true,
        public_base_url: String::new(),
        default_chat_id: None,
        api_base_url: DEFAULT_API_BASE.to_string(),
        bot_token: None,
    }
}

pub(crate) fn validate_config_out(config: &ProviderConfigOut) -> Result<(), String> {
    if config.public_base_url.trim().is_empty() {
        return Err("invalid config: public_base_url cannot be empty".to_string());
    }
    if !(config.public_base_url.starts_with("http://")
        || config.public_base_url.starts_with("https://"))
    {
        return Err("invalid config: public_base_url must be an absolute URL".to_string());
    }
    if config.api_base_url.trim().is_empty() {
        return Err("invalid config: api_base_url cannot be empty".to_string());
    }
    if !(config.api_base_url.starts_with("http://") || config.api_base_url.starts_with("https://"))
    {
        return Err("invalid config: api_base_url must be an absolute URL".to_string());
    }
    Ok(())
}

pub(crate) fn parse_config_value(val: &Value) -> Result<ProviderConfig, String> {
    serde_json::from_value::<ProviderConfig>(val.clone())
        .map_err(|e| format!("invalid config: {e}"))
}

#[cfg(test)]
#[allow(dead_code)]
pub(crate) fn parse_config_bytes(bytes: &[u8]) -> Result<ProviderConfig, String> {
    serde_json::from_slice::<ProviderConfig>(bytes).map_err(|e| format!("invalid config: {e}"))
}

pub(crate) fn load_config(input: &Value) -> Result<ProviderConfig, String> {
    if let Some(cfg) = input.get("config") {
        return parse_config_value(cfg);
    }
    let mut partial = serde_json::Map::new();
    if let Some(v) = input.get("enabled") {
        partial.insert("enabled".into(), v.clone());
    }
    if let Some(v) = input.get("public_base_url") {
        partial.insert("public_base_url".into(), v.clone());
    }
    if let Some(v) = input.get("default_chat_id") {
        partial.insert("default_chat_id".into(), v.clone());
    }
    if let Some(v) = input.get("api_base_url") {
        partial.insert("api_base_url".into(), v.clone());
    }
    if let Some(v) = input.get("bot_token") {
        partial.insert("bot_token".into(), v.clone());
    }
    if let Some(v) = input.get("telegram_bot_token") {
        partial.insert("bot_token".into(), v.clone());
    }
    if !partial.is_empty() {
        return parse_config_value(&Value::Object(partial));
    }

    Ok(ProviderConfig {
        enabled: true,
        public_base_url: "https://invalid.local".to_string(),
        default_chat_id: None,
        api_base_url: Some(DEFAULT_API_BASE.to_string()),
        bot_token: None,
    })
}

pub(crate) fn get_bot_token(cfg: &ProviderConfig) -> Result<String, String> {
    use crate::TOKEN_SECRET;
    use crate::bindings::greentic::secrets_store::secrets_store;

    if let Some(token) = cfg.bot_token.clone() {
        let token = token.trim().to_string();
        if !token.is_empty() {
            return Ok(token);
        }
    }
    match secrets_store::get(TOKEN_SECRET) {
        Ok(Some(bytes)) => String::from_utf8(bytes).map_err(|_| "bot token not utf-8".to_string()),
        Ok(None) => Err(format!(
            "missing bot_token (config or secret: {TOKEN_SECRET})"
        )),
        Err(e) => Err(format!("secret store error: {e:?}")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn valid_config_out() -> ProviderConfigOut {
        ProviderConfigOut {
            enabled: true,
            public_base_url: "https://example.com".to_string(),
            default_chat_id: Some("1234".to_string()),
            api_base_url: "https://api.telegram.org".to_string(),
            bot_token: Some("token".to_string()),
        }
    }

    #[test]
    fn validate_config_out_catches_bad_urls() {
        let mut config = valid_config_out();
        config.public_base_url = "   ".to_string();
        assert_eq!(
            validate_config_out(&config),
            Err("invalid config: public_base_url cannot be empty".to_string())
        );

        let mut config = valid_config_out();
        config.public_base_url = "relative".to_string();
        assert_eq!(
            validate_config_out(&config),
            Err("invalid config: public_base_url must be an absolute URL".to_string())
        );

        let mut config = valid_config_out();
        config.api_base_url = "relative".to_string();
        assert_eq!(
            validate_config_out(&config),
            Err("invalid config: api_base_url must be an absolute URL".to_string())
        );
    }

    #[test]
    fn load_config_uses_nested_top_level_or_defaults() {
        let nested = load_config(&json!({
            "config": {
                "public_base_url": "https://example.com",
                "default_chat_id": "nested"
            }
        }))
        .expect("nested config");
        assert_eq!(nested.default_chat_id.as_deref(), Some("nested"));

        let top_level = load_config(&json!({
            "public_base_url": "https://example.com",
            "bot_token": "top-level"
        }))
        .expect("top-level config");
        assert_eq!(top_level.bot_token.as_deref(), Some("top-level"));

        let defaulted = load_config(&json!({})).expect("default config");
        assert_eq!(defaulted.public_base_url, "https://invalid.local");
        assert_eq!(defaulted.api_base_url.as_deref(), Some(DEFAULT_API_BASE));
    }

    #[test]
    fn load_config_accepts_api_base_url_override_without_public_base_url() {
        let cfg = load_config(&json!({
            "config": { "api_base_url": "http://sink.local" }
        }))
        .expect("a per-field api_base_url override must parse on its own");
        assert_eq!(cfg.api_base_url.as_deref(), Some("http://sink.local"));
        assert_eq!(cfg.public_base_url, "");
    }
}
