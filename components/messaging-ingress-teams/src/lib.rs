mod bindings {
    wit_bindgen::generate!({
        path: "wit/messaging-ingress-teams",
        world: "messaging-ingress-teams",
        generate_all
    });
}

use bindings::exports::provider::common::ingress::Guest as IngressGuest;
use bindings::exports::provider::common::subscriptions::Guest as SubscriptionsGuest;
use bindings::greentic::http::http_client as client;
use bindings::greentic::secrets_store::secrets_store;
use bindings::greentic::state::state_store;
use serde::Deserialize;
use serde_json::{Map, Value, json};
use urlencoding::encode;

const DEFAULT_GRAPH_BASE: &str = "https://graph.microsoft.com/v1.0";
const DEFAULT_AUTH_BASE: &str = "https://login.microsoftonline.com";
const DEFAULT_TOKEN_SCOPE: &str = "https://graph.microsoft.com/.default";
#[cfg(not(test))]
const DEFAULT_CLIENT_ID_KEY: &str = "MS_GRAPH_CLIENT_ID";
const DEFAULT_REFRESH_TOKEN_KEY: &str = "MS_GRAPH_REFRESH_TOKEN";
const STATE_KEY: &str = "messaging.teams.subscriptions";

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct ProviderConfig {
    tenant_id: String,
    client_id: String,
    #[serde(default)]
    graph_base_url: Option<String>,
    #[serde(default)]
    auth_base_url: Option<String>,
    #[serde(default)]
    token_scope: Option<String>,
}

#[derive(Debug, Clone)]
struct SubscriptionSpec {
    resource: String,
    change_type: String,
    expiration_datetime: Option<String>,
    lifecycle_notification_url: Option<String>,
    client_state: Option<String>,
}

#[derive(Debug, Clone)]
struct ExistingSubscription {
    id: String,
    resource: String,
    change_type: String,
    expiration_datetime: Option<String>,
    notification_url: Option<String>,
}

struct Component;

impl IngressGuest for Component {
    fn handle_webhook(headers_json: String, body_json: String) -> Result<String, String> {
        if let Some(token) = validation_token_from_headers(&headers_json) {
            return Ok(token);
        }
        let parsed: Value = serde_json::from_str(&body_json)
            .map_err(|_| "validation error: invalid body".to_string())?;
        let expected_client_state = expected_client_state_from_headers(&headers_json);
        let events = normalize_graph_notifications(&parsed, expected_client_state.as_deref())?;
        let normalized = json!({
            "ok": true,
            "provider": "messaging.teams.graph",
            "event": parsed,
            "events": events,
        });
        serde_json::to_string(&normalized)
            .map_err(|_| "other error: serialization failed".to_string())
    }
}

impl SubscriptionsGuest for Component {
    fn sync_subscriptions(config_json: String, state_json: String) -> Result<String, String> {
        let config = parse_config(&config_json)?;
        let state_val = parse_state(&state_json)?;
        let webhook_url = state_val
            .get("webhook_url")
            .and_then(Value::as_str)
            .ok_or_else(|| "missing webhook_url".to_string())?;
        let desired = parse_desired_subscriptions(&state_val)?;
        if desired.is_empty() {
            return Err("no desired_subscriptions provided".into());
        }

        let token = acquire_token(&config)?;
        let mut existing = list_subscriptions(&config, &token)?;
        let mut actions: Vec<Value> = Vec::new();

        for spec in desired {
            if let Some(found) = find_matching(&existing, &spec, webhook_url) {
                if let Some(expiration) = spec.expiration_datetime.clone() {
                    renew_subscription(&config, &token, &found.id, &expiration)?;
                    actions.push(json!({
                        "action": "renewed",
                        "id": found.id,
                        "resource": found.resource,
                        "change_type": found.change_type,
                        "expiration_datetime": expiration,
                    }));
                }
            } else {
                let created = create_subscription(&config, &token, webhook_url, &spec)?;
                actions.push(json!({
                    "action": "created",
                    "id": created.id,
                    "resource": created.resource,
                    "change_type": created.change_type,
                    "expiration_datetime": created.expiration_datetime,
                }));
                existing.push(created);
            }
        }

        let state_out = json!({
            "ok": true,
            "webhook_url": webhook_url,
            "desired_subscriptions": desired_specs_to_json(&state_val),
            "subscriptions": existing_subscriptions_to_json(&existing),
            "actions": actions,
        });

        write_state(&state_out)?;
        serde_json::to_string(&state_out)
            .map_err(|_| "other error: serialization failed".to_string())
    }
}

bindings::exports::provider::common::ingress::__export_provider_common_ingress_0_0_2_cabi!(
    Component with_types_in bindings::exports::provider::common::ingress
);
bindings::exports::provider::common::subscriptions::__export_provider_common_subscriptions_0_0_2_cabi!(
    Component with_types_in bindings::exports::provider::common::subscriptions
);

