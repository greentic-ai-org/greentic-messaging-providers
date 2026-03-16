use crate::bindings::greentic::secrets_store::secrets_store;
use crate::{DEFAULT_API_BASE, DEFAULT_API_VERSION, DEFAULT_TOKEN_KEY};
use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct ProviderConfig {
    #[serde(default = "default_enabled")]
    pub(crate) enabled: bool,
    pub(crate) phone_number_id: String,
    pub(crate) public_base_url: String,
    #[serde(default)]
    pub(crate) business_account_id: Option<String>,
    #[serde(default)]
    pub(crate) api_base_url: Option<String>,
    #[serde(default)]
    pub(crate) api_version: Option<String>,
    #[serde(default)]
    pub(crate) token: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct ProviderConfigOut {
    pub(crate) enabled: bool,
    pub(crate) phone_number_id: String,
    pub(crate) public_base_url: String,
    pub(crate) business_account_id: Option<String>,
    pub(crate) api_base_url: String,
    pub(crate) api_version: String,
    pub(crate) token: Option<String>,
}

pub(crate) fn default_enabled() -> bool {
    true
}

pub(crate) fn default_config_out() -> ProviderConfigOut {
    ProviderConfigOut {
        enabled: true,
        phone_number_id: String::new(),
        public_base_url: String::new(),
        business_account_id: None,
        api_base_url: DEFAULT_API_BASE.to_string(),
        api_version: DEFAULT_API_VERSION.to_string(),
        token: None,
    }
}

pub(crate) fn validate_config_out(config: &ProviderConfigOut) -> Result<(), String> {
    if config.phone_number_id.trim().is_empty() {
        return Err("config validation failed: phone_number_id is required".to_string());
    }
    if config.public_base_url.trim().is_empty() {
        return Err("config validation failed: public_base_url is required".to_string());
    }
    if !(config.public_base_url.starts_with("http://")
        || config.public_base_url.starts_with("https://"))
    {
        return Err(
            "config validation failed: public_base_url must be an absolute URL".to_string(),
        );
    }
    if !(config.api_base_url.starts_with("http://") || config.api_base_url.starts_with("https://"))
    {
        return Err("config validation failed: api_base_url must be an absolute URL".to_string());
    }
    Ok(())
}

pub(crate) fn validate_provider_config(cfg: ProviderConfig) -> Result<ProviderConfig, String> {
    if cfg.phone_number_id.trim().is_empty() {
        return Err("invalid config: phone_number_id cannot be empty".to_string());
    }
    if cfg.public_base_url.trim().is_empty() {
        return Err("invalid config: public_base_url cannot be empty".to_string());
    }
    if let Some(business_account_id) = cfg.business_account_id.as_deref() {
        let _ = business_account_id.trim();
    }
    Ok(cfg)
}

pub(crate) fn parse_config_value(val: &Value) -> Result<ProviderConfig, String> {
    let cfg = serde_json::from_value::<ProviderConfig>(val.clone())
        .map_err(|e| format!("invalid config: {e}"))?;
    validate_provider_config(cfg)
}

pub(crate) fn load_config(input: &Value) -> Result<ProviderConfig, String> {
    if let Some(cfg) = input.get("config") {
        return parse_config_value(cfg);
    }
    let mut partial = serde_json::Map::new();
    for key in [
        "enabled",
        "phone_number_id",
        "public_base_url",
        "business_account_id",
        "api_base_url",
        "api_version",
        "token",
    ] {
        if let Some(v) = input.get(key) {
            partial.insert(key.to_string(), v.clone());
        }
    }
    if !partial.is_empty() {
        return parse_config_value(&Value::Object(partial));
    }

    Err("config required".into())
}

pub(crate) fn get_token(cfg: &ProviderConfig) -> Result<String, String> {
    if let Some(token) = cfg.token.clone() {
        let token = token.trim().to_string();
        if !token.is_empty() {
            return Ok(token);
        }
    }
    match secrets_store::get(DEFAULT_TOKEN_KEY) {
        Ok(Some(bytes)) => {
            String::from_utf8(bytes).map_err(|_| "access_token not utf-8".to_string())
        }
        Ok(None) => Err(format!(
            "missing token (config or secret: {})",
            DEFAULT_TOKEN_KEY
        )),
        Err(e) => Err(format!("secret store error: {e:?}")),
    }
}

#[cfg(test)]
pub(crate) fn parse_config_bytes(bytes: &[u8]) -> Result<ProviderConfig, String> {
    let cfg = serde_json::from_slice::<ProviderConfig>(bytes)
        .map_err(|e| format!("invalid config: {e}"))?;
    validate_provider_config(cfg)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn valid_config_out() -> ProviderConfigOut {
        ProviderConfigOut {
            enabled: true,
            phone_number_id: "123".to_string(),
            public_base_url: "https://example.com".to_string(),
            business_account_id: Some("acct".to_string()),
            api_base_url: DEFAULT_API_BASE.to_string(),
            api_version: DEFAULT_API_VERSION.to_string(),
            token: Some("token".to_string()),
        }
    }

    #[test]
    fn validate_config_out_rejects_missing_required_values() {
        let mut config = valid_config_out();
        config.phone_number_id = String::new();
        assert_eq!(
            validate_config_out(&config),
            Err("config validation failed: phone_number_id is required".to_string())
        );

        let mut config = valid_config_out();
        config.public_base_url = "relative".to_string();
        assert_eq!(
            validate_config_out(&config),
            Err("config validation failed: public_base_url must be an absolute URL".to_string())
        );

        let mut config = valid_config_out();
        config.api_base_url = "relative".to_string();
        assert_eq!(
            validate_config_out(&config),
            Err("config validation failed: api_base_url must be an absolute URL".to_string())
        );
    }

    #[test]
    fn load_config_supports_nested_and_top_level_shapes() {
        let nested = load_config(&json!({
            "config": {
                "phone_number_id": "321",
                "public_base_url": "https://example.com"
            }
        }))
        .expect("nested config");
        assert_eq!(nested.phone_number_id, "321");

        let top_level = load_config(&json!({
            "phone_number_id": "654",
            "public_base_url": "https://example.com",
            "api_version": "v20.0"
        }))
        .expect("top-level config");
        assert_eq!(top_level.api_version.as_deref(), Some("v20.0"));
    }

    #[test]
    fn load_config_requires_config_values() {
        assert_eq!(
            load_config(&json!({})).unwrap_err(),
            "config required".to_string()
        );
    }

    #[test]
    fn validate_provider_config_rejects_empty_required_fields() {
        let err = validate_provider_config(ProviderConfig {
            enabled: true,
            phone_number_id: "  ".to_string(),
            public_base_url: "https://example.com".to_string(),
            business_account_id: None,
            api_base_url: None,
            api_version: None,
            token: None,
        })
        .unwrap_err();
        assert_eq!(
            err,
            "invalid config: phone_number_id cannot be empty".to_string()
        );

        let err = validate_provider_config(ProviderConfig {
            enabled: true,
            phone_number_id: "123".to_string(),
            public_base_url: "  ".to_string(),
            business_account_id: None,
            api_base_url: None,
            api_version: None,
            token: None,
        })
        .unwrap_err();
        assert_eq!(
            err,
            "invalid config: public_base_url cannot be empty".to_string()
        );
    }
}
