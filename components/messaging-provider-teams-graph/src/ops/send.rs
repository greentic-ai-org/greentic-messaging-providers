//! Step 3 of the egress pipeline: outbound HTTP to Microsoft Graph.

use base64::{Engine, engine::general_purpose::STANDARD};
use greentic_types::messaging::universal_dto::SendPayloadInV1;
use greentic_types::{ChannelMessageEnvelope, Destination};
use provider_common::helpers::{json_bytes, send_payload_error};
use serde_json::{Value, json};

use crate::PROVIDER_TYPE;
use crate::auth::{acquire_graph_token, acquire_graph_token_from_refresh};
use crate::bindings::greentic::http::http_client as client;
use crate::config::{
    ProviderConfig, default_channel_destination, default_chat_destination, load_config,
};

use super::build_team_envelope;

#[derive(Debug, Clone, PartialEq, Eq)]
enum GraphDestination {
    Channel {
        team_id: String,
        channel_id: String,
    },
    Chat {
        chat_id: String,
    },
    Reply {
        team_id: String,
        channel_id: String,
        message_id: String,
    },
}

pub(crate) fn handle_send(input_json: &[u8]) -> Vec<u8> {
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

    let envelope = match parse_or_build_envelope(input_json, &parsed, &cfg) {
        Ok(env) => env,
        Err(err) => return json_bytes(&json!({"ok": false, "error": err})),
    };
    let card = adaptive_card(&parsed);
    let (text, content_type) = match message_content(&parsed, &envelope, card.as_ref()) {
        Ok(value) => value,
        Err(err) => return json_bytes(&json!({"ok": false, "error": err})),
    };
    let destination = match resolve_destination(&parsed, &cfg, &envelope) {
        Ok(dest) => dest,
        Err(err) => return json_bytes(&json!({"ok": false, "error": err})),
    };
    graph_send(
        &cfg,
        destination,
        &text,
        &content_type,
        card.as_ref(),
        "sent",
    )
}

pub(crate) fn handle_reply(input_json: &[u8]) -> Vec<u8> {
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

    let reply_to_id = string_field(&parsed, "reply_to_id")
        .or_else(|| string_field(&parsed, "thread_id"))
        .ok_or_else(|| "reply_to_id or thread_id required".to_string());
    let reply_to_id = match reply_to_id {
        Ok(value) => value,
        Err(err) => return json_bytes(&json!({"ok": false, "error": err})),
    };
    let text = match string_field(&parsed, "text") {
        Some(value) => value,
        None => return json_bytes(&json!({"ok": false, "error": "text required"})),
    };
    let content_type = content_type(&parsed);
    let destination = match channel_ids_from_value(&parsed, &cfg) {
        Ok((team_id, channel_id)) => GraphDestination::Reply {
            team_id,
            channel_id,
            message_id: reply_to_id,
        },
        Err(err) => return json_bytes(&json!({"ok": false, "error": err})),
    };
    graph_send(&cfg, destination, &text, &content_type, None, "replied")
}

pub(crate) fn send_payload(input_json: &[u8]) -> Vec<u8> {
    let send_in = match serde_json::from_slice::<SendPayloadInV1>(input_json) {
        Ok(value) => value,
        Err(err) => {
            return send_payload_error(&format!("invalid send_payload input: {err}"), false);
        }
    };
    if !matches!(send_in.provider_type.as_str(), "messaging.teams.graph") {
        return send_payload_error("provider type mismatch", false);
    }
    let payload_bytes = match STANDARD.decode(&send_in.payload.body_b64) {
        Ok(bytes) => bytes,
        Err(err) => {
            return send_payload_error(&format!("payload decode failed: {err}"), false);
        }
    };
    let payload: Value = serde_json::from_slice(&payload_bytes).unwrap_or(Value::Null);
    let payload_bytes = serde_json::to_vec(&payload).unwrap_or_else(|_| b"{}".to_vec());
    let result_bytes = handle_send(&payload_bytes);
    let result_value: Value = serde_json::from_slice(&result_bytes).unwrap_or(Value::Null);
    let ok = result_value
        .get("ok")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    if ok {
        let msg_id = result_value
            .get("message_id")
            .and_then(Value::as_str)
            .unwrap_or_default();
        json_bytes(&json!({
            "ok": true,
            "message": msg_id,
            "retryable": false
        }))
    } else {
        let message = result_value
            .get("error")
            .and_then(Value::as_str)
            .map(|s| s.to_string())
            .unwrap_or_else(|| "send_payload failed".to_string());
        let retryable = result_value
            .get("retryable")
            .and_then(Value::as_bool)
            .unwrap_or(false);
        send_payload_error(&message, retryable)
    }
}