fn parse_config(config_json: &str) -> Result<ProviderConfig, String> {
    if config_json.trim().is_empty() {
        return Err("config_json required".into());
    }
    serde_json::from_str::<ProviderConfig>(config_json).map_err(|e| format!("invalid config: {e}"))
}

fn parse_state(state_json: &str) -> Result<Value, String> {
    if state_json.trim().is_empty() {
        return Ok(json!({}));
    }
    serde_json::from_str::<Value>(state_json).map_err(|_| "invalid state_json".to_string())
}

fn parse_desired_subscriptions(state: &Value) -> Result<Vec<SubscriptionSpec>, String> {
    let desired = state
        .get("desired_subscriptions")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();

    desired
        .into_iter()
        .map(|entry| {
            let resource = entry
                .get("resource")
                .and_then(Value::as_str)
                .map(ToOwned::to_owned)
                .or_else(|| build_subscription_resource(&entry))
                .ok_or_else(|| {
                    "desired_subscriptions.resource or team_id/channel_id/chat_id required"
                        .to_string()
                })?;
            let change_type = entry
                .get("change_type")
                .and_then(Value::as_str)
                .unwrap_or("created");
            let expiration_datetime = entry
                .get("expiration_datetime")
                .and_then(Value::as_str)
                .map(|s| s.to_string());
            let lifecycle_notification_url = entry
                .get("lifecycle_notification_url")
                .and_then(Value::as_str)
                .map(|s| s.to_string());
            let client_state = entry
                .get("client_state")
                .and_then(Value::as_str)
                .map(|s| s.to_string());
            Ok(SubscriptionSpec {
                resource,
                change_type: change_type.to_string(),
                expiration_datetime,
                lifecycle_notification_url,
                client_state,
            })
        })
        .collect()
}

fn desired_specs_to_json(state: &Value) -> Value {
    state
        .get("desired_subscriptions")
        .cloned()
        .unwrap_or_else(|| json!([]))
}

fn existing_subscriptions_to_json(existing: &[ExistingSubscription]) -> Value {
    let list: Vec<Value> = existing
        .iter()
        .map(|sub| {
            json!({
                "id": sub.id,
                "resource": sub.resource,
                "change_type": sub.change_type,
                "expiration_datetime": sub.expiration_datetime,
                "notification_url": sub.notification_url,
            })
        })
        .collect();
    Value::Array(list)
}

fn validation_token_from_headers(headers_json: &str) -> Option<String> {
    let headers: Value = serde_json::from_str(headers_json).ok()?;
    for key in ["validationToken", "validation_token"] {
        if let Some(token) = headers.get(key).and_then(Value::as_str)
            && !token.is_empty()
        {
            return Some(token.to_string());
        }
    }
    for key in ["query", "query_string", "raw_query"] {
        if let Some(query) = headers.get(key).and_then(Value::as_str)
            && let Some(token) = query_param_value(query, "validationToken")
        {
            return Some(token);
        }
    }
    if let Some(url) = headers.get("url").and_then(Value::as_str)
        && let Some((_, query)) = url.split_once('?')
        && let Some(token) = query_param_value(query, "validationToken")
    {
        return Some(token);
    }
    None
}

fn expected_client_state_from_headers(headers_json: &str) -> Option<String> {
    let headers: Value = serde_json::from_str(headers_json).ok()?;
    headers
        .get("expected_client_state")
        .or_else(|| headers.get("client_state"))
        .and_then(Value::as_str)
        .map(ToOwned::to_owned)
}

fn query_param_value(query: &str, name: &str) -> Option<String> {
    query.split('&').find_map(|part| {
        let (key, value) = part.split_once('=')?;
        if key == name {
            Some(urlencoding::decode(value).ok()?.into_owned())
        } else {
            None
        }
    })
}

fn normalize_graph_notifications(
    body: &Value,
    expected_client_state: Option<&str>,
) -> Result<Value, String> {
    let notifications = body
        .get("value")
        .and_then(Value::as_array)
        .ok_or_else(|| "validation error: Graph notification value[] required".to_string())?;
    let mut events = Vec::new();
    for notification in notifications {
        if let Some(expected) = expected_client_state {
            let actual = notification
                .get("clientState")
                .and_then(Value::as_str)
                .unwrap_or_default();
            if actual != expected {
                return Err("validation error: Graph clientState mismatch".to_string());
            }
        }
        if let Some(event) = normalize_notification(notification) {
            events.push(event);
        }
    }
    Ok(Value::Array(events))
}

