use crate::auth;
use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct ProviderConfig {
    #[serde(default = "default_enabled")]
    pub(crate) enabled: bool,
    #[serde(default)]
    pub(crate) public_base_url: String,
    #[serde(default)]
    pub(crate) host: String,
    #[serde(default = "default_port")]
    pub(crate) port: u16,
    #[serde(default)]
    pub(crate) username: String,
    #[serde(default)]
    pub(crate) from_address: String,
    #[serde(default = "default_tls")]
    pub(crate) tls_mode: String,
    #[serde(default)]
    pub(crate) default_to_address: Option<String>,
    #[serde(default)]
    pub(crate) graph_tenant_id: Option<String>,
    #[serde(default)]
    pub(crate) graph_authority: Option<String>,
    #[serde(default)]
    pub(crate) graph_base_url: Option<String>,
    #[serde(default)]
    pub(crate) graph_token_endpoint: Option<String>,
    #[serde(default)]
    pub(crate) graph_scope: Option<String>,
    #[serde(default)]
    pub(crate) password: Option<String>,
    /// Graph API client ID (read from secrets or config).
    #[serde(default)]
    pub(crate) graph_client_id: Option<String>,
    /// Graph API client secret (optional; for client_credentials grant).
    #[serde(default)]
    pub(crate) graph_client_secret: Option<String>,
    /// Graph API refresh token (optional; for refresh_token grant).
    #[serde(default)]
    pub(crate) graph_refresh_token: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct ProviderConfigOut {
    pub(crate) enabled: bool,
    pub(crate) public_base_url: String,
    pub(crate) host: String,
    pub(crate) port: u16,
    pub(crate) username: String,
    pub(crate) from_address: String,
    pub(crate) tls_mode: String,
    pub(crate) default_to_address: Option<String>,
    pub(crate) password: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) graph_authority: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) graph_base_url: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) graph_token_endpoint: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) graph_scope: Option<String>,
}

pub(crate) fn default_port() -> u16 {
    587
}

pub(crate) fn default_tls() -> String {
    "starttls".to_string()
}

pub(crate) fn default_enabled() -> bool {
    true
}

