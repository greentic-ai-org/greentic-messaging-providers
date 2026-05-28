//! Microsoft Graph authentication and legacy Bot Framework JWT helpers.
//!
//! Handles:
//! - Graph token acquisition for outbound messages
//! - JWT validation for inbound webhooks (Phase 1: decode-only)

use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use urlencoding::encode as url_encode;

use crate::bindings::greentic::http::http_client as client;
use crate::config::{ProviderConfig, get_secret, get_secret_any_case};
use crate::{
    DEFAULT_BOT_APP_PASSWORD_KEY, DEFAULT_BOT_TOKEN_ENDPOINT, DEFAULT_BOT_TOKEN_SCOPE,
    DEFAULT_GRAPH_ACCESS_TOKEN_KEY, DEFAULT_GRAPH_CLIENT_SECRET_KEY,
    DEFAULT_GRAPH_REFRESH_TOKEN_KEY,
};

/// Valid issuers for Bot Framework JWT tokens.
const VALID_ISSUERS: &[&str] = &[
    "https://api.botframework.com",
    "https://sts.windows.net/d6d49420-f39b-4df7-a1dc-d59a935871db/",
    "https://login.microsoftonline.com/d6d49420-f39b-4df7-a1dc-d59a935871db/v2.0",
    "https://sts.windows.net/f8cdef31-a31e-4b4a-93e4-5f571e91255a/",
    "https://login.microsoftonline.com/f8cdef31-a31e-4b4a-93e4-5f571e91255a/v2.0",
];

/// JWT claims from Bot Framework tokens.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct BotClaims {
    /// Issuer
    pub iss: Option<String>,
    /// Audience (should be Bot App ID)
    pub aud: Option<String>,
    /// Expiration time (Unix timestamp)
    pub exp: Option<u64>,
    /// Issued at (Unix timestamp)
    pub iat: Option<u64>,
    /// Service URL (for Teams activities)
    #[serde(rename = "serviceurl")]
    pub service_url: Option<String>,
}

pub(crate) fn acquire_graph_token(cfg: &ProviderConfig) -> Result<String, String> {
    if let Some(token) = cfg
        .access_token
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
        .or_else(|| get_secret_any_case(DEFAULT_GRAPH_ACCESS_TOKEN_KEY).ok())
    {
        return Ok(token);
    }

    let refresh_token = cfg
        .refresh_token
        .clone()
        .or_else(|| get_secret_any_case(DEFAULT_GRAPH_REFRESH_TOKEN_KEY).ok())
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .ok_or_else(|| {
            "MS_GRAPH_REFRESH_TOKEN or MS_GRAPH_ACCESS_TOKEN is required for Graph auth".to_string()
        })?;

    let token_url = format!(
        "{}/{}/oauth2/v2.0/token",
        cfg.auth_base_url.trim_end_matches('/'),
        cfg.tenant_id
    );
    let mut form = graph_refresh_token_form(cfg, &refresh_token);
    if let Some(secret) = cfg
        .client_secret
        .clone()
        .or_else(|| get_secret_any_case(DEFAULT_GRAPH_CLIENT_SECRET_KEY).ok())
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
    {
        form.push_str("&client_secret=");
        form.push_str(&url_encode(&secret));
    }

    send_token_request(&token_url, &form)
}

pub(crate) fn graph_refresh_token_form(cfg: &ProviderConfig, refresh_token: &str) -> String {
    format!(
        "grant_type=refresh_token&client_id={}&refresh_token={}&scope={}",
        url_encode(&cfg.client_id),
        url_encode(refresh_token),
        url_encode(&cfg.token_scope)
    )
}

#[allow(dead_code)]
pub(crate) fn acquire_bot_token(cfg: &ProviderConfig) -> Result<String, String> {
    let app_id = cfg
        .ms_bot_app_id
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| "ms_bot_app_id is required".to_string())?;

    let password = cfg
        .ms_bot_app_password
        .clone()
        .or_else(|| get_secret(DEFAULT_BOT_APP_PASSWORD_KEY).ok())
        .ok_or_else(|| "ms_bot_app_password is required (config or secret store)".to_string())?;

    let form = format!(
        "grant_type=client_credentials&client_id={}&client_secret={}&scope={}",
        url_encode(app_id),
        url_encode(&password),
        url_encode(DEFAULT_BOT_TOKEN_SCOPE)
    );

    send_token_request(DEFAULT_BOT_TOKEN_ENDPOINT, &form)
}