fn normalize_notification(notification: &Value) -> Option<Value> {
    let resource = notification
        .get("resource")
        .and_then(Value::as_str)
        .unwrap_or_default();
    let resolved_resource_data;
    let notification_tenant_id = notification.get("tenantId").and_then(Value::as_str);
    let resource_data = match notification.get("resourceData") {
        Some(data) if message_has_body(data) => data,
        Some(data) => {
            resolved_resource_data = resolve_graph_message(resource, notification_tenant_id)
                .unwrap_or_else(|| data.clone());
            &resolved_resource_data
        }
        None => {
            resolved_resource_data =
                resolve_graph_message(resource, notification_tenant_id).unwrap_or(Value::Null);
            &resolved_resource_data
        }
    };
    let ids = parse_graph_resource(resource);
    let message_id = resource_data
        .get("id")
        .and_then(Value::as_str)
        .or(ids.message_id.as_deref())
        .unwrap_or_default();
    let body = resource_data.get("body").unwrap_or(&Value::Null);
    let content = body
        .get("content")
        .and_then(Value::as_str)
        .unwrap_or_default();
    let content_type = body
        .get("contentType")
        .and_then(Value::as_str)
        .unwrap_or("text");
    let text = if content_type.eq_ignore_ascii_case("html") {
        strip_html(content)
    } else {
        content.to_string()
    };
    if should_skip_message(resource_data, &text) {
        return None;
    }
    let destination = if let Some(chat_id) = ids.chat_id.as_ref() {
        Some((chat_id.clone(), "chat"))
    } else if let (Some(team_id), Some(channel_id)) =
        (ids.team_id.as_ref(), ids.channel_id.as_ref())
    {
        Some((format!("{team_id}:{channel_id}"), "channel"))
    } else {
        None
    };
    let mut metadata = Map::new();
    insert_string(&mut metadata, "graph_resource", resource);
    insert_string(
        &mut metadata,
        "change_type",
        notification
            .get("changeType")
            .and_then(Value::as_str)
            .unwrap_or_default(),
    );
    insert_string(
        &mut metadata,
        "subscription_id",
        notification
            .get("subscriptionId")
            .and_then(Value::as_str)
            .unwrap_or_default(),
    );
    insert_string(
        &mut metadata,
        "tenant_id",
        notification
            .get("tenantId")
            .and_then(Value::as_str)
            .unwrap_or_default(),
    );
    insert_optional_string(&mut metadata, "team_id", ids.team_id.as_deref());
    insert_optional_string(&mut metadata, "channel_id", ids.channel_id.as_deref());
    insert_optional_string(&mut metadata, "chat_id", ids.chat_id.as_deref());
    insert_string(
        &mut metadata,
        "webUrl",
        resource_data
            .get("webUrl")
            .and_then(Value::as_str)
            .unwrap_or_default(),
    );
    insert_string(
        &mut metadata,
        "replyToId",
        resource_data
            .get("replyToId")
            .and_then(Value::as_str)
            .unwrap_or_default(),
    );
    insert_string(&mut metadata, "body_content", content);
    insert_string(&mut metadata, "body_content_type", content_type);

    let provider_message_id = if message_id.is_empty() {
        None
    } else {
        Some(format!("teams:{message_id}"))
    };
    let session_id = destination
        .as_ref()
        .map(|(id, _)| id.clone())
        .or_else(|| ids.chat_id.clone())
        .or_else(|| ids.channel_id.clone())
        .or_else(|| ids.team_id.clone())
        .unwrap_or_else(|| "teams".to_string());
    let envelope_id = provider_message_id
        .clone()
        .unwrap_or_else(|| format!("teams:{session_id}"));

    let mut event = Map::new();
    event.insert("id".to_string(), Value::String(envelope_id));
    event.insert(
        "tenant".to_string(),
        json!({
            "env": "default",
            "tenant": "default",
            "tenant_id": "default",
            "attempt": 0
        }),
    );
    event.insert("channel".to_string(), Value::String("teams".to_string()));
    event.insert("session_id".to_string(), Value::String(session_id));
    if !message_id.is_empty() {
        metadata.insert(
            "provider_message_id".to_string(),
            Value::String(format!("teams:{message_id}")),
        );
        metadata.insert(
            "message_id".to_string(),
            Value::String(message_id.to_string()),
        );
        event.insert(
            "provider_message_id".to_string(),
            Value::String(format!("teams:{message_id}")),
        );
        event.insert(
            "message_id".to_string(),
            Value::String(message_id.to_string()),
        );
    }
    metadata.insert(
        "provider".to_string(),
        Value::String("messaging.teams.graph".to_string()),
    );
    metadata.insert("source".to_string(), Value::String("teams".to_string()));
    event.insert("text".to_string(), Value::String(text));
    insert_optional_actor(&mut event, "from", graph_from_id(resource_data).as_deref());
    insert_optional_destination(&mut event, "to", destination.as_ref());
    event.insert("metadata".to_string(), Value::Object(metadata));
    Some(Value::Object(event))
}

fn message_has_body(resource_data: &Value) -> bool {
    resource_data
        .get("body")
        .and_then(|body| body.get("content"))
        .and_then(Value::as_str)
        .map(str::trim)
        .is_some_and(|content| !content.is_empty())
}

fn should_skip_message(resource_data: &Value, text: &str) -> bool {
    if has_card_attachment(resource_data) {
        return true;
    }
    text.trim().is_empty()
}

fn has_card_attachment(resource_data: &Value) -> bool {
    resource_data
        .get("attachments")
        .and_then(Value::as_array)
        .is_some_and(|attachments| {
            attachments.iter().any(|attachment| {
                attachment
                    .get("contentType")
                    .and_then(Value::as_str)
                    .is_some_and(|content_type| content_type.contains("card."))
            })
        })
}

