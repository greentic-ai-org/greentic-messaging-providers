use serde_json::{Map, Value, json};

pub fn is_bot_framework_activity(body: &Value) -> bool {
    body.get("type").and_then(Value::as_str).is_some()
        && (body.get("serviceUrl").and_then(Value::as_str).is_some()
            || body
                .get("channelId")
                .and_then(Value::as_str)
                .is_some_and(|channel| {
                    channel.eq_ignore_ascii_case("msteams") || channel.eq_ignore_ascii_case("teams")
                }))
}

pub fn handle_bot_framework_activity(
    headers_json: &str,
    activity: &Value,
) -> Result<Value, String> {
    validate_bot_framework_auth(headers_json)?;

    let activity_type = activity
        .get("type")
        .and_then(Value::as_str)
        .unwrap_or_default();
    let service_url = activity
        .get("serviceUrl")
        .and_then(Value::as_str)
        .unwrap_or_default();
    let conversation_id = activity
        .get("conversation")
        .and_then(|value| value.get("id"))
        .and_then(Value::as_str)
        .unwrap_or_default();
    if service_url.trim().is_empty() || conversation_id.trim().is_empty() {
        return Err(
            "validation error: Bot Framework activity missing serviceUrl or conversation.id"
                .to_string(),
        );
    }

    let submit = extract_submit(activity);
    let envelope = normalize_activity(activity, submit.as_ref());
    let follow_up = follow_up_activity(activity, submit.as_ref());
    let invoke_response = if activity_type.eq_ignore_ascii_case("invoke") {
        Some(invoke_response_card(submit.as_ref()))
    } else {
        None
    };

    Ok(json!({
        "ok": true,
        "provider": "messaging.teams",
        "kind": "bot_framework_activity",
        "activity_type": activity_type,
        "service_url": service_url,
        "conversation_id": conversation_id,
        "event": activity,
        "events": [envelope],
        "conversation": {
            "serviceUrl": service_url,
            "id": conversation_id
        },
        "reply_activity": follow_up,
        "invoke_response": invoke_response
    }))
}

fn validate_bot_framework_auth(headers_json: &str) -> Result<(), String> {
    let headers: Value = serde_json::from_str(headers_json)
        .map_err(|_| "unauthorized: invalid Bot Framework headers".to_string())?;
    let authorization = header_value(&headers, "authorization")
        .or_else(|| header_value(&headers, "Authorization"))
        .unwrap_or_default();
    let token = authorization.trim();
    if token.is_empty() {
        return Err("unauthorized: Bot Framework bearer token required".to_string());
    }
    if !token.to_ascii_lowercase().starts_with("bearer ") {
        return Err("unauthorized: Bot Framework authorization must use Bearer".to_string());
    }
    if token[7..].trim().is_empty() {
        return Err("unauthorized: Bot Framework bearer token required".to_string());
    }
    Ok(())
}

