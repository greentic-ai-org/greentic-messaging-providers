use chrono::{DateTime, Duration, SecondsFormat, TimeZone, Utc};
use greentic_types::messaging::universal_dto::{
    SubscriptionDeleteInV1, SubscriptionDeleteOutV1, SubscriptionEnsureInV1,
    SubscriptionEnsureOutV1, SubscriptionRenewInV1, SubscriptionRenewOutV1,
};
use provider_common::helpers::json_bytes;
use serde_json::{Value, json};

use crate::auth;
use crate::bindings::greentic::http::http_client as client;
use crate::config::{ProviderConfig, load_config};
use crate::{DEFAULT_GRAPH_BASE, GRAPH_MAX_EXPIRATION_MINUTES, PROVIDER_TYPE};

pub(crate) fn graph_base_url(cfg: &ProviderConfig) -> String {
    cfg.graph_base_url
        .as_ref()
        .cloned()
        .unwrap_or_else(|| DEFAULT_GRAPH_BASE.to_string())
        .trim_end_matches('/')
        .to_string()
}

pub(crate) fn graph_post(token: &str, url: &str, body: &Value) -> Result<Value, String> {
    graph_request(token, "POST", url, Some(body))
}

pub(crate) fn graph_patch(token: &str, url: &str, body: &Value) -> Result<Value, String> {
    graph_request(token, "PATCH", url, Some(body))
}

pub(crate) fn graph_delete(token: &str, url: &str) -> Result<Value, String> {
    graph_request(token, "DELETE", url, None)
}

pub(crate) fn graph_get(token: &str, url: &str) -> Result<Value, String> {
    graph_request(token, "GET", url, None)
}

pub(crate) fn graph_request(
    token: &str,
    method: &str,
    url: &str,
    body: Option<&Value>,
) -> Result<Value, String> {
    let mut headers = vec![("Authorization".into(), format!("Bearer {token}"))];
    let (body_vec, _needs_content) = if let Some(value) = body {
        let bytes = serde_json::to_vec(value).map_err(|e| format!("invalid graph body: {e}"))?;
        headers.push(("Content-Type".into(), "application/json".into()));
        (Some(bytes), true)
    } else {
        (None, false)
    };
    let request = client::Request {
        method: method.into(),
        url: url.to_string(),
        headers,
        body: body_vec,
    };
    let resp = client::send(&request, None, None)
        .map_err(|e| format!("graph request error: {}", e.message))?;
    if resp.status < 200 || resp.status >= 300 {
        let err_body = resp
            .body
            .as_deref()
            .and_then(|b| std::str::from_utf8(b).ok())
            .unwrap_or("");
        return Err(format!(
            "graph request returned {} body={}",
            resp.status,
            &err_body[..err_body.len().min(500)]
        ));
    }
    let body = match resp.body {
        Some(body) if !body.is_empty() => body,
        _ => return Ok(Value::Null),
    };
    serde_json::from_slice(&body).map_err(|e| format!("graph response decode failed: {e}"))
}

pub(crate) fn subscription_ensure(input_json: &[u8]) -> Vec<u8> {
    let parsed = match serde_json::from_slice::<Value>(input_json) {
        Ok(value) => value,
        Err(err) => {
            return subscription_error(&format!("invalid subscription input: {err}"));
        }
    };
    let dto = match serde_json::from_value::<SubscriptionEnsureInV1>(parsed.clone()) {
        Ok(value) => value,
        Err(err) => {
            return subscription_error(&format!("invalid subscription payload: {err}"));
        }
    };
    if let Err(err) = ensure_provider(&dto.provider) {
        return subscription_error(&err);
    }
    let cfg = match load_config(&parsed) {
        Ok(cfg) => cfg,
        Err(err) => return subscription_error(&err),
    };
    let token = match auth::acquire_graph_token(&cfg, &dto.user) {
        Ok(value) => value,
        Err(err) => return subscription_error(&err),
    };
    let change_types = if dto.change_types.is_empty() {
        vec!["created".to_string()]
    } else {
        dto.change_types.clone()
    };
    let expiration = target_expiration(dto.expiration_minutes, dto.expiration_target_unix_ms);
    let expiration = clamp_expiration(expiration);
    let iso_expiration = expiration.to_rfc3339_opts(SecondsFormat::Secs, true);
    let mut body = json!({
        "changeType": change_types.join(","),
        "notificationUrl": dto.notification_url,
        "resource": dto.resource,
        "expirationDateTime": iso_expiration,
    });
    if let Some(client_state) = &dto.client_state {
        body["clientState"] = Value::String(client_state.clone());
    }
    if let Some(metadata) = &dto.metadata {
        body["metadata"] = metadata.clone();
    }
    let url = format!("{}/subscriptions", graph_base_url(&cfg));
    let resp = match graph_post(&token, &url, &body) {
        Ok(value) => value,
        Err(err) => return subscription_error(&err),
    };
    let subscription_id = resp
        .get("id")
        .and_then(Value::as_str)
        .map(|s| s.to_string())
        .unwrap_or_default();
    if subscription_id.is_empty() {
        return subscription_error("subscription response missing id");
    }
    let expiration_ms = resp
        .get("expirationDateTime")
        .and_then(Value::as_str)
        .and_then(parse_datetime)
        .map(|dt| dt.timestamp_millis() as u64)
        .unwrap_or_else(|| expiration.timestamp_millis() as u64);
    let out = SubscriptionEnsureOutV1 {
        v: 1,
        subscription_id,
        expiration_unix_ms: expiration_ms,
        resource: dto.resource,
        change_types,
        client_state: dto.client_state.clone(),
        metadata: dto.metadata.clone(),
        binding_id: dto.binding_id.clone(),
        user: dto.user,
    };
    json_bytes(&json!({"ok": true, "subscription": out}))
}