#[cfg(test)]
pub(crate) fn parse_config_bytes(bytes: &[u8]) -> Result<ProviderConfig, String> {
    let cfg = serde_json::from_slice::<ProviderConfig>(bytes)
        .map_err(|e| format!("invalid config: {e}"))?;
    validate_provider_config(cfg)
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
        "public_base_url",
        "host",
        "port",
        "username",
        "from_address",
        "default_to_address",
        "tls_mode",
        "password",
        "graph_tenant_id",
        "graph_authority",
        "graph_base_url",
        "graph_token_endpoint",
        "graph_scope",
        "graph_client_id",
        "graph_client_secret",
        "graph_refresh_token",
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

pub(crate) fn default_config_out() -> ProviderConfigOut {
    ProviderConfigOut {
        enabled: true,
        public_base_url: String::new(),
        host: String::new(),
        port: default_port(),
        username: String::new(),
        from_address: String::new(),
        tls_mode: default_tls(),
        default_to_address: None,
        password: None,
        graph_authority: None,
        graph_base_url: None,
        graph_token_endpoint: None,
        graph_scope: None,
    }
}

pub(crate) fn validate_config_out(config: &ProviderConfigOut) -> Result<(), String> {
    if config.from_address.trim().is_empty() {
        return Err("config validation failed: from_address is required".to_string());
    }
    if config.port == 0 {
        return Err("config validation failed: port must be greater than zero".to_string());
    }
    if !(config.public_base_url.trim().is_empty()
        || config.public_base_url.starts_with("http://")
        || config.public_base_url.starts_with("https://"))
    {
        return Err(
            "config validation failed: public_base_url must be an absolute URL".to_string(),
        );
    }
    Ok(())
}

pub(crate) fn validate_provider_config(cfg: ProviderConfig) -> Result<ProviderConfig, String> {
    if cfg.from_address.trim().is_empty() {
        return Err("invalid config: from_address cannot be empty".to_string());
    }
    if let Some(password) = cfg.password.as_deref() {
        let _ = password.trim();
    }
    Ok(cfg)
}

/// Build a minimal ProviderConfig from secrets for the Graph API send path.
/// This is used when the operator doesn't pass config via payload metadata.
/// Reads ALL Graph credentials in a single pass so send_payload doesn't need
/// to call the secrets store again during token acquisition.
pub(crate) fn config_from_secrets() -> Result<ProviderConfig, String> {
    let from_address = auth::get_secret_any_case("from_address")
        .or_else(|_| auth::get_secret_any_case("FROM_ADDRESS"))
        .unwrap_or_default();
    let graph_tenant_id = auth::get_secret_any_case("graph_tenant_id")
        .or_else(|_| auth::get_secret_any_case("GRAPH_TENANT_ID"))
        .or_else(|_| auth::get_secret_any_case("ms_graph_tenant_id"))
        .or_else(|_| auth::get_secret_any_case("MS_GRAPH_TENANT_ID"))
        .ok();
    let graph_client_id = auth::get_secret_any_case("ms_graph_client_id")
        .or_else(|_| auth::get_secret_any_case("graph_client_id"))
        .or_else(|_| auth::get_secret_any_case("MS_GRAPH_CLIENT_ID"))
        .or_else(|_| auth::get_secret_any_case("GRAPH_CLIENT_ID"))
        .ok();
    let graph_client_secret = auth::get_secret_any_case("ms_graph_client_secret")
        .or_else(|_| auth::get_secret_any_case("graph_client_secret"))
        .or_else(|_| auth::get_secret_any_case("MS_GRAPH_CLIENT_SECRET"))
        .or_else(|_| auth::get_secret_any_case("GRAPH_CLIENT_SECRET"))
        .ok();
    let graph_refresh_token = auth::get_secret_any_case("ms_graph_refresh_token")
        .or_else(|_| auth::get_secret_any_case("graph_refresh_token"))
        .or_else(|_| auth::get_secret_any_case("MS_GRAPH_REFRESH_TOKEN"))
        .or_else(|_| auth::get_secret_any_case("GRAPH_REFRESH_TOKEN"))
        .ok();
    if from_address.is_empty() {
        return Err("from_address not found in secrets (seed 'from_address' secret)".to_string());
    }
    Ok(ProviderConfig {
        enabled: true,
        public_base_url: "https://localhost".to_string(),
        host: "unused".to_string(),
        port: 587,
        username: from_address.clone(),
        from_address,
        tls_mode: "starttls".to_string(),
        default_to_address: None,
        graph_tenant_id,
        graph_authority: None,
        graph_base_url: None,
        graph_token_endpoint: None,
        graph_scope: None,
        password: None,
        graph_client_id,
        graph_client_secret,
        graph_refresh_token,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn valid_config() -> Value {
        json!({
            "public_base_url": "https://mail.example",
            "host": "smtp.example",
            "username": "mailer",
            "from_address": "bot@example.com"
        })
    }

    #[test]
    fn load_config_accepts_top_level_shape_with_defaults() {
        let cfg = load_config(&valid_config()).expect("config");

        assert!(cfg.enabled);
        assert_eq!(cfg.port, 587);
        assert_eq!(cfg.tls_mode, "starttls");
        assert_eq!(cfg.public_base_url, "https://mail.example");
    }

    #[test]
    fn nested_config_rejects_unknown_fields() {
        let value = json!({
            "config": {
                "public_base_url": "https://mail.example",
                "host": "smtp.example",
                "username": "mailer",
                "from_address": "bot@example.com",
                "surprise": true
            }
        });

        let err = load_config(&value).expect_err("unknown fields should fail");

        assert!(err.contains("unknown field"), "{err}");
    }

    #[test]
    fn validate_config_out_rejects_relative_public_base_url() {
        let mut cfg = default_config_out();
        cfg.public_base_url = "/relative".to_string();
        cfg.host = "smtp.example".to_string();
        cfg.username = "mailer".to_string();
        cfg.from_address = "bot@example.com".to_string();

        let err = validate_config_out(&cfg).expect_err("relative urls should fail");

        assert!(err.contains("absolute URL"), "{err}");
    }

    #[test]
    fn load_config_requires_real_smtp_identity() {
        let mut value = valid_config();
        value["from_address"] = json!(" ");

        let err = load_config(&value).expect_err("blank sender should fail");

        assert!(err.contains("from_address"), "{err}");
    }
}