/// Sends token request to the OAuth endpoint.
fn send_token_request(url: &str, form: &str) -> Result<String, String> {
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

    if resp.status < 200 || resp.status >= 300 {
        let err_body = resp
            .body
            .as_ref()
            .and_then(|b| String::from_utf8(b.clone()).ok())
            .unwrap_or_default();
        return Err(format!(
            "token endpoint returned status {}: {}",
            resp.status, err_body
        ));
    }

    let body = resp.body.unwrap_or_default();
    let json: Value =
        serde_json::from_slice(&body).map_err(|e| format!("invalid token response: {e}"))?;

    let token = json
        .get("access_token")
        .and_then(Value::as_str)
        .ok_or_else(|| "token response missing access_token".to_string())?;

    Ok(token.to_string())
}

/// Validates a JWT token from Bot Framework webhooks.
///
/// Phase 1 implementation: Decode-only validation (for dev/testing).
/// - Decodes the JWT payload without cryptographic verification
/// - Validates audience matches Bot App ID
/// - Validates expiration time
/// - Validates issuer is in the allowed list
///
/// Note: Full validation would require fetching Microsoft's public keys from
/// `https://login.botframework.com/v1/.well-known/keys` and verifying the signature.
pub(crate) fn validate_jwt(token: &str, app_id: &str) -> Result<BotClaims, String> {
    let claims = decode_jwt_claims(token)?;

    // Validate audience
    let aud = claims.aud.as_deref().unwrap_or_default();
    if aud != app_id {
        return Err(format!(
            "invalid audience: expected {}, got {}",
            app_id, aud
        ));
    }

    // Validate expiration
    if let Some(exp) = claims.exp {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);
        if exp < now {
            return Err("token expired".to_string());
        }
    }

    // Validate issuer
    let iss = claims.iss.as_deref().unwrap_or_default();
    if !VALID_ISSUERS.contains(&iss) {
        return Err(format!("invalid issuer: {}", iss));
    }

    Ok(claims)
}

/// Decodes JWT payload without signature verification.
///
/// JWT format: header.payload.signature (base64url encoded)
fn decode_jwt_claims(token: &str) -> Result<BotClaims, String> {
    let parts: Vec<&str> = token.split('.').collect();
    if parts.len() != 3 {
        return Err("invalid JWT format: expected 3 parts".to_string());
    }

    let payload = parts[1];
    let payload_bytes = URL_SAFE_NO_PAD
        .decode(payload)
        .or_else(|_| {
            // Try with padding
            let padded = match payload.len() % 4 {
                2 => format!("{}==", payload),
                3 => format!("{}=", payload),
                _ => payload.to_string(),
            };
            URL_SAFE_NO_PAD.decode(&padded)
        })
        .map_err(|e| format!("failed to decode JWT payload: {e}"))?;

    let claims: BotClaims = serde_json::from_slice(&payload_bytes)
        .map_err(|e| format!("failed to parse JWT claims: {e}"))?;

    Ok(claims)
}