fn graph_send(
    cfg: &ProviderConfig,
    destination: GraphDestination,
    text: &str,
    content_type: &str,
    card: Option<&Value>,
    status: &str,
) -> Vec<u8> {
    let token = match acquire_graph_token(cfg) {
        Ok(tok) => tok,
        Err(err) => return json_bytes(&json!({"ok": false, "error": err})),
    };
    let url = graph_url(cfg, &destination);
    let body = graph_message_body(text, content_type, card);
    let mut resp = match graph_post(&url, &token, &body) {
        Ok(resp) => resp,
        Err(err) => return json_bytes(&json!({"ok": false, "error": err})),
    };
    if resp.status == 401
        && let Ok(refreshed_token) = acquire_graph_token_from_refresh(cfg)
    {
        resp = match graph_post(&url, &refreshed_token, &body) {
            Ok(resp) => resp,
            Err(err) => return json_bytes(&json!({"ok": false, "error": err})),
        };
    }

    if resp.status < 200 || resp.status >= 300 {
        let err_body = resp
            .body
            .as_ref()
            .and_then(|b| serde_json::from_slice::<Value>(b).ok())
            .unwrap_or(Value::Null);
        let (error, retryable) = classify_graph_error(resp.status, &err_body);
        return json_bytes(&json!({
            "ok": false,
            "error": error,
            "retryable": retryable,
            "response": err_body,
        }));
    }

    let body_bytes = resp.body.unwrap_or_default();
    let body_json: Value = serde_json::from_slice(&body_bytes).unwrap_or(Value::Null);
    let message_id = body_json
        .get("id")
        .and_then(Value::as_str)
        .map(ToOwned::to_owned)
        .unwrap_or_else(|| "graph-message".to_string());
    json_bytes(&json!({
        "ok": true,
        "status": status,
        "provider_type": PROVIDER_TYPE,
        "message_id": message_id,
        "provider_message_id": format!("teams:{message_id}"),
        "response": body_json,
    }))
}

fn graph_post(url: &str, token: &str, body: &Value) -> Result<client::Response, String> {
    let request = client::Request {
        method: "POST".into(),
        url: url.to_string(),
        headers: vec![
            ("Content-Type".into(), "application/json".into()),
            ("Authorization".into(), format!("Bearer {token}")),
        ],
        body: Some(serde_json::to_vec(body).unwrap_or_else(|_| b"{}".to_vec())),
    };
    client::send(&request, None, None).map_err(|err| format!("transport error: {}", err.message))
}

fn graph_url(cfg: &ProviderConfig, destination: &GraphDestination) -> String {
    let base = cfg.graph_base_url.trim_end_matches('/');
    match destination {
        GraphDestination::Channel {
            team_id,
            channel_id,
        } => format!("{base}/teams/{team_id}/channels/{channel_id}/messages"),
        GraphDestination::Reply {
            team_id,
            channel_id,
            message_id,
        } => format!("{base}/teams/{team_id}/channels/{channel_id}/messages/{message_id}/replies"),
        GraphDestination::Chat { chat_id } => format!("{base}/chats/{chat_id}/messages"),
    }
}

