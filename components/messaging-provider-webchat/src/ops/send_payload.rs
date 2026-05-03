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
    let extensions = payload.get("extensions").cloned();
    let envelope_attachments = payload.get("attachments").filter(|v| v.is_array()).cloned();
    if text.is_empty()
        && adaptive_card_json.is_none()
        && extensions.is_none()
        && envelope_attachments.is_none()
    {
        return Err("text, adaptive_card, attachments, or extensions required".into());
    }

    // If session_id is present, try to append the bot response as a Direct Line
    // activity so that GET /activities polling returns it to the frontend.
    if let Some(session_id) = value_as_trimmed_string(payload.get("session_id")) {
        let env = value_as_trimmed_string(payload.get("env"));
        let tenant = value_as_trimmed_string(payload.get("tenant"));
        let team = value_as_trimmed_string(payload.get("team"));
        if let Err(err) = append_bot_activity_to_conversation(
            &session_id,
            &text,
            adaptive_card_json.as_deref(),
            extensions.as_ref(),
            envelope_attachments.as_ref(),
            env.as_deref(),
            tenant.as_deref(),
            team.as_deref(),
        ) {
            // Surface failures: the activity append is best-effort but a
            // silent miss leaves /activities polling empty. Operator log
            // tail is the only signal we have when this happens in cloud.
            eprintln!(
                "[webchat send_payload] activity append failed conv={} env={:?} tenant={:?} team={:?} text_len={} ac={} err={}",
                session_id,
                env,
                tenant,
                team,
                text.len(),
                adaptive_card_json.is_some(),
                err,
            );
        }
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
#[allow(clippy::too_many_arguments)]
fn append_bot_activity_to_conversation(
    conversation_id: &str,
    text: &str,
    adaptive_card_json: Option<&str>,
    extensions: Option<&Value>,
    envelope_attachments: Option<&Value>,
    env: Option<&str>,
    tenant: Option<&str>,
    team: Option<&str>,
) -> Result<(), String> {
    let ctx = DirectLineContext {
        env: env.unwrap_or("default").to_string(),
        tenant: tenant.unwrap_or("default").to_string(),
        team: team.map(str::to_string),
    };
    let mut store = HostStateStore;
    append_bot_activity_to_conversation_with_store(
        &mut store,
        &ctx,
        conversation_id,
        text,
        adaptive_card_json,
        extensions,
        envelope_attachments,
    )
}

#[allow(clippy::too_many_arguments)]
fn append_bot_activity_to_conversation_with_store<S: crate::directline::store::StateStore>(
    store: &mut S,
    ctx: &DirectLineContext,
    conversation_id: &str,
    text: &str,
    adaptive_card_json: Option<&str>,
    extensions: Option<&Value>,
    envelope_attachments: Option<&Value>,
) -> Result<(), String> {
    let (conv_key, conv_bytes) =
        match find_existing_conversation_state(store, ctx, conversation_id)? {
            Some(found) => found,
            None => return Ok(()),
        };

    let mut conversation: crate::directline::state::ConversationState =
        serde_json::from_slice(&conv_bytes).map_err(|e| e.to_string())?;

    let watermark = conversation.bump_watermark();
    let raw = build_bot_activity_raw(text, adaptive_card_json, extensions, envelope_attachments);

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

fn find_existing_conversation_state<S: crate::directline::store::StateStore>(
    store: &mut S,
    ctx: &DirectLineContext,
    conversation_id: &str,
) -> Result<Option<(String, Vec<u8>)>, String> {
    let mut tried_keys: Vec<String> = Vec::new();
    for candidate_ctx in candidate_conversation_contexts(ctx) {
        let key = conversation_key(&candidate_ctx, conversation_id);
        if let Some(bytes) = store.read(&key)? {
            return Ok(Some((key, bytes)));
        }
        tried_keys.push(key);
    }
    // No candidate matched — log the keys we tried so operators can
    // diff against what handle_conversations actually wrote.
    eprintln!(
        "[webchat send_payload] conversation lookup miss conv={} ctx_env={} ctx_tenant={} ctx_team={:?} tried_keys=[{}]",
        conversation_id,
        ctx.env,
        ctx.tenant,
        ctx.team,
        tried_keys.join(","),
    );
    Ok(None)
}

fn candidate_conversation_contexts(ctx: &DirectLineContext) -> Vec<DirectLineContext> {
    let mut contexts: Vec<DirectLineContext> = Vec::new();
    contexts.push(ctx.clone());

    // team=None — covers JWT contexts that didn't carry a team (token URL had
    // no `team` query param) but the egress path injected one via metadata.
    contexts.push(DirectLineContext {
        env: ctx.env.clone(),
        tenant: ctx.tenant.clone(),
        team: None,
    });

    // team="default" — covers the inverse case where the conversation was
    // created with the operator-default team but the egress path lost it.
    contexts.push(DirectLineContext {
        env: ctx.env.clone(),
        tenant: ctx.tenant.clone(),
        team: Some("default".to_string()),
    });

    // If the supplied team string has surrounding whitespace, also try the
    // trimmed form. Defends against operator-side metadata stuffing.
    if let Some(team) = ctx.team.as_deref() {
        let trimmed = team.trim();
        if !trimmed.is_empty() && trimmed != team {
            contexts.push(DirectLineContext {
                env: ctx.env.clone(),
                tenant: ctx.tenant.clone(),
                team: Some(trimmed.to_string()),
            });
        }
    }

    contexts.dedup();
    contexts
}

/// Build the raw DirectLine activity JSON for a bot message, merging any
/// `extensions` fields (attachments, channelData, entities, etc.) and the
/// envelope's top-level `attachments` field back into their DirectLine-native
/// camelCase form.
///
/// Merge order for attachments: legacy AC (metadata) → envelope.attachments
/// (top-level typed field) → extensions.adaptive_card → extensions.attachments.
///
/// Pure function — no state I/O — suitable for unit testing the merge logic.
fn build_bot_activity_raw(
    text: &str,
    adaptive_card_json: Option<&str>,
    extensions: Option<&Value>,
    envelope_attachments: Option<&Value>,
) -> Value {
    let mut raw = json!({
        "type": "message",
        "from": {"id": "bot", "name": "Bot", "role": "bot"},
    });
    if !text.is_empty() {
        raw["text"] = Value::String(text.to_string());
    }

    let mut attachments: Vec<Value> = Vec::new();
    if let Some(ac_json) = adaptive_card_json
        && let Ok(ac_value) = serde_json::from_str::<Value>(ac_json)
    {
        attachments.push(json!({
            "contentType": "application/vnd.microsoft.card.adaptive",
            "content": ac_value,
        }));
    }

    if let Some(Value::Array(env_atts)) = envelope_attachments {
        attachments.extend(env_atts.clone());
    }

    if let Some(ext) = extensions
        && let Some(ext_obj) = ext.as_object()
    {
        if adaptive_card_json.is_none()
            && let Some(ac) = ext_obj.get("adaptive_card")
        {
            attachments.push(json!({
                "contentType": "application/vnd.microsoft.card.adaptive",
                "content": ac.clone(),
            }));
        }
        if let Some(Value::Array(ext_atts)) = ext_obj.get("attachments") {
            attachments.extend(ext_atts.clone());
        }
        let passthroughs: &[(&str, &str)] = &[
            ("entities", "entities"),
            ("name", "name"),
            ("input_hint", "inputHint"),
            ("speak", "speak"),
            ("suggested_actions", "suggestedActions"),
        ];
        if let Some(channel_data) = ext_obj
            .get("channel_data")
            .and_then(sanitize_outbound_channel_data)
        {
            raw["channelData"] = channel_data;
        }
        for (src, dst) in passthroughs {
            if let Some(v) = ext_obj.get(*src)
                && !v.is_null()
            {
                raw[*dst] = v.clone();
            }
        }
    }

    if !attachments.is_empty() {
        raw["attachments"] = Value::Array(attachments);
    }
    raw
}

fn sanitize_outbound_channel_data(value: &Value) -> Option<Value> {
    let mut channel_data = match value {
        Value::Object(map) => map.clone(),
        other if !other.is_null() => return Some(other.clone()),
        _ => return None,
    };

    // These keys are useful on the inbound Direct Line activity, but if they
    // are echoed on a bot response the webchat client can treat the response
    // as a postBack/request artefact instead of rendering the card.
    for key in [
        "postBack",
        "clientActivityID",
        "clientActivityId",
        "attachmentSizes",
    ] {
        channel_data.remove(key);
    }

    if channel_data.is_empty() {
        None
    } else {
        Some(Value::Object(channel_data))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use std::collections::BTreeMap;

    #[derive(Default)]
    struct MemoryStateStore {
        values: BTreeMap<String, Vec<u8>>,
    }

    impl crate::directline::store::StateStore for MemoryStateStore {
        fn read(&mut self, key: &str) -> Result<Option<Vec<u8>>, String> {
            Ok(self.values.get(key).cloned())
        }

        fn write(&mut self, key: &str, value: &[u8]) -> Result<(), String> {
            self.values.insert(key.to_string(), value.to_vec());
            Ok(())
        }
    }

    #[test]
    fn build_bot_activity_raw_plain_text_only() {
        let raw = build_bot_activity_raw("hello", None, None, None);
        assert_eq!(raw["type"], "message");
        assert_eq!(raw["text"], "hello");
        assert_eq!(raw["from"]["id"], "bot");
        assert!(raw.get("attachments").is_none());
    }

    #[test]
    fn build_bot_activity_raw_adaptive_card_via_legacy_metadata() {
        // Upstream still writes metadata["adaptive_card"] as JSON string.
        let ac_json = r#"{"type":"AdaptiveCard","body":[{"type":"TextBlock","text":"hi"}]}"#;
        let raw = build_bot_activity_raw("hi", Some(ac_json), None, None);

        let attachments = raw["attachments"].as_array().expect("attachments array");
        assert_eq!(attachments.len(), 1);
        assert_eq!(
            attachments[0]["contentType"],
            "application/vnd.microsoft.card.adaptive"
        );
        assert_eq!(attachments[0]["content"]["type"], "AdaptiveCard");
    }

    #[test]
    fn build_bot_activity_raw_adaptive_card_via_extensions() {
        // New path: RAG component writes extensions["adaptive_card"] as typed Value.
        let extensions = json!({
            "adaptive_card": {"type": "AdaptiveCard", "body": [{"type": "TextBlock", "text": "hi"}]},
        });
        let raw = build_bot_activity_raw("hi", None, Some(&extensions), None);

        let attachments = raw["attachments"].as_array().expect("attachments array");
        assert_eq!(attachments.len(), 1);
        assert_eq!(
            attachments[0]["contentType"],
            "application/vnd.microsoft.card.adaptive"
        );
        assert_eq!(attachments[0]["content"]["type"], "AdaptiveCard");
    }

    #[test]
    fn build_bot_activity_raw_merges_ac_from_metadata_and_native_attachments_from_extensions() {
        // Legacy AC in metadata + provider-native attachments in extensions.
        let ac_json = r#"{"type":"AdaptiveCard","body":[]}"#;
        let extensions = json!({
            "attachments": [
                {"contentType": "application/vnd.microsoft.card.hero", "content": {"title": "Hero"}}
            ]
        });
        let raw = build_bot_activity_raw("", Some(ac_json), Some(&extensions), None);

        let attachments = raw["attachments"].as_array().expect("attachments array");
        assert_eq!(attachments.len(), 2, "AC + hero card expected");
        assert_eq!(
            attachments[0]["contentType"],
            "application/vnd.microsoft.card.adaptive"
        );
        assert_eq!(
            attachments[1]["contentType"],
            "application/vnd.microsoft.card.hero"
        );
    }

    #[test]
    fn build_bot_activity_raw_materializes_all_directline_fields() {
        // Full scenario: RAG component emits AC + citations (via channelData.rag) +
        // all DirectLine Bot Framework fields, expecting them preserved verbatim.
        let extensions = json!({
            "adaptive_card": {"type": "AdaptiveCard", "body": []},
            "channel_data": {"rag": {"citations": [{"id": "c1"}]}, "feature": "x"},
            "entities": [{"type": "mention", "text": "@bot"}],
            "input_hint": "acceptingInput",
            "speak": "hello there",
            "suggested_actions": {"actions": [{"type": "imBack", "title": "Yes", "value": "yes"}]},
            "name": "event/custom",
        });

        let raw = build_bot_activity_raw("Based on your docs...", None, Some(&extensions), None);

        // Text preserved.
        assert_eq!(raw["text"], "Based on your docs...");
        // AC wrapped as attachment.
        let atts = raw["attachments"].as_array().unwrap();
        assert_eq!(
            atts[0]["contentType"],
            "application/vnd.microsoft.card.adaptive"
        );
        // Snake_case extensions keys → DirectLine camelCase.
        assert_eq!(
            raw["channelData"],
            json!({"rag": {"citations": [{"id": "c1"}]}, "feature": "x"})
        );
        assert_eq!(raw["entities"][0]["type"], "mention");
        assert_eq!(raw["inputHint"], "acceptingInput");
        assert_eq!(raw["speak"], "hello there");
        assert_eq!(raw["suggestedActions"]["actions"][0]["value"], "yes");
        assert_eq!(raw["name"], "event/custom");
    }

    #[test]
    fn build_bot_activity_raw_rag_citations_round_trip() {
        // Regression test for TASK-082 Bug 3 — RAG component emits citations
        // via extensions["channel_data"]["rag"], they must survive into the
        // DirectLine activity's channelData field exactly as emitted.
        let extensions = json!({
            "adaptive_card": {"type": "AdaptiveCard", "body": []},
            "channel_data": {
                "rag": {
                    "citations": [
                        {"id": "c1", "source": "docs/x.md", "snippet": "..."},
                        {"id": "c2", "source": "docs/y.md", "snippet": "..."}
                    ]
                }
            }
        });

        let raw = build_bot_activity_raw("answer", None, Some(&extensions), None);

        let citations = raw
            .pointer("/channelData/rag/citations")
            .and_then(|v| v.as_array())
            .expect("citations preserved under channelData.rag");
        assert_eq!(citations.len(), 2);
        assert_eq!(citations[0]["id"], "c1");
        assert_eq!(citations[0]["source"], "docs/x.md");
        assert_eq!(citations[1]["id"], "c2");
    }

    #[test]
    fn build_bot_activity_raw_skips_null_extension_fields() {
        let extensions = json!({
            "channel_data": null,
            "entities": null,
            "speak": "keep me",
        });
        let raw = build_bot_activity_raw("x", None, Some(&extensions), None);
        assert!(raw.get("channelData").is_none());
        assert!(raw.get("entities").is_none());
        assert_eq!(raw["speak"], "keep me");
    }

    #[test]
    fn build_bot_activity_raw_strips_click_artifacts_from_channel_data() {
        let extensions = json!({
            "channel_data": {
                "postBack": true,
                "clientActivityID": "abc123",
                "attachmentSizes": [],
                "feature": "menu",
                "rag": {"citations": [{"id": "c1"}]}
            }
        });
        let raw = build_bot_activity_raw("submenu", None, Some(&extensions), None);
        assert_eq!(raw["channelData"]["feature"], "menu");
        assert_eq!(raw["channelData"]["rag"]["citations"][0]["id"], "c1");
        assert!(raw["channelData"].get("postBack").is_none());
        assert!(raw["channelData"].get("clientActivityID").is_none());
        assert!(raw["channelData"].get("attachmentSizes").is_none());
    }

    #[test]
    fn build_bot_activity_raw_drops_channel_data_when_only_click_artifacts_remain() {
        let extensions = json!({
            "channel_data": {
                "postBack": true,
                "clientActivityID": "abc123"
            }
        });
        let raw = build_bot_activity_raw("submenu", None, Some(&extensions), None);
        assert!(raw.get("channelData").is_none());
    }

    #[test]
    fn build_bot_activity_raw_forwards_envelope_attachments() {
        // Regression test for TASK-082 Bug 3 follow-up — envelope's top-level
        // `attachments: Vec<Attachment>` field must reach the DirectLine activity.
        let envelope_attachments = json!([
            {
                "contentType": "image/png",
                "contentUrl": "https://cdn.example.com/diagram.png",
                "name": "diagram.png",
            }
        ]);
        let raw = build_bot_activity_raw("see image", None, None, Some(&envelope_attachments));

        let attachments = raw["attachments"].as_array().expect("attachments array");
        assert_eq!(attachments.len(), 1);
        assert_eq!(attachments[0]["contentType"], "image/png");
        assert_eq!(
            attachments[0]["contentUrl"],
            "https://cdn.example.com/diagram.png"
        );
    }

    #[test]
    fn build_bot_activity_raw_merges_envelope_attachments_with_legacy_ac_and_extensions() {
        // All three sources present — expect legacy AC first, envelope attachments
        // second, extensions.attachments last. Deterministic order matters for
        // clients that rely on card-ordering semantics.
        let ac_json = r#"{"type":"AdaptiveCard","body":[]}"#;
        let envelope_attachments = json!([
            {"contentType": "image/png", "contentUrl": "https://e/1.png"}
        ]);
        let extensions = json!({
            "attachments": [
                {"contentType": "application/vnd.microsoft.card.hero", "content": {"title": "H"}}
            ]
        });

        let raw = build_bot_activity_raw(
            "multi",
            Some(ac_json),
            Some(&extensions),
            Some(&envelope_attachments),
        );

        let attachments = raw["attachments"].as_array().expect("attachments array");
        assert_eq!(attachments.len(), 3);
        assert_eq!(
            attachments[0]["contentType"],
            "application/vnd.microsoft.card.adaptive"
        );
        assert_eq!(attachments[1]["contentType"], "image/png");
        assert_eq!(
            attachments[2]["contentType"],
            "application/vnd.microsoft.card.hero"
        );
    }

    #[test]
    fn build_bot_activity_raw_ignores_non_array_envelope_attachments() {
        // Defensive: if upstream emits a malformed non-array value, drop silently.
        let envelope_attachments = json!({"this": "is not an array"});
        let raw = build_bot_activity_raw("x", None, None, Some(&envelope_attachments));
        assert!(raw.get("attachments").is_none());
    }

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

    #[test]
    fn candidate_conversation_contexts_falls_back_between_default_and_teamless() {
        let with_default = DirectLineContext {
            env: "default".into(),
            tenant: "demo".into(),
            team: Some("default".into()),
        };
        let teamless = DirectLineContext {
            env: "default".into(),
            tenant: "demo".into(),
            team: None,
        };

        assert_eq!(
            candidate_conversation_contexts(&with_default),
            vec![with_default.clone(), teamless.clone()]
        );
        assert_eq!(
            candidate_conversation_contexts(&teamless),
            vec![teamless, with_default]
        );
    }

    #[test]
    fn append_bot_activity_uses_existing_teamless_conversation_for_default_team_payload() {
        let mut store = MemoryStateStore::default();
        let existing_ctx = DirectLineContext {
            env: "default".into(),
            tenant: "demo".into(),
            team: None,
        };
        let requested_ctx = DirectLineContext {
            env: "default".into(),
            tenant: "demo".into(),
            team: Some("default".into()),
        };
        let conversation_id = "conv-1";
        let key = conversation_key(&existing_ctx, conversation_id);
        let state = crate::directline::state::ConversationState::new(existing_ctx.clone());
        store
            .write(&key, &serde_json::to_vec(&state).unwrap())
            .unwrap();

        append_bot_activity_to_conversation_with_store(
            &mut store,
            &requested_ctx,
            conversation_id,
            "hello",
            None,
            None,
            None,
        )
        .unwrap();

        let bytes = store.read(&key).unwrap().expect("updated conversation");
        let updated: crate::directline::state::ConversationState =
            serde_json::from_slice(&bytes).unwrap();
        assert_eq!(updated.activities.len(), 1);
        assert_eq!(updated.activities[0].text.as_deref(), Some("hello"));
        assert_eq!(
            updated.activities[0].raw["from"]["id"].as_str(),
            Some("bot")
        );
    }
}