#[cfg(test)]
fn resolve_graph_message(_resource: &str, _tenant_id: Option<&str>) -> Option<Value> {
    None
}

#[cfg(not(test))]
fn resolve_graph_message(resource: &str, tenant_id: Option<&str>) -> Option<Value> {
    let resource = resource.trim().trim_start_matches('/');
    if resource.is_empty() {
        return None;
    }
    let client_id = get_secret_any_case(DEFAULT_CLIENT_ID_KEY).ok()?;
    let cfg = ProviderConfig {
        tenant_id: tenant_id
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(ToOwned::to_owned)
            .or_else(|| get_secret_any_case("MS_GRAPH_TENANT_ID").ok())?,
        client_id,
        graph_base_url: None,
        auth_base_url: None,
        token_scope: None,
    };
    let token = acquire_token(&cfg).ok()?;
    let url = format!("{}/{}", DEFAULT_GRAPH_BASE, resource);
    let request = client::Request {
        method: "GET".into(),
        url,
        headers: vec![("Authorization".into(), format!("Bearer {}", token))],
        body: None,
    };
    let resp = client::send(&request, None, None).ok()?;
    if resp.status < 200 || resp.status >= 300 {
        return None;
    }
    let body = resp.body.unwrap_or_default();
    serde_json::from_slice(&body).ok()
}

fn insert_optional_actor(map: &mut Map<String, Value>, key: &str, id: Option<&str>) {
    let Some(id) = id.map(str::trim).filter(|value| !value.is_empty()) else {
        return;
    };
    map.insert(
        key.to_string(),
        json!({
            "id": id,
            "kind": "user"
        }),
    );
}

fn insert_optional_destination(
    map: &mut Map<String, Value>,
    key: &str,
    destination: Option<&(String, &str)>,
) {
    let Some((id, kind)) = destination else {
        return;
    };
    let id = id.trim();
    if id.is_empty() {
        return;
    }
    map.insert(
        key.to_string(),
        json!([{
            "id": id,
            "kind": kind
        }]),
    );
}

#[derive(Default)]
struct ResourceIds {
    team_id: Option<String>,
    channel_id: Option<String>,
    chat_id: Option<String>,
    message_id: Option<String>,
}

fn parse_graph_resource(resource: &str) -> ResourceIds {
    let parts: Vec<&str> = resource.trim_matches('/').split('/').collect();
    let mut ids = ResourceIds::default();
    let mut i = 0;
    while i < parts.len() {
        match parts[i] {
            segment if graph_segment_id(segment, "teams").is_some() => {
                ids.team_id = graph_segment_id(segment, "teams");
                i += 1;
            }
            segment if graph_segment_id(segment, "channels").is_some() => {
                ids.channel_id = graph_segment_id(segment, "channels");
                i += 1;
            }
            segment if graph_segment_id(segment, "chats").is_some() => {
                ids.chat_id = graph_segment_id(segment, "chats");
                i += 1;
            }
            segment if graph_segment_id(segment, "messages").is_some() => {
                ids.message_id = graph_segment_id(segment, "messages");
                i += 1;
            }
            "teams" if i + 1 < parts.len() => {
                ids.team_id = Some(parts[i + 1].to_string());
                i += 2;
            }
            "channels" if i + 1 < parts.len() => {
                ids.channel_id = Some(parts[i + 1].to_string());
                i += 2;
            }
            "chats" if i + 1 < parts.len() => {
                ids.chat_id = Some(parts[i + 1].to_string());
                i += 2;
            }
            "messages" if i + 1 < parts.len() => {
                ids.message_id = Some(parts[i + 1].to_string());
                i += 2;
            }
            "replies" if i + 1 < parts.len() => {
                ids.message_id = Some(parts[i + 1].to_string());
                i += 2;
            }
            _ => i += 1,
        }
    }
    ids
}

fn graph_segment_id(segment: &str, name: &str) -> Option<String> {
    let prefix = format!("{name}('");
    segment
        .strip_prefix(&prefix)
        .and_then(|rest| rest.strip_suffix("')"))
        .map(ToOwned::to_owned)
        .filter(|value| !value.is_empty())
}

fn graph_from_id(resource_data: &Value) -> Option<String> {
    let from = resource_data.get("from")?;
    for path in [
        &["user", "id"][..],
        &["application", "id"],
        &["device", "id"],
        &["conversation", "id"],
    ] {
        let mut current = from;
        for key in path {
            current = current.get(*key)?;
        }
        if let Some(id) = current.as_str().map(str::trim).filter(|id| !id.is_empty()) {
            return Some(id.to_string());
        }
    }
    None
}

fn insert_string(map: &mut Map<String, Value>, key: &str, value: &str) {
    map.insert(key.to_string(), Value::String(value.to_string()));
}