pub(crate) fn subscription_renew(input_json: &[u8]) -> Vec<u8> {
    let parsed = match serde_json::from_slice::<Value>(input_json) {
        Ok(value) => value,
        Err(err) => {
            return subscription_error(&format!("invalid subscription input: {err}"));
        }
    };
    let dto = match serde_json::from_value::<SubscriptionRenewInV1>(parsed.clone()) {
        Ok(value) => value,
        Err(err) => {
            return subscription_error(&format!("invalid subscription payload: {err}"));
        }
    };
    if let Err(err) = ensure_provider(&dto.provider) {
        return subscription_error(&err);
    }
    let cfg = match load_config(&parsed) {
        Ok(cfg) => cfg,
        Err(err) => return subscription_error(&err),
    };
    let token = match auth::acquire_graph_token(&cfg, &dto.user) {
        Ok(value) => value,
        Err(err) => return subscription_error(&err),
    };
    let expiration = target_expiration(dto.expiration_minutes, dto.expiration_target_unix_ms);
    let expiration = clamp_expiration(expiration);
    let iso_expiration = expiration.to_rfc3339_opts(SecondsFormat::Secs, true);
    let body = json!({
        "expirationDateTime": iso_expiration,
    });
    let url = format!(
        "{}/subscriptions/{}",
        graph_base_url(&cfg),
        dto.subscription_id
    );
    let resp = match graph_patch(&token, &url, &body) {
        Ok(value) => value,
        Err(err) => return subscription_error(&err),
    };
    let expiration_ms = resp
        .get("expirationDateTime")
        .and_then(Value::as_str)
        .and_then(parse_datetime)
        .map(|dt| dt.timestamp_millis() as u64)
        .unwrap_or_else(|| expiration.timestamp_millis() as u64);
    let out = SubscriptionRenewOutV1 {
        v: 1,
        subscription_id: dto.subscription_id,
        expiration_unix_ms: expiration_ms,
        metadata: dto.metadata.clone(),
        user: dto.user,
    };
    json_bytes(&json!({"ok": true, "subscription": out}))
}

pub(crate) fn subscription_delete(input_json: &[u8]) -> Vec<u8> {
    let parsed = match serde_json::from_slice::<Value>(input_json) {
        Ok(value) => value,
        Err(err) => {
            return subscription_error(&format!("invalid subscription input: {err}"));
        }
    };
    let dto = match serde_json::from_value::<SubscriptionDeleteInV1>(parsed.clone()) {
        Ok(value) => value,
        Err(err) => {
            return subscription_error(&format!("invalid subscription payload: {err}"));
        }
    };
    if let Err(err) = ensure_provider(&dto.provider) {
        return subscription_error(&err);
    }
    let cfg = match load_config(&parsed) {
        Ok(cfg) => cfg,
        Err(err) => return subscription_error(&err),
    };
    let token = match auth::acquire_graph_token(&cfg, &dto.user) {
        Ok(value) => value,
        Err(err) => return subscription_error(&err),
    };
    let url = format!(
        "{}/subscriptions/{}",
        graph_base_url(&cfg),
        dto.subscription_id
    );
    if let Err(err) = graph_delete(&token, &url) {
        return subscription_error(&err);
    }
    let out = SubscriptionDeleteOutV1 {
        v: 1,
        subscription_id: dto.subscription_id,
        user: dto.user,
    };
    json_bytes(&json!({"ok": true, "subscription": out}))
}

fn subscription_error(message: &str) -> Vec<u8> {
    json_bytes(&json!({"ok": false, "error": message}))
}

fn ensure_provider(provider: &str) -> Result<(), String> {
    if provider != PROVIDER_TYPE {
        return Err(format!(
            "provider mismatch: expected {PROVIDER_TYPE}, got {provider}"
        ));
    }
    Ok(())
}

fn target_expiration(minutes: Option<u32>, target_unix_ms: Option<u64>) -> DateTime<Utc> {
    if let Some(ms) = target_unix_ms
        && let Some(dt) = parse_datetime_value(ms)
    {
        return dt;
    }
    if let Some(mins) = minutes {
        return Utc::now() + Duration::minutes(mins as i64);
    }
    Utc::now() + Duration::minutes(GRAPH_MAX_EXPIRATION_MINUTES as i64)
}

