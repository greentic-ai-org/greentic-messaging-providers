//! Inbound webhook ingestion from Bot Framework.
//!
//! `ingest_http` is invoked by the operator HTTP adapter for every Bot
//! Framework activity: it validates the bearer token (Phase 1: decode-only),
//! extracts `serviceUrl`, `conversation.id`, and `activity.id` for downstream
//! replies, and converts Bot Framework activities (including Adaptive Card
//! actions) into `ChannelMessageEnvelope`s for the flow runtime.

use base64::{Engine, engine::general_purpose::STANDARD};
use greentic_types::messaging::universal_dto::{HttpInV1, HttpOutV1};
use provider_common::http_compat::{http_out_error, http_out_v1_bytes, parse_operator_http_in};
use serde_json::{Value, json};

use crate::auth::{extract_bearer_token, validate_jwt};
use crate::config::{get_activity_id, get_conversation_id, load_config};

use super::build_team_envelope;

/// Handles incoming HTTP webhook from Bot Framework.
///
/// - Validates JWT from Authorization header
/// - Parses Bot Framework Activity
/// - Extracts serviceUrl, conversation.id, activity.id for downstream replies
pub(crate) fn ingest_http(input_json: &[u8]) -> Vec<u8> {
    // Try native greentic-types format first, fall back to operator format
    let request = match serde_json::from_slice::<HttpInV1>(input_json) {
        Ok(req) => req,
        Err(_) => match parse_operator_http_in(input_json) {
            Ok(req) => req,
            Err(err) => return http_out_error(400, &format!("invalid http input: {err}")),
        },
    };

    // Extract and validate JWT from Authorization header (Phase 1: decode-only)
    let auth_header = request
        .headers
        .iter()
        .find(|h| h.name.eq_ignore_ascii_case("authorization"))
        .map(|h| h.value.as_str());

    // Load config to get Bot App ID for JWT validation
    let parsed_input: Value = serde_json::from_slice(input_json).unwrap_or(Value::Null);
    let cfg = load_config(&parsed_input).ok();

    if let (Some(auth), Some(config)) = (auth_header, &cfg)
        && let Some(token) = extract_bearer_token(auth)
    {
        // Validate JWT (Phase 1: decode-only, no signature verification)
        if let Err(err) = validate_jwt(&token, &config.ms_bot_app_id) {
            // Log warning but don't fail - allow dev/testing without full validation
            eprintln!("JWT validation warning: {}", err);
        }
    }

    let body_bytes = match STANDARD.decode(&request.body_b64) {
        Ok(bytes) => bytes,
        Err(err) => return http_out_error(400, &format!("invalid body encoding: {err}")),
    };
    let body_val: Value = serde_json::from_slice(&body_bytes).unwrap_or(Value::Null);

    // Bot Framework activity: detect type and handle accordingly
    let activity_type = body_val
        .get("type")
        .and_then(Value::as_str)
        .unwrap_or_default();

    // Extract critical fields for downstream operations
    let service_url = body_val
        .get("serviceUrl")
        .and_then(Value::as_str)
        .map(|s| s.to_string());
    let conversation_id = get_conversation_id(&body_val);
    let activity_id = get_activity_id(&body_val);

    let is_bot_framework = activity_type == "invoke" || activity_type == "message";
    let is_card_action = is_bot_framework
        && (body_val.get("value").is_some()
            || body_val
                .get("name")
                .and_then(Value::as_str)
                .is_some_and(|n| n.starts_with("adaptiveCard/")));

    if is_card_action {
        return handle_card_action(&body_val, service_url, conversation_id, activity_id);
    }

    // Bot Framework message activity (regular text messages)
    let text = extract_bot_text(&body_val);
    let team_id = extract_team_id(&body_val);
    let channel_id = extract_channel_id(&body_val);
    let user = extract_sender(&body_val);

    let mut envelope = build_team_envelope(text.clone(), user, team_id.clone(), channel_id.clone());

    // Forward locale from Bot Framework activity for i18n card translation.
    if let Some(locale) = body_val.get("locale").and_then(Value::as_str)
        && !locale.is_empty()
    {
        envelope
            .metadata
            .insert("locale".to_string(), locale.to_string());
    }

    // Store critical fields for downstream replies
    if let Some(ref url) = service_url {
        envelope.metadata.insert("serviceUrl".into(), url.clone());
    }
    if let Some(ref conv_id) = conversation_id {
        envelope
            .metadata
            .insert("conversationId".into(), conv_id.clone());
    }
    if let Some(ref act_id) = activity_id {
        envelope
            .metadata
            .insert("activityId".into(), act_id.clone());
        envelope
            .metadata
            .insert("reply_to_id".into(), act_id.clone());
    }

    let normalized = json!({
        "ok": true,
        "event": body_val,
        "team_id": team_id,
        "channel_id": channel_id,
        "service_url": service_url,
        "conversation_id": conversation_id,
        "activity_id": activity_id,
    });
    let normalized_bytes = serde_json::to_vec(&normalized).unwrap_or_else(|_| b"{}".to_vec());
    let out = HttpOutV1 {
        status: 200,
        headers: Vec::new(),
        body_b64: STANDARD.encode(&normalized_bytes),
        events: vec![envelope],
    };
    http_out_v1_bytes(&out)
}