fn insert_optional_string(map: &mut Map<String, Value>, key: &str, value: Option<&str>) {
    if let Some(value) = value.map(str::trim).filter(|value| !value.is_empty()) {
        map.insert(key.to_string(), Value::String(value.to_string()));
    }
}

fn strip_html(input: &str) -> String {
    let mut out = String::new();
    let mut in_tag = false;
    for ch in input.chars() {
        match ch {
            '<' => in_tag = true,
            '>' => in_tag = false,
            _ if !in_tag => out.push(ch),
            _ => {}
        }
    }
    out
}

fn build_subscription_resource(entry: &Value) -> Option<String> {
    let chat_id = entry
        .get("chat_id")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty());
    if let Some(chat_id) = chat_id {
        return Some(chat_message_resource(chat_id));
    }
    let team_id = entry
        .get("team_id")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())?;
    let channel_id = entry
        .get("channel_id")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())?;
    if let Some(message_id) = entry
        .get("message_id")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        Some(channel_reply_resource(team_id, channel_id, message_id))
    } else {
        Some(channel_message_resource(team_id, channel_id))
    }
}

fn channel_message_resource(team_id: &str, channel_id: &str) -> String {
    format!("/teams/{team_id}/channels/{channel_id}/messages")
}

fn channel_reply_resource(team_id: &str, channel_id: &str, message_id: &str) -> String {
    format!("/teams/{team_id}/channels/{channel_id}/messages/{message_id}/replies")
}

fn chat_message_resource(chat_id: &str) -> String {
    format!("/chats/{chat_id}/messages")
}

fn find_matching<'a>(
    existing: &'a [ExistingSubscription],
    desired: &SubscriptionSpec,
    webhook_url: &str,
) -> Option<&'a ExistingSubscription> {
    existing.iter().find(|sub| {
        sub.resource == desired.resource
            && sub.change_type == desired.change_type
            && sub
                .notification_url
                .as_ref()
                .map(|url| url == webhook_url)
                .unwrap_or(false)
    })
}

fn acquire_token(cfg: &ProviderConfig) -> Result<String, String> {
    let auth_base = cfg
        .auth_base_url
        .clone()
        .unwrap_or_else(|| DEFAULT_AUTH_BASE.to_string());
    let token_url = format!("{}/{}/oauth2/v2.0/token", auth_base, cfg.tenant_id);
    let scope = cfg
        .token_scope
        .clone()
        .unwrap_or_else(|| DEFAULT_TOKEN_SCOPE.to_string());

    let refresh_token = get_secret(DEFAULT_REFRESH_TOKEN_KEY).map_err(|_| {
        "MS_GRAPH_REFRESH_TOKEN is required for Teams Graph subscriptions".to_string()
    })?;
    let form = format!(
        "client_id={}&grant_type=refresh_token&refresh_token={}&scope={}",
        encode(&cfg.client_id),
        encode(&refresh_token),
        encode(&scope)
    );
    send_token_request(&token_url, &form)
}

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
        return Err(format!("token endpoint returned status {}", resp.status));
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

fn list_subscriptions(
    cfg: &ProviderConfig,
    token: &str,
) -> Result<Vec<ExistingSubscription>, String> {
    let graph_base = cfg
        .graph_base_url
        .clone()
        .unwrap_or_else(|| DEFAULT_GRAPH_BASE.to_string());
    let url = format!("{}/subscriptions", graph_base);
    let request = client::Request {
        method: "GET".into(),
        url,
        headers: vec![("Authorization".into(), format!("Bearer {}", token))],
        body: None,
    };
    let resp = client::send(&request, None, None)
        .map_err(|e| format!("transport error: {}", e.message))?;
    if resp.status < 200 || resp.status >= 300 {
        return Err(graph_status_error(
            "list subscriptions",
            resp.status,
            resp.body,
        ));
    }
    let body = resp.body.unwrap_or_default();
    let json: Value = serde_json::from_slice(&body)
        .map_err(|e| format!("invalid subscriptions response: {e}"))?;
    let list = json
        .get("value")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    let mut out = Vec::new();
    for item in list {
        let id = item.get("id").and_then(Value::as_str);
        let resource = item.get("resource").and_then(Value::as_str);
        let change_type = item.get("changeType").and_then(Value::as_str);
        if let (Some(id), Some(resource), Some(change_type)) = (id, resource, change_type) {
            out.push(ExistingSubscription {
                id: id.to_string(),
                resource: resource.to_string(),
                change_type: change_type.to_string(),
                expiration_datetime: item
                    .get("expirationDateTime")
                    .and_then(Value::as_str)
                    .map(|s| s.to_string()),
                notification_url: item
                    .get("notificationUrl")
                    .and_then(Value::as_str)
                    .map(|s| s.to_string()),
            });
        }
    }
    Ok(out)
}

