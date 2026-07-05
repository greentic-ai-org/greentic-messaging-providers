use crate::auth;
use serde::{Deserialize, Serialize};
use serde_json::Value;

/// Selects which inbound/outbound backend the email provider uses.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub(crate) enum EmailKind {
    #[default]
    Graph,
    Gmail,
}

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
    /// Selects the inbound/outbound backend; consumed by the Gmail ingress
    /// path added in a later task.
    #[serde(default)]
    #[allow(dead_code)]
    pub(crate) kind: EmailKind,
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
    // Gmail backend fields (consumed by the Gmail fetch/auth path added in a
    // later task): deserialized and schema-declared now, not yet read.
    /// Gmail OAuth client ID.
    #[serde(default)]
    #[allow(dead_code)]
    pub(crate) gmail_client_id: Option<String>,
    /// Gmail OAuth client secret.
    #[serde(default)]
    #[allow(dead_code)]
    pub(crate) gmail_client_secret: Option<String>,
    /// Gmail OAuth refresh token.
    #[serde(default)]
    #[allow(dead_code)]
    pub(crate) gmail_refresh_token: Option<String>,
    /// Gmail OAuth token endpoint (defaults to Google's if unset).
    #[serde(default)]
    #[allow(dead_code)]
    pub(crate) gmail_token_endpoint: Option<String>,
    /// Gmail OAuth scope.
    #[serde(default)]
    #[allow(dead_code)]
    pub(crate) gmail_scope: Option<String>,
    /// Gmail mailbox address (user) polled via the Gmail API.
    #[serde(default)]
    #[allow(dead_code)]
    pub(crate) gmail_user: Option<String>,
    /// Shared token verifying inbound Gmail Pub/Sub push requests.
    #[serde(default)]
    #[allow(dead_code)]
    pub(crate) gmail_pubsub_verification_token: Option<String>,
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
        "kind",
        "graph_tenant_id",
        "graph_authority",
        "graph_base_url",
        "graph_token_endpoint",
        "graph_scope",
        "graph_client_id",
        "graph_client_secret",
        "graph_refresh_token",
        "gmail_client_id",
        "gmail_client_secret",
        "gmail_refresh_token",
        "gmail_token_endpoint",
        "gmail_scope",
        "gmail_user",
        "gmail_pubsub_verification_token",
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
        kind: EmailKind::Graph,
        graph_tenant_id,
        graph_authority: None,
        graph_base_url: None,
        graph_token_endpoint: None,
        graph_scope: None,
        password: None,
        graph_client_id,
        graph_client_secret,
        graph_refresh_token,
        gmail_client_id: None,
        gmail_client_secret: None,
        gmail_refresh_token: None,
        gmail_token_endpoint: None,
        gmail_scope: None,
        gmail_user: None,
        gmail_pubsub_verification_token: None,
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

    #[test]
    fn load_config_with_kind_gmail_and_gmail_fields_deserializes() {
        let mut value = valid_config();
        value["kind"] = json!("gmail");
        value["gmail_client_id"] = json!("client-id");
        value["gmail_client_secret"] = json!("client-secret");
        value["gmail_refresh_token"] = json!("refresh-token");
        value["gmail_token_endpoint"] = json!("https://oauth2.googleapis.com/token");
        value["gmail_scope"] = json!("https://www.googleapis.com/auth/gmail.readonly");
        value["gmail_user"] = json!("me@example.com");
        value["gmail_pubsub_verification_token"] = json!("shared-token");

        let cfg = load_config(&value).expect("config");

        assert_eq!(cfg.kind, EmailKind::Gmail);
        assert_eq!(cfg.gmail_client_id.as_deref(), Some("client-id"));
        assert_eq!(cfg.gmail_client_secret.as_deref(), Some("client-secret"));
        assert_eq!(cfg.gmail_refresh_token.as_deref(), Some("refresh-token"));
        assert_eq!(
            cfg.gmail_token_endpoint.as_deref(),
            Some("https://oauth2.googleapis.com/token")
        );
        assert_eq!(
            cfg.gmail_scope.as_deref(),
            Some("https://www.googleapis.com/auth/gmail.readonly")
        );
        assert_eq!(cfg.gmail_user.as_deref(), Some("me@example.com"));
        assert_eq!(
            cfg.gmail_pubsub_verification_token.as_deref(),
            Some("shared-token")
        );
    }

    #[test]
    fn load_config_with_only_graph_fields_defaults_kind_to_graph() {
        let mut value = valid_config();
        value["graph_tenant_id"] = json!("tenant-123");
        value["graph_client_id"] = json!("graph-client-id");

        let cfg = load_config(&value).expect("config");

        assert_eq!(cfg.kind, EmailKind::Graph);
        assert_eq!(cfg.gmail_client_id, None);
        assert_eq!(cfg.gmail_pubsub_verification_token, None);
    }

    #[test]
    fn nested_config_still_rejects_unknown_fields_with_gmail_fields_present() {
        let value = json!({
            "config": {
                "public_base_url": "https://mail.example",
                "host": "smtp.example",
                "username": "mailer",
                "from_address": "bot@example.com",
                "kind": "gmail",
                "gmail_client_id": "client-id",
                "surprise": true
            }
        });

        let err = load_config(&value).expect_err("unknown fields should fail");

        assert!(err.contains("unknown field"), "{err}");
    }
}