fn graph_message_body(text: &str, content_type: &str, card: Option<&Value>) -> Value {
    if let Some(card) = card {
        let attachment_id = "greentic-adaptive-card-1";
        return json!({
            "subject": null,
            "body": {
                "contentType": "html",
                "content": format!("<attachment id=\"{attachment_id}\"></attachment>")
            },
            "attachments": [{
                "id": attachment_id,
                "contentType": "application/vnd.microsoft.card.adaptive",
                "contentUrl": null,
                "content": serde_json::to_string(card).unwrap_or_else(|_| "{}".to_string())
            }],
            "summary": text
        });
    }
    json!({
        "body": {
            "contentType": if content_type.eq_ignore_ascii_case("html") { "html" } else { "text" },
            "content": text
        }
    })
}

fn classify_graph_error(status: u16, body: &Value) -> (String, bool) {
    let detail = body
        .get("error")
        .and_then(|err| err.get("message"))
        .and_then(Value::as_str)
        .unwrap_or_default();
    let prefix = match status {
        401 => "expired or invalid Graph token",
        403 => "missing Graph permission or admin consent",
        404 => "bad Teams team/channel/chat/message id",
        429 => "Graph rate limit exceeded",
        _ => "Graph request failed",
    };
    let message = if detail.is_empty() {
        format!("{prefix} (status {status})")
    } else {
        format!("{prefix} (status {status}): {detail}")
    };
    (message, status == 429)
}

fn parse_or_build_envelope(
    input_json: &[u8],
    parsed: &Value,
    cfg: &ProviderConfig,
) -> Result<ChannelMessageEnvelope, String> {
    serde_json::from_slice(input_json).or_else(|err| {
        build_team_envelope_from_input(parsed, cfg)
            .map_err(|message| format!("invalid envelope: {message}: {err}"))
    })
}

fn message_content(
    parsed: &Value,
    envelope: &ChannelMessageEnvelope,
    card: Option<&Value>,
) -> Result<(String, String), String> {
    let content_type = content_type(parsed);
    let text = string_field(parsed, "html")
        .filter(|_| content_type == "html")
        .or_else(|| string_field(parsed, "text"))
        .or_else(|| envelope.text.as_ref().map(|value| value.trim().to_string()))
        .filter(|value| !value.is_empty())
        .or_else(|| card.map(adaptive_card_fallback));
    text.map(|value| (value, content_type))
        .ok_or_else(|| "text or adaptive_card fallback required".to_string())
}

fn adaptive_card(parsed: &Value) -> Option<Value> {
    parsed
        .get("_ac_json")
        .and_then(Value::as_str)
        .and_then(|s| serde_json::from_str::<Value>(s).ok())
}

fn adaptive_card_fallback(card: &Value) -> String {
    let mut parts = Vec::new();
    collect_card_text(&card, &mut parts);
    let text = parts.join("\n");
    if text.trim().is_empty() {
        "[Adaptive Card]".to_string()
    } else {
        text
    }
}

fn collect_card_text(value: &Value, parts: &mut Vec<String>) {
    match value {
        Value::Object(map) => {
            if let Some(text) = map.get("text").and_then(Value::as_str)
                && !text.trim().is_empty()
            {
                parts.push(text.trim().to_string());
            }
            if let Some(title) = map.get("title").and_then(Value::as_str)
                && !title.trim().is_empty()
            {
                parts.push(title.trim().to_string());
            }
            for child in map.values() {
                collect_card_text(child, parts);
            }
        }
        Value::Array(items) => {
            for child in items {
                collect_card_text(child, parts);
            }
        }
        _ => {}
    }
}

fn content_type(parsed: &Value) -> String {
    string_field(parsed, "content_type")
        .or_else(|| {
            parsed
                .get("metadata")
                .and_then(|m| m.get("contentType").or_else(|| m.get("content_type")))
                .and_then(Value::as_str)
                .map(|s| s.trim().to_string())
        })
        .filter(|value| value.eq_ignore_ascii_case("html"))
        .map(|_| "html".to_string())
        .unwrap_or_else(|| "text".to_string())
}