fn create_subscription(
    cfg: &ProviderConfig,
    token: &str,
    webhook_url: &str,
    spec: &SubscriptionSpec,
) -> Result<ExistingSubscription, String> {
    let graph_base = cfg
        .graph_base_url
        .clone()
        .unwrap_or_else(|| DEFAULT_GRAPH_BASE.to_string());
    let url = format!("{}/subscriptions", graph_base);
    let expiration = spec
        .expiration_datetime
        .clone()
        .ok_or_else(|| "desired_subscriptions.expiration_datetime required".to_string())?;

    let mut payload = json!({
        "changeType": spec.change_type,
        "notificationUrl": webhook_url,
        "lifecycleNotificationUrl": spec
            .lifecycle_notification_url
            .as_deref()
            .unwrap_or(webhook_url),
        "resource": spec.resource,
        "expirationDateTime": expiration,
    });
    if let Some(client_state) = spec.client_state.as_ref() {
        payload
            .as_object_mut()
            .expect("payload object")
            .insert("clientState".into(), Value::String(client_state.clone()));
    }

    let request = client::Request {
        method: "POST".into(),
        url,
        headers: vec![
            ("Content-Type".into(), "application/json".into()),
            ("Authorization".into(), format!("Bearer {}", token)),
        ],
        body: Some(serde_json::to_vec(&payload).unwrap_or_else(|_| b"{}".to_vec())),
    };
    let resp = client::send(&request, None, None)
        .map_err(|e| format!("transport error: {}", e.message))?;
    if resp.status < 200 || resp.status >= 300 {
        return Err(graph_status_error(
            "create subscription",
            resp.status,
            resp.body,
        ));
    }
    let body = resp.body.unwrap_or_default();
    let json: Value =
        serde_json::from_slice(&body).map_err(|e| format!("invalid create response: {e}"))?;
    let id = json
        .get("id")
        .and_then(Value::as_str)
        .ok_or_else(|| "create response missing id".to_string())?;
    Ok(ExistingSubscription {
        id: id.to_string(),
        resource: spec.resource.clone(),
        change_type: spec.change_type.clone(),
        expiration_datetime: json
            .get("expirationDateTime")
            .and_then(Value::as_str)
            .map(|s| s.to_string())
            .or(Some(expiration)),
        notification_url: Some(webhook_url.to_string()),
    })
}

fn renew_subscription(
    cfg: &ProviderConfig,
    token: &str,
    subscription_id: &str,
    expiration: &str,
) -> Result<(), String> {
    let graph_base = cfg
        .graph_base_url
        .clone()
        .unwrap_or_else(|| DEFAULT_GRAPH_BASE.to_string());
    let url = format!("{}/subscriptions/{}", graph_base, subscription_id);
    let payload = json!({ "expirationDateTime": expiration });
    let request = client::Request {
        method: "PATCH".into(),
        url,
        headers: vec![
            ("Content-Type".into(), "application/json".into()),
            ("Authorization".into(), format!("Bearer {}", token)),
        ],
        body: Some(serde_json::to_vec(&payload).unwrap_or_else(|_| b"{}".to_vec())),
    };
    let resp = client::send(&request, None, None)
        .map_err(|e| format!("transport error: {}", e.message))?;
    if resp.status < 200 || resp.status >= 300 {
        return Err(graph_status_error(
            "renew subscription",
            resp.status,
            resp.body,
        ));
    }
    Ok(())
}

fn graph_status_error(action: &str, status: u16, body: Option<Vec<u8>>) -> String {
    let body = body.unwrap_or_default();
    let body_text = String::from_utf8_lossy(&body).trim().to_string();
    if body_text.is_empty() {
        format!("{action} status {status}")
    } else {
        format!("{action} status {status}: {body_text}")
    }
}

fn write_state(state: &Value) -> Result<(), String> {
    let bytes = serde_json::to_vec(state).map_err(|_| "invalid state payload".to_string())?;
    state_store::write(STATE_KEY, &bytes, None)
        .map_err(|e| format!("state store error: {}", e.message))
        .map(|_| ())
}

fn get_secret(key: &str) -> Result<String, String> {
    match secrets_store::get(key) {
        Ok(Some(bytes)) => String::from_utf8(bytes).map_err(|_| format!("secret {key} not utf-8")),
        Ok(None) => Err(format!("missing secret: {key}")),
        Err(e) => Err(format!("secret store error: {e:?}")),
    }
}