/// Extracts Bearer token from Authorization header.
pub(crate) fn extract_bearer_token(auth_header: &str) -> Option<String> {
    let trimmed = auth_header.trim();
    if trimmed.len() > 7 && trimmed[..7].eq_ignore_ascii_case("bearer ") {
        Some(trimmed[7..].trim().to_string())
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use base64::engine::general_purpose::URL_SAFE_NO_PAD;
    use serde_json::json;

    #[test]
    fn extract_bearer_token_works() {
        assert_eq!(
            extract_bearer_token("Bearer abc123"),
            Some("abc123".to_string())
        );
        assert_eq!(
            extract_bearer_token("bearer abc123"),
            Some("abc123".to_string())
        );
        assert_eq!(
            extract_bearer_token("BEARER abc123"),
            Some("abc123".to_string())
        );
        assert_eq!(extract_bearer_token("Basic abc123"), None);
        assert_eq!(extract_bearer_token(""), None);
    }

    #[test]
    fn decode_jwt_claims_rejects_invalid_format() {
        assert!(decode_jwt_claims("invalid").is_err());
        assert!(decode_jwt_claims("a.b").is_err());
        assert!(decode_jwt_claims("a.b.c.d").is_err());
    }

    #[test]
    fn valid_issuers_are_defined() {
        assert!(VALID_ISSUERS.iter().all(|iss| iss.starts_with("https://")));
    }

    fn unsigned_token(claims: Value) -> Result<String, serde_json::Error> {
        let header = URL_SAFE_NO_PAD.encode(br#"{"alg":"none"}"#);
        let payload = URL_SAFE_NO_PAD.encode(serde_json::to_vec(&claims)?);
        Ok(format!("{header}.{payload}."))
    }

    #[test]
    fn validate_jwt_accepts_expected_botframework_claims() -> Result<(), String> {
        let token = unsigned_token(json!({
            "iss": "https://api.botframework.com",
            "aud": "bot-app-id",
            "exp": 4_102_444_800_u64,
            "iat": 1_u64,
            "serviceurl": "https://smba.trafficmanager.net/amer/"
        }))
        .map_err(|err| err.to_string())?;

        let claims = validate_jwt(&token, "bot-app-id")?;

        assert_eq!(claims.aud.as_deref(), Some("bot-app-id"));
        assert_eq!(
            claims.service_url.as_deref(),
            Some("https://smba.trafficmanager.net/amer/")
        );
        Ok(())
    }

    #[test]
    fn validate_jwt_rejects_wrong_audience_expired_and_issuer() -> Result<(), String> {
        let valid_exp = 4_102_444_800_u64;
        let wrong_audience = unsigned_token(json!({
            "iss": "https://api.botframework.com",
            "aud": "other",
            "exp": valid_exp
        }))
        .map_err(|err| err.to_string())?;
        assert!(
            validate_jwt(&wrong_audience, "bot-app-id")
                .expect_err("audience")
                .contains("invalid audience")
        );

        let expired = unsigned_token(json!({
            "iss": "https://api.botframework.com",
            "aud": "bot-app-id",
            "exp": 1_u64
        }))
        .map_err(|err| err.to_string())?;
        assert_eq!(
            validate_jwt(&expired, "bot-app-id").expect_err("expired"),
            "token expired"
        );

        let bad_issuer = unsigned_token(json!({
            "iss": "https://evil.example",
            "aud": "bot-app-id",
            "exp": valid_exp
        }))
        .map_err(|err| err.to_string())?;
        assert!(
            validate_jwt(&bad_issuer, "bot-app-id")
                .expect_err("issuer")
                .contains("invalid issuer")
        );
        Ok(())
    }

    #[test]
    fn acquire_bot_token_validates_local_config_before_secret_lookup() {
        let cfg = ProviderConfig {
            enabled: true,
            public_base_url: Some("https://example.com".to_string()),
            tenant_id: "tenant".to_string(),
            client_id: "client".to_string(),
            refresh_token: Some("refresh".to_string()),
            client_secret: None,
            access_token: None,
            graph_base_url: crate::DEFAULT_GRAPH_BASE_URL.to_string(),
            auth_base_url: crate::DEFAULT_AUTH_BASE_URL.to_string(),
            token_scope: crate::DEFAULT_GRAPH_TOKEN_SCOPE.to_string(),
            team_id: None,
            channel_id: None,
            chat_id: None,
            user_id: None,
            ms_bot_app_id: Some(" ".to_string()),
            ms_bot_app_password: Some("secret".to_string()),
            default_service_url: None,
        };

        assert_eq!(
            acquire_bot_token(&cfg).expect_err("missing app id"),
            "ms_bot_app_id is required"
        );
    }

    #[test]
    fn graph_refresh_token_form_omits_client_secret() {
        let cfg = ProviderConfig {
            enabled: true,
            public_base_url: None,
            tenant_id: "tenant".to_string(),
            client_id: "client id".to_string(),
            refresh_token: Some("refresh".to_string()),
            client_secret: None,
            access_token: None,
            graph_base_url: crate::DEFAULT_GRAPH_BASE_URL.to_string(),
            auth_base_url: crate::DEFAULT_AUTH_BASE_URL.to_string(),
            token_scope: "scope value".to_string(),
            team_id: None,
            channel_id: None,
            chat_id: None,
            user_id: None,
            ms_bot_app_id: None,
            ms_bot_app_password: None,
            default_service_url: None,
        };

        let form = graph_refresh_token_form(&cfg, "refresh token");

        assert!(form.contains("grant_type=refresh_token"));
        assert!(form.contains("client_id=client%20id"));
        assert!(form.contains("refresh_token=refresh%20token"));
        assert!(form.contains("scope=scope%20value"));
        assert!(!form.contains("client_secret"));
    }
}