/// Converts a Bot Framework `invoke` / `message` activity carrying an Adaptive
/// Card action (`Action.Submit` or `Action.Execute`) into a synthetic envelope
/// with all action fields forwarded as metadata for MCP routing.
fn handle_card_action(
    body_val: &Value,
    service_url: Option<String>,
    conversation_id: Option<String>,
    activity_id: Option<String>,
) -> Vec<u8> {
    let sender = body_val
        .get("from")
        .and_then(|f| f.get("id"))
        .and_then(Value::as_str)
        .map(|s| s.to_string());
    let team_id = body_val
        .get("channelData")
        .and_then(|cd| cd.get("team"))
        .and_then(|t| t.get("id"))
        .and_then(Value::as_str)
        .map(|s| s.to_string());
    let channel_id = body_val
        .get("channelData")
        .and_then(|cd| cd.get("channel"))
        .and_then(|c| c.get("id"))
        .and_then(Value::as_str)
        .map(|s| s.to_string());

    // For Action.Execute: value.action.data
    // For Action.Submit: value directly
    let action_data = body_val
        .get("value")
        .and_then(|v| v.get("action"))
        .and_then(|a| a.get("data"))
        .cloned()
        .or_else(|| body_val.get("value").cloned())
        .unwrap_or(Value::Null);

    let route_to_card = action_data
        .get("routeToCardId")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string();
    let card_id = action_data
        .get("cardId")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string();
    let action_text = if !route_to_card.is_empty() {
        format!("[card:{route_to_card}]")
    } else if !card_id.is_empty() {
        format!("[action:{card_id}]")
    } else {
        "[card-action]".to_string()
    };

    let mut envelope =
        build_team_envelope(action_text, sender, team_id.clone(), channel_id.clone());

    // Store critical fields for downstream replies
    if let Some(ref url) = service_url {
        envelope.metadata.insert("serviceUrl".into(), url.clone());
    }
    if let Some(ref conv_id) = conversation_id {
        envelope
            .metadata
            .insert("conversationId".into(), conv_id.clone());
    }
    if let Some(ref act_id) = activity_id {
        envelope
            .metadata
            .insert("activityId".into(), act_id.clone());
        envelope
            .metadata
            .insert("reply_to_id".into(), act_id.clone());
    }

    if !route_to_card.is_empty() {
        envelope
            .metadata
            .insert("routeToCardId".into(), route_to_card);
    }
    if !card_id.is_empty() {
        envelope.metadata.insert("cardId".into(), card_id);
    }
    // Forward ALL action_data fields to metadata for MCP routing
    if let Some(obj) = action_data.as_object() {
        for (k, v) in obj {
            let s = match v {
                Value::String(s) => s.clone(),
                _ => v.to_string(),
            };
            envelope.metadata.insert(k.clone(), s);
        }
    }
    envelope.metadata.insert(
        "teams.actionData".into(),
        serde_json::to_string(&action_data).unwrap_or_default(),
    );

    let normalized = json!({
        "ok": true,
        "event": body_val,
        "team_id": team_id,
        "channel_id": channel_id,
        "service_url": service_url,
        "conversation_id": conversation_id,
        "activity_id": activity_id,
    });
    let normalized_bytes = serde_json::to_vec(&normalized).unwrap_or_else(|_| b"{}".to_vec());
    let out = HttpOutV1 {
        status: 200,
        headers: Vec::new(),
        body_b64: STANDARD.encode(&normalized_bytes),
        events: vec![envelope],
    };
    http_out_v1_bytes(&out)
}

/// Extracts message text from Bot Framework Activity.
pub(crate) fn extract_bot_text(value: &Value) -> String {
    // Bot Framework Activity: text is at top level
    value
        .get("text")
        .and_then(Value::as_str)
        .map(|s| s.to_string())
        .unwrap_or_default()
}

pub(crate) fn extract_team_id(value: &Value) -> Option<String> {
    // Bot Framework: channelData.team.id
    value
        .get("channelData")
        .and_then(|cd| cd.get("team"))
        .and_then(|t| t.get("id"))
        .and_then(Value::as_str)
        .map(|s| s.to_string())
}

pub(crate) fn extract_channel_id(value: &Value) -> Option<String> {
    // Bot Framework: channelData.channel.id
    value
        .get("channelData")
        .and_then(|cd| cd.get("channel"))
        .and_then(|c| c.get("id"))
        .and_then(Value::as_str)
        .map(|s| s.to_string())
}

pub(crate) fn extract_sender(value: &Value) -> Option<String> {
    // Bot Framework: from.id
    value
        .get("from")
        .and_then(|f| f.get("id"))
        .and_then(Value::as_str)
        .map(|s| s.to_string())
}