fn resolve_destination(
    parsed: &Value,
    cfg: &ProviderConfig,
    envelope: &ChannelMessageEnvelope,
) -> Result<GraphDestination, String> {
    let reply_id =
        string_field(parsed, "reply_to_id").or_else(|| string_field(parsed, "thread_id"));
    if let Some(message_id) = reply_id {
        let (team_id, channel_id) = channel_ids(parsed, cfg, envelope)?;
        return Ok(GraphDestination::Reply {
            team_id,
            channel_id,
            message_id,
        });
    }

    let first_to = envelope.to.first();
    let kind = string_field(parsed, "kind")
        .or_else(|| first_to.and_then(|d| d.kind.clone()))
        .unwrap_or_else(|| "channel".to_string());
    match kind.as_str() {
        "channel" => {
            let (team_id, channel_id) = channel_ids(parsed, cfg, envelope)?;
            Ok(GraphDestination::Channel {
                team_id,
                channel_id,
            })
        }
        "chat" => {
            let chat_id = string_field(parsed, "chat_id")
                .or_else(|| {
                    first_to
                        .filter(|d| d.kind.as_deref() == Some("chat"))
                        .map(|d| d.id.clone())
                })
                .or_else(|| default_chat_destination(cfg).map(|d| d.id))
                .ok_or_else(|| "chat_id required for chat destination".to_string())?;
            Ok(GraphDestination::Chat { chat_id })
        }
        "reply" => {
            let message_id = string_field(parsed, "message_id")
                .ok_or_else(|| "message_id required for reply destination".to_string())?;
            let (team_id, channel_id) = channel_ids(parsed, cfg, envelope)?;
            Ok(GraphDestination::Reply {
                team_id,
                channel_id,
                message_id,
            })
        }
        other => Err(format!("unsupported Teams Graph destination kind: {other}")),
    }
}

fn channel_ids(
    parsed: &Value,
    cfg: &ProviderConfig,
    envelope: &ChannelMessageEnvelope,
) -> Result<(String, String), String> {
    if let Ok(ids) = channel_ids_from_value(parsed, cfg) {
        return Ok(ids);
    }
    if let Some(dest) = envelope.to.first() {
        return parse_channel_destination(dest);
    }
    if let Some(dest) = default_channel_destination(cfg) {
        return parse_channel_destination(&dest);
    }
    Err("team_id and channel_id required for channel destination".to_string())
}

fn channel_ids_from_value(
    parsed: &Value,
    cfg: &ProviderConfig,
) -> Result<(String, String), String> {
    let team_id = string_field(parsed, "team_id").or_else(|| cfg.team_id.clone());
    let channel_id = string_field(parsed, "channel_id").or_else(|| cfg.channel_id.clone());
    match (team_id, channel_id) {
        (Some(team_id), Some(channel_id)) => Ok((team_id, channel_id)),
        _ => {
            if let Some(destination) = string_field(parsed, "destination")
                .or_else(|| string_field(parsed, "to"))
                .or_else(|| string_field(parsed, "channel"))
            {
                parse_channel_id_string(&destination)
            } else {
                Err("team_id and channel_id required for channel destination".to_string())
            }
        }
    }
}

fn parse_channel_destination(dest: &Destination) -> Result<(String, String), String> {
    parse_channel_id_string(&dest.id)
}

fn parse_channel_id_string(value: &str) -> Result<(String, String), String> {
    let (team_id, channel_id) = value
        .split_once(':')
        .ok_or_else(|| "channel destination must be team_id:channel_id".to_string())?;
    let team_id = team_id.trim();
    let channel_id = channel_id.trim();
    if team_id.is_empty() || channel_id.is_empty() {
        return Err("channel destination must include team_id and channel_id".to_string());
    }
    Ok((team_id.to_string(), channel_id.to_string()))
}

fn build_team_envelope_from_input(
    parsed: &Value,
    cfg: &ProviderConfig,
) -> Result<ChannelMessageEnvelope, String> {
    let text = string_field(parsed, "text").ok_or_else(|| "text required".to_string())?;
    let destination = channel_destination(parsed, cfg)?;
    let team_id = string_field(parsed, "team_id").or_else(|| cfg.team_id.clone());
    let channel_id = string_field(parsed, "channel_id").or_else(|| cfg.channel_id.clone());
    let mut envelope = build_team_envelope(text, None, team_id, channel_id);
    envelope.to = vec![destination];
    Ok(envelope)
}