fn header_value(headers: &Value, name: &str) -> Option<String> {
    let obj = headers.as_object()?;
    obj.iter()
        .find(|(key, _)| key.eq_ignore_ascii_case(name))
        .and_then(|(_, value)| value.as_str())
        .map(ToOwned::to_owned)
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct SubmitPayload {
    action_id: String,
    text: String,
    raw: Value,
}

fn extract_submit(activity: &Value) -> Option<SubmitPayload> {
    let mut merged = Map::new();
    let value = activity.get("value").unwrap_or(&Value::Null);
    merge_object(&mut merged, value);

    if let Some(data) = value.get("data") {
        merge_object(&mut merged, data);
    }
    if let Some(action_text) = value.get("action").and_then(Value::as_str) {
        merged
            .entry("action_id".to_string())
            .or_insert_with(|| Value::String(action_text.to_string()));
    }
    if let Some(action) = value.get("action") {
        merge_object(&mut merged, action);
        if let Some(data) = action.get("data") {
            merge_object(&mut merged, data);
        }
        if let Some(verb) = action.get("verb").and_then(Value::as_str) {
            merged
                .entry("action_id".to_string())
                .or_insert_with(|| Value::String(verb.to_string()));
        }
    }
    if let Some(msteams) = value.get("msteams") {
        merge_object(&mut merged, msteams);
        if let Some(inner) = msteams.get("value") {
            if let Some(parsed) = parse_json_string(inner) {
                merge_object(&mut merged, &parsed);
            } else {
                merge_object(&mut merged, inner);
            }
        }
    }

    let action_id = first_string(
        &merged,
        &[
            "action_id",
            "action",
            "verb",
            "button",
            "submitAction",
            "id",
        ],
    )
    .unwrap_or_else(|| "submit".to_string());
    let text = first_string(
        &merged,
        &[
            "text",
            "inputText",
            "comment",
            "message",
            "value",
            "displayText",
        ],
    )
    .unwrap_or_default();

    if merged.is_empty() && !activity_type(activity).eq_ignore_ascii_case("invoke") {
        return None;
    }

    Some(SubmitPayload {
        action_id,
        text,
        raw: Value::Object(merged),
    })
}

fn merge_object(target: &mut Map<String, Value>, value: &Value) {
    let Some(obj) = value.as_object() else {
        return;
    };
    for (key, val) in obj {
        if key == "msteams" || key == "data" || key == "action" {
            continue;
        }
        target.entry(key.clone()).or_insert_with(|| val.clone());
    }
}

fn parse_json_string(value: &Value) -> Option<Value> {
    let text = value.as_str()?.trim();
    if text.is_empty() {
        return None;
    }
    serde_json::from_str(text).ok()
}

fn first_string(map: &Map<String, Value>, keys: &[&str]) -> Option<String> {
    keys.iter().find_map(|key| {
        map.get(*key)
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(ToOwned::to_owned)
    })
}

fn normalize_activity(activity: &Value, submit: Option<&SubmitPayload>) -> Value {
    let activity_id = activity
        .get("id")
        .and_then(Value::as_str)
        .unwrap_or("activity");
    let conversation = activity.get("conversation").unwrap_or(&Value::Null);
    let conversation_id = conversation
        .get("id")
        .and_then(Value::as_str)
        .unwrap_or_default();
    let tenant_id = activity
        .get("channelData")
        .and_then(|value| value.get("tenant"))
        .and_then(|value| value.get("id"))
        .and_then(Value::as_str)
        .unwrap_or_default();
    let text = submit
        .map(|payload| payload.text.as_str())
        .filter(|value| !value.is_empty())
        .or_else(|| activity.get("text").and_then(Value::as_str))
        .unwrap_or_default();

    let mut metadata = Map::new();
    insert_string(&mut metadata, "provider", "messaging.teams");
    insert_string(&mut metadata, "source", "teams");
    insert_string(&mut metadata, "activity_type", activity_type(activity));
    insert_string(
        &mut metadata,
        "service_url",
        activity
            .get("serviceUrl")
            .and_then(Value::as_str)
            .unwrap_or_default(),
    );
    insert_string(&mut metadata, "conversation_id", conversation_id);
    insert_string(&mut metadata, "tenant_id", tenant_id);
    if let Some(payload) = submit {
        insert_string(&mut metadata, "action_id", &payload.action_id);
        insert_string(&mut metadata, "submitted_text", &payload.text);
        metadata.insert("submit".to_string(), payload.raw.clone());
    }

    let mut event = Map::new();
    event.insert(
        "id".to_string(),
        Value::String(format!("teams-bot:{activity_id}")),
    );
    event.insert(
        "tenant".to_string(),
        json!({
            "env": "default",
            "tenant": "default",
            "tenant_id": if tenant_id.is_empty() { "default" } else { tenant_id },
            "attempt": 0
        }),
    );
    event.insert("channel".to_string(), Value::String("teams".to_string()));
    event.insert(
        "session_id".to_string(),
        Value::String(if conversation_id.is_empty() {
            "teams".to_string()
        } else {
            conversation_id.to_string()
        }),
    );
    event.insert("text".to_string(), Value::String(text.to_string()));
    event.insert(
        "provider_message_id".to_string(),
        Value::String(format!("teams-bot:{activity_id}")),
    );
    event.insert("metadata".to_string(), Value::Object(metadata));
    if let Some(actor) = activity_actor(activity, "from") {
        event.insert("from".to_string(), actor);
    }
    if let Some(actor) = activity_actor(activity, "recipient") {
        event.insert("to".to_string(), Value::Array(vec![actor]));
    }
    Value::Object(event)
}

fn activity_type(activity: &Value) -> &str {
    activity
        .get("type")
        .and_then(Value::as_str)
        .unwrap_or_default()
}

fn activity_actor(activity: &Value, key: &str) -> Option<Value> {
    let actor = activity.get(key)?;
    let id = actor.get("id").and_then(Value::as_str).unwrap_or_default();
    if id.trim().is_empty() {
        return None;
    }
    Some(json!({
        "id": id,
        "kind": "user",
        "name": actor.get("name").and_then(Value::as_str).unwrap_or_default()
    }))
}

fn follow_up_activity(activity: &Value, submit: Option<&SubmitPayload>) -> Value {
    let card = follow_up_card(submit);
    json!({
        "type": "message",
        "from": activity.get("recipient").cloned().unwrap_or(Value::Null),
        "recipient": activity.get("from").cloned().unwrap_or(Value::Null),
        "conversation": activity.get("conversation").cloned().unwrap_or(Value::Null),
        "replyToId": activity.get("id").cloned().unwrap_or(Value::Null),
        "attachments": [
            {
                "contentType": "application/vnd.microsoft.card.adaptive",
                "content": card
            }
        ]
    })
}

fn invoke_response_card(submit: Option<&SubmitPayload>) -> Value {
    json!({
        "status": 200,
        "body": {
            "type": "application/vnd.microsoft.card.adaptive",
            "value": follow_up_card(submit)
        }
    })
}

fn follow_up_card(submit: Option<&SubmitPayload>) -> Value {
    let action_id = submit
        .map(|payload| payload.action_id.as_str())
        .unwrap_or("message");
    let submitted_text = submit.map(|payload| payload.text.as_str()).unwrap_or("");
    json!({
        "$schema": "http://adaptivecards.io/schemas/adaptive-card.json",
        "type": "AdaptiveCard",
        "version": "1.5",
        "body": [
            {
                "type": "TextBlock",
                "weight": "Bolder",
                "text": "Teams action received"
            },
            {
                "type": "FactSet",
                "facts": [
                    { "title": "Button", "value": action_id },
                    { "title": "Text", "value": submitted_text }
                ]
            }
        ]
    })
}

fn insert_string(map: &mut Map<String, Value>, key: &str, value: &str) {
    if !value.trim().is_empty() {
        map.insert(key.to_string(), Value::String(value.to_string()));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn headers() -> String {
        json!({"Authorization": "Bearer test-token"}).to_string()
    }

    fn activity(value: Value) -> Value {
        json!({
            "type": "invoke",
            "id": "activity-1",
            "serviceUrl": "https://smba.trafficmanager.net/emea/",
            "channelId": "msteams",
            "conversation": {"id": "conv-1", "conversationType": "personal"},
            "from": {"id": "user-1", "name": "Ada"},
            "recipient": {"id": "28:bot-id", "name": "Greentic"},
            "channelData": {"tenant": {"id": "tenant-1"}},
            "value": value
        })
    }

    #[test]
    fn submit_payload_extracts_action_and_text() {
        let parsed = extract_submit(&activity(json!({
            "action": "approve",
            "text": "Looks good",
            "msteams": {"type": "messageBack"}
        })))
        .expect("submit payload");
        assert_eq!(parsed.action_id, "approve");
        assert_eq!(parsed.text, "Looks good");
    }

    #[test]
    fn action_execute_payload_extracts_verb_and_data() {
        let parsed = extract_submit(&activity(json!({
            "action": {
                "type": "Action.Execute",
                "verb": "reject",
                "data": {"text": "Needs work"}
            }
        })))
        .expect("submit payload");
        assert_eq!(parsed.action_id, "reject");
        assert_eq!(parsed.text, "Needs work");
    }

    #[test]
    fn handle_activity_returns_follow_up_card() {
        let result = handle_bot_framework_activity(
            &headers(),
            &activity(json!({"action_id": "approve", "inputText": "Ship it"})),
        )
        .expect("activity");
        assert_eq!(result["ok"], true);
        assert_eq!(result["conversation"]["id"], "conv-1");
        assert_eq!(result["events"][0]["metadata"]["action_id"], "approve");
        assert_eq!(result["events"][0]["metadata"]["submitted_text"], "Ship it");
        assert_eq!(
            result["reply_activity"]["attachments"][0]["content"]["body"][1]["facts"][0]["value"],
            "approve"
        );
        assert_eq!(result["invoke_response"]["status"], 200);
    }

    #[test]
    fn missing_bearer_is_rejected() {
        let err = handle_bot_framework_activity("{}", &activity(json!({}))).unwrap_err();
        assert!(err.contains("bearer token required"));
    }
}
