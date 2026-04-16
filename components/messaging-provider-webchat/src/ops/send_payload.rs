//! Step 3 of the egress pipeline: persist the encoded payload + emit a bot
//! activity back into the Direct Line conversation so frontend polling can
//! pick it up.
//!
//! For webchat, "send" is a local operation — there is no outbound HTTP call to
//! a third-party API. We write to the host state store and, when a
//! `session_id` is present on the payload, append a bot activity to the
//! stored conversation state.

use base64::{Engine as _, engine::general_purpose};
use greentic_types::messaging::universal_dto::SendPayloadInV1;
use provider_common::helpers::{json_bytes, send_payload_error, send_payload_success};
use serde_json::{Value, json};

use crate::directline::HostStateStore;
use crate::directline::jwt::DirectLineContext;
use crate::directline::state::{StoredActivity, conversation_key};
use crate::directline::store::StateStore as _;

use super::helpers::{
    extract_text, public_base_url_from_value, route_from_value, tenant_channel_from_value,
    value_as_trimmed_string,
};

pub(crate) fn send_payload(input_json: &[u8]) -> Vec<u8> {
    let send_in = match serde_json::from_slice::<SendPayloadInV1>(input_json) {
        Ok(value) => value,
        Err(err) => {
            return send_payload_error(&format!("invalid send_payload input: {err}"), false);
        }
    };
    if !send_in.provider_type.starts_with("messaging.webchat") {
        return send_payload_error("provider type mismatch", false);
    }
    let payload_bytes = match general_purpose::STANDARD.decode(&send_in.payload.body_b64) {
        Ok(bytes) => bytes,
        Err(err) => {
            return send_payload_error(&format!("payload decode failed: {err}"), false);
        }
    };
    let payload: Value = serde_json::from_slice(&payload_bytes).unwrap_or(Value::Null);
    match persist_send_payload(&payload) {
        Ok(_) => send_payload_success(),
        Err(err) => send_payload_error(&err, false),
    }
}

fn persist_send_payload(payload: &Value) -> Result<(), String> {
    let route = route_from_value(payload);
    let tenant_channel_id = tenant_channel_from_value(payload);
    let key = route
        .clone()
        .or(tenant_channel_id.clone())
        .ok_or_else(|| "route or tenant_channel_id required".to_string())?;
    let text = extract_text(payload);
    let adaptive_card_json = payload
        .get("adaptive_card")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());
    if text.is_empty() && adaptive_card_json.is_none() {
        return Err("text or adaptive_card required".into());
    }

    // If session_id is present, try to append the bot response as a Direct Line
    // activity so that GET /activities polling returns it to the frontend.
    if let Some(session_id) = value_as_trimmed_string(payload.get("session_id")) {
        let env = value_as_trimmed_string(payload.get("env"));
        let tenant = value_as_trimmed_string(payload.get("tenant"));
        let team = value_as_trimmed_string(payload.get("team"));
        let _ = append_bot_activity_to_conversation(
            &session_id,
            &text,
            adaptive_card_json.as_deref(),
            env.as_deref(),
            tenant.as_deref(),
            team.as_deref(),
        );
    }

    let public_base_url = public_base_url_from_value(payload);
    let stored = json!({
        "route": route,
        "tenant_channel_id": tenant_channel_id,
        "public_base_url": public_base_url,
        "mode": value_as_trimmed_string(payload.get("mode")).unwrap_or_else(|| "local_queue".to_string()),
        "base_url": value_as_trimmed_string(payload.get("base_url")),
        "text": text,
    });
    let mut state_store = HostStateStore;
    state_store.write(&key, &json_bytes(&stored))?;
    Ok(())
}

/// Append a bot-originated activity to the Direct Line conversation state.
/// Uses provided env/tenant context from envelope metadata, falling back to "default".
/// Best-effort: silently ignores errors (conversation may not exist).
fn append_bot_activity_to_conversation(
    conversation_id: &str,
    text: &str,
    adaptive_card_json: Option<&str>,
    env: Option<&str>,
    tenant: Option<&str>,
    team: Option<&str>,
) -> Result<(), String> {
    let ctx = DirectLineContext {
        env: env.unwrap_or("default").to_string(),
        tenant: tenant.unwrap_or("default").to_string(),
        team: team.map(str::to_string),
    };
    let conv_key = conversation_key(&ctx, conversation_id);
    let mut store = HostStateStore;

    let conv_bytes = match store.read(&conv_key) {
        Ok(Some(bytes)) => bytes,
        _ => return Ok(()), // conversation not found, skip silently
    };

    let mut conversation: crate::directline::state::ConversationState =
        serde_json::from_slice(&conv_bytes).map_err(|e| e.to_string())?;

    let watermark = conversation.bump_watermark();
    let mut raw = json!({
        "type": "message",
        "from": {"id": "bot", "name": "Bot", "role": "bot"},
    });
    if !text.is_empty() {
        raw["text"] = Value::String(text.to_string());
    }
    // Include Adaptive Card as a Direct Line attachment if present.
    if let Some(ac_json) = adaptive_card_json
        && let Ok(ac_value) = serde_json::from_str::<Value>(ac_json)
    {
        raw["attachments"] = json!([{
            "contentType": "application/vnd.microsoft.card.adaptive",
            "content": ac_value,
        }]);
    }
    let activity = StoredActivity {
        id: format!("bot-{watermark}"),
        type_: "message".to_string(),
        text: if text.is_empty() {
            None
        } else {
            Some(text.to_string())
        },
        from: Some("bot".to_string()),
        timestamp: chrono::Utc::now().timestamp_millis(),
        watermark,
        raw,
    };
    conversation.activities.push(activity);

    let updated = serde_json::to_vec(&conversation).map_err(|e| e.to_string())?;
    store.write(&conv_key, &updated)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn append_context_uses_team_from_payload() {
        assert_eq!(
            value_as_trimmed_string(json!({"team": "default"}).get("team")).as_deref(),
            Some("default")
        );
        assert_eq!(
            value_as_trimmed_string(json!({"team": "  "}).get("team")),
            None
        );
    }
}