fn channel_destination(parsed: &Value, cfg: &ProviderConfig) -> Result<Destination, String> {
    let kind = string_field(parsed, "kind").unwrap_or_else(|| "channel".to_string());
    match kind.as_str() {
        "channel" => {
            let (team_id, channel_id) = channel_ids_from_value(parsed, cfg)?;
            Ok(Destination {
                id: format!("{team_id}:{channel_id}"),
                kind: Some("channel".into()),
            })
        }
        "chat" => {
            let chat_id = string_field(parsed, "chat_id")
                .or_else(|| cfg.chat_id.clone())
                .ok_or_else(|| "chat_id required for chat destination".to_string())?;
            Ok(Destination {
                id: chat_id,
                kind: Some("chat".into()),
            })
        }
        "reply" => {
            let (team_id, channel_id) = channel_ids_from_value(parsed, cfg)?;
            Ok(Destination {
                id: format!("{team_id}:{channel_id}"),
                kind: Some("reply".into()),
            })
        }
        other => Err(format!(
            "unsupported destination kind for envelope fallback: {other}"
        )),
    }
}

fn string_field(parsed: &Value, key: &str) -> Option<String> {
    parsed
        .get(key)
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
}

#[cfg(test)]
mod tests {
    use super::*;
    use base64::engine::general_purpose::STANDARD;
    use greentic_types::messaging::universal_dto::{ProviderPayloadV1, SendPayloadInV1};

    fn cfg() -> ProviderConfig {
        ProviderConfig {
            enabled: true,
            public_base_url: None,
            setup_mode: None,
            tenant_id: "tenant".to_string(),
            client_id: "client".to_string(),
            refresh_token: Some("refresh".to_string()),
            client_secret: None,
            access_token: Some("token".to_string()),
            graph_base_url: "https://graph.microsoft.com/v1.0".to_string(),
            auth_base_url: "https://login.microsoftonline.com".to_string(),
            token_scope: "https://graph.microsoft.com/.default".to_string(),
            team_id: Some("default-team".to_string()),
            team_name: Some("Default Team".to_string()),
            channel_id: Some("default-channel".to_string()),
            channel_name: Some("General".to_string()),
            desired_channel_name: Some("General".to_string()),
            chat_id: Some("default-chat".to_string()),
            user_id: None,
            ms_bot_app_id: None,
            ms_bot_app_password: None,
            bot_display_name: None,
            messaging_endpoint: None,
            default_service_url: None,
        }
    }

    #[test]
    fn graph_url_builds_channel_reply_and_chat_endpoints() {
        let cfg = cfg();
        assert_eq!(
            graph_url(
                &cfg,
                &GraphDestination::Channel {
                    team_id: "team".to_string(),
                    channel_id: "channel".to_string()
                }
            ),
            "https://graph.microsoft.com/v1.0/teams/team/channels/channel/messages"
        );
        assert_eq!(
            graph_url(
                &cfg,
                &GraphDestination::Reply {
                    team_id: "team".to_string(),
                    channel_id: "channel".to_string(),
                    message_id: "message".to_string()
                }
            ),
            "https://graph.microsoft.com/v1.0/teams/team/channels/channel/messages/message/replies"
        );
        assert_eq!(
            graph_url(
                &cfg,
                &GraphDestination::Chat {
                    chat_id: "chat".to_string()
                }
            ),
            "https://graph.microsoft.com/v1.0/chats/chat/messages"
        );
    }

    #[test]
    fn graph_message_body_uses_graph_chat_message_shape() {
        assert_eq!(
            graph_message_body("hello", "text", None),
            json!({"body": {"contentType": "text", "content": "hello"}})
        );
        assert_eq!(
            graph_message_body("<b>hello</b>", "html", None),
            json!({"body": {"contentType": "html", "content": "<b>hello</b>"}})
        );
    }

