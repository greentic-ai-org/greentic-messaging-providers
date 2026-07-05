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
    /// Selects the inbound/outbound backend; branched on in `ingress::dispatch_post`.
    #[serde(default)]
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
    // Gmail backend fields, read by `auth::acquire_google_token`,
    // `gmail::fetch`, and `gmail::envelope`.
    /// Gmail OAuth client ID.
    #[serde(default)]
    pub(crate) gmail_client_id: Option<String>,
    /// Gmail OAuth client secret.
    #[serde(default)]
    pub(crate) gmail_client_secret: Option<String>,
    /// Gmail OAuth refresh token.
    #[serde(default)]
    pub(crate) gmail_refresh_token: Option<String>,
    /// Gmail OAuth token endpoint (defaults to Google's if unset).
    #[serde(default)]
    pub(crate) gmail_token_endpoint: Option<String>,
    /// Gmail OAuth scope.
    #[serde(default)]
    pub(crate) gmail_scope: Option<String>,
    /// Gmail mailbox address (user) polled via the Gmail API.
    #[serde(default)]
    pub(crate) gmail_user: Option<String>,
    /// Shared token verifying inbound Gmail Pub/Sub push requests.
    #[serde(default)]
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

/// Parses the `kind` secret case-insensitively; unset/unrecognized values
/// default to Graph, matching `EmailKind::default()`.
fn parse_email_kind(raw: &str) -> EmailKind {
    if raw.eq_ignore_ascii_case("gmail") {
        EmailKind::Gmail
    } else {
        EmailKind::Graph
    }
}

/// Raw secret values gathered by `config_from_secrets`. Kept separate from
/// the store I/O so the assembly logic in `build_config_from_secret_lookups`
/// is unit-testable without a live secrets-store host.
#[derive(Default)]
struct SecretLookups {
    kind: Option<String>,
    from_address: Option<String>,
    graph_tenant_id: Option<String>,
    graph_client_id: Option<String>,
    graph_client_secret: Option<String>,
    graph_refresh_token: Option<String>,
    gmail_client_id: Option<String>,
    gmail_client_secret: Option<String>,
    gmail_refresh_token: Option<String>,
    gmail_token_endpoint: Option<String>,
    gmail_scope: Option<String>,
    gmail_user: Option<String>,
    gmail_pubsub_verification_token: Option<String>,
}

/// Assembles a `ProviderConfig` from already-fetched secret values. `Graph`
/// requires `from_address`; `Gmail` falls back to `gmail_user` when
/// `from_address` is absent (Gmail's mailbox address doubles as the sender).
fn build_config_from_secret_lookups(lookups: SecretLookups) -> Result<ProviderConfig, String> {
    let kind = lookups
        .kind
        .as_deref()
        .map(parse_email_kind)
        .unwrap_or_default();
    let SecretLookups {
        kind: _,
        from_address,
        graph_tenant_id,
        graph_client_id,
        graph_client_secret,
        graph_refresh_token,
        gmail_client_id,
        gmail_client_secret,
        gmail_refresh_token,
        gmail_token_endpoint,
        gmail_scope,
        gmail_user,
        gmail_pubsub_verification_token,
    } = lookups;

    let from_address = match (from_address.filter(|s| !s.is_empty()), kind) {
        (Some(addr), _) => addr,
        (None, EmailKind::Gmail) => gmail_user.clone().unwrap_or_default(),
        (None, EmailKind::Graph) => String::new(),
    };
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
        kind,
        graph_tenant_id,
        graph_authority: None,
        graph_base_url: None,
        graph_token_endpoint: None,
        graph_scope: None,
        password: None,
        graph_client_id,
        graph_client_secret,
        graph_refresh_token,
        gmail_client_id,
        gmail_client_secret,
        gmail_refresh_token,
        gmail_token_endpoint,
        gmail_scope,
        gmail_user,
        gmail_pubsub_verification_token,
    })
}