fn clamp_expiration(expiration: DateTime<Utc>) -> DateTime<Utc> {
    let now = Utc::now();
    let max = now + Duration::minutes(GRAPH_MAX_EXPIRATION_MINUTES as i64);
    if expiration > max {
        max
    } else if expiration < now {
        now
    } else {
        expiration
    }
}

fn parse_datetime(value: &str) -> Option<DateTime<Utc>> {
    DateTime::parse_from_rfc3339(value)
        .ok()
        .map(|dt| dt.with_timezone(&Utc))
}

fn parse_datetime_value(unix_ms: u64) -> Option<DateTime<Utc>> {
    Utc.timestamp_millis_opt(unix_ms as i64).single()
}

#[cfg(test)]
mod tests {
    use super::*;
    use greentic_types::messaging::universal_dto::AuthUserRefV1;

    fn config() -> ProviderConfig {
        serde_json::from_value(json!({
            "public_base_url": "https://mail.example.com",
            "host": "smtp.example.com",
            "username": "mailer",
            "from_address": "bot@example.com",
            "graph_base_url": "https://graph.example.com/v1.0/"
        }))
        .expect("config")
    }

    fn parse_json(bytes: Vec<u8>) -> Value {
        serde_json::from_slice(&bytes).expect("json")
    }

    #[test]
    fn graph_base_url_trims_trailing_slash_and_defaults() {
        let cfg = config();
        assert_eq!(graph_base_url(&cfg), "https://graph.example.com/v1.0");

        let mut default_cfg = config();
        default_cfg.graph_base_url = None;
        assert_eq!(graph_base_url(&default_cfg), DEFAULT_GRAPH_BASE);
    }

    #[test]
    fn subscription_ensure_rejects_provider_mismatch_before_secret_lookup() {
        let input = json!({
            "v": 1,
            "provider": "slack",
            "tenant_hint": null,
            "team_hint": null,
            "binding_id": null,
            "resource": "me/messages",
            "change_types": [],
            "notification_url": "https://mail.example.com/hook",
            "expiration_minutes": null,
            "expiration_target_unix_ms": null,
            "client_state": null,
            "metadata": null,
            "user": {
                "user_id": "user-1",
                "token_key": "refresh",
                "tenant_id": null,
                "email": null,
                "display_name": null
            },
            "config": {
                "public_base_url": "https://mail.example.com",
                "host": "smtp.example.com",
                "username": "mailer",
                "from_address": "bot@example.com"
            }
        });

        let out = parse_json(subscription_ensure(input.to_string().as_bytes()));

        assert_eq!(out["ok"], false);
        assert_eq!(
            out["error"],
            format!("provider mismatch: expected {PROVIDER_TYPE}, got slack")
        );
    }

    #[test]
    fn subscription_renew_rejects_invalid_payload() {
        let out = parse_json(subscription_renew(br#"{"provider":"email"}"#.as_slice()));

        assert_eq!(out["ok"], false);
        assert!(
            out["error"]
                .as_str()
                .unwrap_or("")
                .contains("invalid subscription payload")
        );
    }

    #[test]
    fn subscription_delete_rejects_invalid_json() {
        let out = parse_json(subscription_delete(b"{"));

        assert_eq!(out["ok"], false);
        assert!(
            out["error"]
                .as_str()
                .unwrap_or("")
                .contains("invalid subscription input")
        );
    }

    #[test]
    fn expiration_helpers_parse_and_clamp() {
        let now = Utc::now();
        let future_ms = (now + Duration::minutes(10)).timestamp_millis() as u64;
        let target = target_expiration(None, Some(future_ms));
        assert!(target >= now);

        assert!(parse_datetime("2026-01-02T03:04:05Z").is_some());
        assert_eq!(parse_datetime("not a date"), None);
        assert!(parse_datetime_value(future_ms).is_some());

        let too_far = Utc::now() + Duration::minutes(GRAPH_MAX_EXPIRATION_MINUTES as i64 + 60);
        let clamped = clamp_expiration(too_far);
        assert!(clamped <= Utc::now() + Duration::minutes(GRAPH_MAX_EXPIRATION_MINUTES as i64));
    }

    #[test]
    fn subscription_error_shape_is_stable() {
        let out = parse_json(subscription_error("boom"));

        assert_eq!(out, json!({"ok": false, "error": "boom"}));
    }

    #[test]
    fn user_ref_shape_deserializes_for_subscription_tests() {
        let user: AuthUserRefV1 = serde_json::from_value(json!({
            "user_id": "u",
            "token_key": "t",
            "tenant_id": "tenant"
        }))
        .expect("user");
        assert_eq!(user.tenant_id.as_deref(), Some("tenant"));
    }
}