    #[test]
    fn graph_message_body_sends_adaptive_card_as_graph_attachment() {
        let body = graph_message_body(
            "fallback",
            "text",
            Some(&json!({
                "type": "AdaptiveCard",
                "version": "1.3",
                "body": [{"type": "TextBlock", "text": "hello"}]
            })),
        );

        assert_eq!(body["body"]["contentType"], "html");
        assert_eq!(
            body["body"]["content"],
            "<attachment id=\"greentic-adaptive-card-1\"></attachment>"
        );
        assert_eq!(
            body["attachments"][0]["contentType"],
            "application/vnd.microsoft.card.adaptive"
        );
        let card: Value = serde_json::from_str(body["attachments"][0]["content"].as_str().unwrap())
            .expect("card content json");
        assert_eq!(card["type"], "AdaptiveCard");
        assert_eq!(body["summary"], "fallback");
    }

    #[test]
    fn channel_destination_accepts_team_colon_channel() {
        assert_eq!(
            parse_channel_id_string("team:channel").expect("ids"),
            ("team".to_string(), "channel".to_string())
        );
        assert!(parse_channel_id_string("channel").is_err());
    }

    #[test]
    fn resolve_destination_supports_explicit_channel_chat_and_reply() {
        let cfg = cfg();
        let envelope = build_team_envelope("hello".to_string(), None, None, None);
        assert_eq!(
            resolve_destination(
                &json!({"team_id":"team","channel_id":"channel"}),
                &cfg,
                &envelope
            )
            .expect("channel"),
            GraphDestination::Channel {
                team_id: "team".to_string(),
                channel_id: "channel".to_string()
            }
        );
        assert_eq!(
            resolve_destination(&json!({"kind":"chat","chat_id":"chat"}), &cfg, &envelope)
                .expect("chat"),
            GraphDestination::Chat {
                chat_id: "chat".to_string()
            }
        );
        assert_eq!(
            resolve_destination(
                &json!({"reply_to_id":"msg","team_id":"team","channel_id":"channel"}),
                &cfg,
                &envelope
            )
            .expect("reply"),
            GraphDestination::Reply {
                team_id: "team".to_string(),
                channel_id: "channel".to_string(),
                message_id: "msg".to_string()
            }
        );
    }

    #[test]
    fn resolve_destination_uses_ids_not_human_labels() {
        let mut cfg = cfg();
        cfg.team_id = Some("team-id".to_string());
        cfg.team_name = Some("Human Team".to_string());
        cfg.channel_id = Some("channel-id".to_string());
        cfg.channel_name = Some("General".to_string());
        let envelope = build_team_envelope("hello".to_string(), None, None, None);

        assert_eq!(
            resolve_destination(&json!({}), &cfg, &envelope).expect("destination"),
            GraphDestination::Channel {
                team_id: "team-id".to_string(),
                channel_id: "channel-id".to_string()
            }
        );

        assert!(
            resolve_destination(
                &json!({
                    "team_name": "Human Team",
                    "channel_name": "General"
                }),
                &ProviderConfig {
                    team_id: None,
                    channel_id: None,
                    ..cfg
                },
                &envelope
            )
            .expect_err("labels are not routable")
            .contains("team_id and channel_id")
        );
    }

    #[test]
    fn send_payload_rejects_non_teams_provider_types() {
        let payload = ProviderPayloadV1 {
            content_type: "application/json".to_string(),
            body_b64: STANDARD.encode(br#"{}"#),
            metadata: Default::default(),
        };
        let input = SendPayloadInV1 {
            provider_type: "messaging.slack.api".to_string(),
            tenant_id: None,
            auth_user: None,
            payload,
        };

        let out: Value =
            serde_json::from_slice(&send_payload(&serde_json::to_vec(&input).expect("input")))
                .expect("json");

        assert_eq!(out["ok"], false);
        assert!(
            out["message"]
                .as_str()
                .unwrap_or_default()
                .contains("mismatch")
        );
    }
}