/// Build a minimal ProviderConfig from secrets for the send path. Reads
/// `kind` to pick Graph vs. Gmail, then ALL credentials for that shape in a
/// single pass so send_payload doesn't need to call the secrets store again
/// during token acquisition.
pub(crate) fn config_from_secrets() -> Result<ProviderConfig, String> {
    let lookups = SecretLookups {
        kind: auth::get_secret_any_case("kind")
            .or_else(|_| auth::get_secret_any_case("EMAIL_KIND"))
            .ok(),
        from_address: auth::get_secret_any_case("from_address")
            .or_else(|_| auth::get_secret_any_case("FROM_ADDRESS"))
            .ok(),
        graph_tenant_id: auth::get_secret_any_case("graph_tenant_id")
            .or_else(|_| auth::get_secret_any_case("GRAPH_TENANT_ID"))
            .or_else(|_| auth::get_secret_any_case("ms_graph_tenant_id"))
            .or_else(|_| auth::get_secret_any_case("MS_GRAPH_TENANT_ID"))
            .ok(),
        graph_client_id: auth::get_secret_any_case("ms_graph_client_id")
            .or_else(|_| auth::get_secret_any_case("graph_client_id"))
            .or_else(|_| auth::get_secret_any_case("MS_GRAPH_CLIENT_ID"))
            .or_else(|_| auth::get_secret_any_case("GRAPH_CLIENT_ID"))
            .ok(),
        graph_client_secret: auth::get_secret_any_case("ms_graph_client_secret")
            .or_else(|_| auth::get_secret_any_case("graph_client_secret"))
            .or_else(|_| auth::get_secret_any_case("MS_GRAPH_CLIENT_SECRET"))
            .or_else(|_| auth::get_secret_any_case("GRAPH_CLIENT_SECRET"))
            .ok(),
        graph_refresh_token: auth::get_secret_any_case("ms_graph_refresh_token")
            .or_else(|_| auth::get_secret_any_case("graph_refresh_token"))
            .or_else(|_| auth::get_secret_any_case("MS_GRAPH_REFRESH_TOKEN"))
            .or_else(|_| auth::get_secret_any_case("GRAPH_REFRESH_TOKEN"))
            .ok(),
        gmail_client_id: auth::get_secret_any_case("gmail_client_id")
            .or_else(|_| auth::get_secret_any_case("GMAIL_CLIENT_ID"))
            .ok(),
        gmail_client_secret: auth::get_secret_any_case("gmail_client_secret")
            .or_else(|_| auth::get_secret_any_case("GMAIL_CLIENT_SECRET"))
            .ok(),
        gmail_refresh_token: auth::get_secret_any_case("gmail_refresh_token")
            .or_else(|_| auth::get_secret_any_case("GMAIL_REFRESH_TOKEN"))
            .ok(),
        gmail_token_endpoint: auth::get_secret_any_case("gmail_token_endpoint")
            .or_else(|_| auth::get_secret_any_case("GMAIL_TOKEN_ENDPOINT"))
            .ok(),
        gmail_scope: auth::get_secret_any_case("gmail_scope")
            .or_else(|_| auth::get_secret_any_case("GMAIL_SCOPE"))
            .ok(),
        gmail_user: auth::get_secret_any_case("gmail_user")
            .or_else(|_| auth::get_secret_any_case("GMAIL_USER"))
            .ok(),
        gmail_pubsub_verification_token: auth::get_secret_any_case(
            "gmail_pubsub_verification_token",
        )
        .or_else(|_| auth::get_secret_any_case("GMAIL_PUBSUB_VERIFICATION_TOKEN"))
        .ok(),
    };
    build_config_from_secret_lookups(lookups)
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

    #[test]
    fn build_config_from_secret_lookups_defaults_to_graph_when_kind_absent() {
        let lookups = SecretLookups {
            from_address: Some("bot@example.com".to_string()),
            ..Default::default()
        };

        let cfg = build_config_from_secret_lookups(lookups).expect("config");

        assert_eq!(cfg.kind, EmailKind::Graph);
        assert_eq!(cfg.from_address, "bot@example.com");
    }

    /// The real entrypoint fix: `config_from_secrets` reads the `kind`
    /// secret and the `gmail_*` secrets the same way it already reads
    /// `graph_*`; this proves the assembled config is Gmail-shaped so
    /// `dispatch_send`'s `match cfg.kind` takes the Gmail arm instead of
    /// always falling into Graph.
    #[test]
    fn build_config_from_secret_lookups_produces_gmail_shaped_config_for_kind_gmail() {
        let lookups = SecretLookups {
            kind: Some("gmail".to_string()),
            gmail_client_id: Some("client-id".to_string()),
            gmail_client_secret: Some("client-secret".to_string()),
            gmail_refresh_token: Some("refresh-token".to_string()),
            gmail_user: Some("me@example.com".to_string()),
            gmail_token_endpoint: Some("https://oauth2.googleapis.com/token".to_string()),
            gmail_scope: Some("scope".to_string()),
            gmail_pubsub_verification_token: Some("verify-token".to_string()),
            ..Default::default()
        };

        let cfg = build_config_from_secret_lookups(lookups).expect("config");

        match cfg.kind {
            EmailKind::Gmail => {}
            EmailKind::Graph => panic!("kind=gmail secret must dispatch to the Gmail arm"),
        }
        assert_eq!(cfg.from_address, "me@example.com");
        assert_eq!(cfg.username, "me@example.com");
        assert_eq!(cfg.gmail_client_id.as_deref(), Some("client-id"));
        assert_eq!(cfg.gmail_client_secret.as_deref(), Some("client-secret"));
        assert_eq!(cfg.gmail_refresh_token.as_deref(), Some("refresh-token"));
        assert_eq!(cfg.gmail_user.as_deref(), Some("me@example.com"));
        assert_eq!(
            cfg.gmail_token_endpoint.as_deref(),
            Some("https://oauth2.googleapis.com/token")
        );
        assert_eq!(cfg.gmail_scope.as_deref(), Some("scope"));
        assert_eq!(
            cfg.gmail_pubsub_verification_token.as_deref(),
            Some("verify-token")
        );
        assert!(cfg.graph_client_id.is_none());
    }

    #[test]
    fn build_config_from_secret_lookups_kind_is_case_insensitive() {
        let lookups = SecretLookups {
            kind: Some("GMAIL".to_string()),
            gmail_user: Some("me@example.com".to_string()),
            ..Default::default()
        };

        let cfg = build_config_from_secret_lookups(lookups).expect("config");

        assert_eq!(cfg.kind, EmailKind::Gmail);
    }

    #[test]
    fn build_config_from_secret_lookups_requires_from_address_or_gmail_user_for_gmail_kind() {
        let lookups = SecretLookups {
            kind: Some("gmail".to_string()),
            ..Default::default()
        };

        let err = build_config_from_secret_lookups(lookups)
            .expect_err("both from_address and gmail_user missing should fail");

        assert!(err.contains("from_address"), "{err}");
    }

    #[test]
    fn build_config_from_secret_lookups_graph_kind_does_not_fall_back_to_gmail_user() {
        let lookups = SecretLookups {
            gmail_user: Some("me@example.com".to_string()),
            ..Default::default()
        };

        let err = build_config_from_secret_lookups(lookups)
            .expect_err("graph kind must still require from_address");

        assert!(err.contains("from_address"), "{err}");
    }
}