#[cfg(not(test))]
fn get_secret_any_case(uppercase: &str) -> Result<String, String> {
    get_secret(uppercase).or_else(|_| get_secret(&uppercase.to_ascii_lowercase()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_config_applies_optional_defaults_later() {
        let cfg = parse_config(r#"{"tenant_id":"tenant","client_id":"client"}"#).expect("config");

        assert_eq!(cfg.tenant_id, "tenant");
        assert_eq!(cfg.client_id, "client");
        assert!(cfg.graph_base_url.is_none());
    }

    #[test]
    fn parse_desired_subscriptions_defaults_change_type() {
        let state = json!({
            "desired_subscriptions": [
                {
                    "resource": "teams/team-1/channels/channel-1/messages",
                    "expiration_datetime": "2026-01-01T00:00:00Z",
                    "lifecycle_notification_url": "https://example.com/lifecycle"
                }
            ]
        });

        let desired = parse_desired_subscriptions(&state).expect("desired");

        assert_eq!(desired.len(), 1);
        assert_eq!(desired[0].change_type, "created");
        assert_eq!(
            desired[0].resource,
            "teams/team-1/channels/channel-1/messages"
        );
        assert_eq!(
            desired[0].lifecycle_notification_url.as_deref(),
            Some("https://example.com/lifecycle")
        );
    }

    #[test]
    fn parse_state_accepts_empty_and_rejects_invalid_json() {
        assert_eq!(parse_state("").expect("empty state"), json!({}));
        assert_eq!(
            parse_state("{").expect_err("invalid state"),
            "invalid state_json"
        );
    }

    #[test]
    fn parse_desired_subscriptions_requires_resource() {
        let state = json!({
            "desired_subscriptions": [
                {"change_type": "updated"}
            ]
        });

        assert_eq!(
            parse_desired_subscriptions(&state).expect_err("resource required"),
            "desired_subscriptions.resource or team_id/channel_id/chat_id required"
        );
    }

    #[test]
    fn parse_desired_subscriptions_builds_channel_and_chat_resources() {
        let state = json!({
            "desired_subscriptions": [
                {"team_id": "team-1", "channel_id": "channel-1"},
                {"chat_id": "chat-1"}
            ]
        });

        let desired = parse_desired_subscriptions(&state).expect("desired");

        assert_eq!(
            desired[0].resource,
            "/teams/team-1/channels/channel-1/messages"
        );
        assert_eq!(desired[1].resource, "/chats/chat-1/messages");
    }

    #[test]
    fn find_matching_requires_same_resource_change_type_and_webhook() {
        let desired = SubscriptionSpec {
            resource: "users/me/messages".to_string(),
            change_type: "created".to_string(),
            expiration_datetime: None,
            lifecycle_notification_url: None,
            client_state: None,
        };
        let existing = vec![
            ExistingSubscription {
                id: "wrong-url".to_string(),
                resource: desired.resource.clone(),
                change_type: desired.change_type.clone(),
                expiration_datetime: None,
                notification_url: Some("https://old.example/hook".to_string()),
            },
            ExistingSubscription {
                id: "match".to_string(),
                resource: desired.resource.clone(),
                change_type: desired.change_type.clone(),
                expiration_datetime: None,
                notification_url: Some("https://new.example/hook".to_string()),
            },
        ];

        let found = find_matching(&existing, &desired, "https://new.example/hook")
            .expect("matching subscription");

        assert_eq!(found.id, "match");
    }

    #[test]
    fn existing_subscriptions_to_json_preserves_graph_fields() {
        let subscriptions = vec![ExistingSubscription {
            id: "sub-1".to_string(),
            resource: "teams/t/channels/c/messages".to_string(),
            change_type: "created".to_string(),
            expiration_datetime: Some("2026-01-01T00:00:00Z".to_string()),
            notification_url: Some("https://chat.example.com/hook".to_string()),
        }];

        let out = existing_subscriptions_to_json(&subscriptions);

        assert_eq!(out[0]["id"], "sub-1");
        assert_eq!(out[0]["resource"], "teams/t/channels/c/messages");
        assert_eq!(out[0]["notification_url"], "https://chat.example.com/hook");
    }

    #[test]
    fn graph_status_error_includes_response_body() {
        let err = graph_status_error(
            "create subscription",
            400,
            Some(br#"{"error":{"message":"lifecycleNotificationUrl is required"}}"#.to_vec()),
        );

        assert!(err.contains("create subscription status 400"));
        assert!(err.contains("lifecycleNotificationUrl is required"));
    }

    #[test]
    fn webhook_returns_validation_token_plain_text() {
        let out = <Component as IngressGuest>::handle_webhook(
            r#"{"query":"validationToken=hello%20graph"}"#.to_string(),
            String::new(),
        )
        .expect("token");

        assert_eq!(out, "hello graph");
    }

    #[test]
    fn webhook_normalizes_graph_channel_notification() {
        let out = <Component as IngressGuest>::handle_webhook(
            r#"{"expected_client_state":"state-1"}"#.to_string(),
            json!({
                "value": [{
                    "subscriptionId": "sub-1",
                    "clientState": "state-1",
                    "changeType": "created",
                    "tenantId": "tenant-1",
                    "resource": "/teams/team-1/channels/channel-1/messages/message-1",
                    "resourceData": {
                        "id": "message-1",
                        "body": {
                            "contentType": "html",
                            "content": "<b>hello</b>"
                        },
                        "from": {"user": {"id": "user-1"}},
                        "webUrl": "https://teams.example/message"
                    }
                }]
            })
            .to_string(),
        )
        .expect("normalized");
        let parsed: Value = serde_json::from_str(&out).expect("json");

        assert_eq!(parsed["ok"], true);
        assert_eq!(parsed["events"][0]["text"], "hello");
        let envelope: greentic_types::ChannelMessageEnvelope =
            serde_json::from_value(parsed["events"][0].clone()).expect("channel envelope");
        assert_eq!(envelope.id, "teams:message-1");
        assert_eq!(envelope.channel, "teams");
        assert_eq!(envelope.session_id, "team-1:channel-1");
        assert_eq!(envelope.text.as_deref(), Some("hello"));
        assert_eq!(
            parsed["events"][0]["provider_message_id"],
            "teams:message-1"
        );
        assert_eq!(parsed["events"][0]["to"][0]["id"], "team-1:channel-1");
        assert_eq!(parsed["events"][0]["to"][0]["kind"], "channel");
        assert_eq!(parsed["events"][0]["from"]["id"], "user-1");
        assert_eq!(parsed["events"][0]["from"]["kind"], "user");
        assert_eq!(parsed["events"][0]["metadata"]["team_id"], "team-1");
        assert_eq!(parsed["events"][0]["metadata"]["channel_id"], "channel-1");
    }

    #[test]
    fn webhook_omits_unknown_sender_and_parses_quoted_graph_resource_ids() {
        let out = <Component as IngressGuest>::handle_webhook(
            "{}".to_string(),
            json!({
                "value": [{
                    "subscriptionId": "sub-1",
                    "changeType": "created",
                    "tenantId": "tenant-1",
                    "resource": "teams('team-1')/channels('19:channel@thread.tacv2')/messages('1780340545252')",
                    "resourceData": {
                        "id": "1780340545252",
                        "body": {
                            "contentType": "text",
                            "content": "hello"
                        }
                    }
                }]
            })
            .to_string(),
        )
        .expect("normalized");
        let parsed: Value = serde_json::from_str(&out).expect("json");
        let event = parsed["events"][0].as_object().expect("event object");

        assert!(!event.contains_key("from"));
        assert_eq!(event["to"][0]["id"], "team-1:19:channel@thread.tacv2");
        assert_eq!(event["to"][0]["kind"], "channel");
        assert_eq!(event["metadata"]["team_id"], "team-1");
        assert_eq!(event["metadata"]["channel_id"], "19:channel@thread.tacv2");
        assert_eq!(event["provider_message_id"], "teams:1780340545252");
        let envelope: greentic_types::ChannelMessageEnvelope =
            serde_json::from_value(parsed["events"][0].clone()).expect("channel envelope");
        assert_eq!(envelope.id, "teams:1780340545252");
        assert_eq!(envelope.session_id, "team-1:19:channel@thread.tacv2");
    }

    #[test]
    fn webhook_skips_unresolved_empty_notifications() {
        let out = <Component as IngressGuest>::handle_webhook(
            "{}".to_string(),
            json!({
                "value": [{
                    "subscriptionId": "sub-1",
                    "changeType": "created",
                    "tenantId": "tenant-1",
                    "resource": "teams('team-1')/channels('channel-1')/messages('message-1')",
                    "resourceData": {
                        "id": "message-1"
                    }
                }]
            })
            .to_string(),
        )
        .expect("normalized");
        let parsed: Value = serde_json::from_str(&out).expect("json");

        assert_eq!(parsed["events"].as_array().expect("events").len(), 0);
    }

    #[test]
    fn webhook_skips_card_attachment_notifications_to_prevent_echo_loop() {
        let out = <Component as IngressGuest>::handle_webhook(
            "{}".to_string(),
            json!({
                "value": [{
                    "subscriptionId": "sub-1",
                    "changeType": "created",
                    "tenantId": "tenant-1",
                    "resource": "/teams/team-1/channels/channel-1/messages/message-1",
                    "resourceData": {
                        "id": "message-1",
                        "body": {
                            "contentType": "html",
                            "content": "<attachment id=\"greentic-adaptive-card-1\"></attachment>"
                        },
                        "attachments": [{
                            "id": "greentic-adaptive-card-1",
                            "contentType": "application/vnd.microsoft.card.adaptive",
                            "content": "{}"
                        }]
                    }
                }]
            })
            .to_string(),
        )
        .expect("normalized");
        let parsed: Value = serde_json::from_str(&out).expect("json");

        assert_eq!(parsed["events"].as_array().expect("events").len(), 0);
    }

    #[test]
    fn webhook_rejects_client_state_mismatch() {
        let err = <Component as IngressGuest>::handle_webhook(
            r#"{"expected_client_state":"expected"}"#.to_string(),
            json!({
                "value": [{
                    "clientState": "actual",
                    "resource": "/chats/chat-1/messages/message-1"
                }]
            })
            .to_string(),
        )
        .expect_err("mismatch");

        assert!(err.contains("clientState mismatch"));
    }

    #[test]
    fn resource_builders_match_graph_paths() {
        assert_eq!(
            channel_message_resource("team", "channel"),
            "/teams/team/channels/channel/messages"
        );
        assert_eq!(
            channel_reply_resource("team", "channel", "message"),
            "/teams/team/channels/channel/messages/message/replies"
        );
        assert_eq!(chat_message_resource("chat"), "/chats/chat/messages");
    }
}
