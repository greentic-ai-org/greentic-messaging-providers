//! Inbound Webex webhook ingestion.
//!
//! `ingest_http` accepts either the native `HttpInV1` or the operator wrapper
//! format, normalises the body, and dispatches to [`handle_webhook_event`] to
//! produce a [`ChannelMessageEnvelope`]. The actual Webex API calls (fetch
//! message / fetch attachment actions) and envelope helpers live in
//! [`super::ingest_helpers`] to keep both files under the 500-line cap.

use base64::{Engine, engine::general_purpose::STANDARD};
use greentic_types::ChannelMessageEnvelope;
use greentic_types::messaging::universal_dto::{HttpInV1, HttpOutV1};
use provider_common::http_compat::{http_out_error, http_out_v1_bytes, parse_operator_http_in};
use serde_json::{Value, json};

use super::ingest_helpers::{
    build_webhook_envelope, build_webhook_metadata, fetch_action_details, fetch_message_details,
    pick_sender,
};
use crate::config::{get_secret_string, load_config};
use crate::{DEFAULT_API_BASE, DEFAULT_TOKEN_KEY, ProviderConfig};

pub(crate) struct IngestOutcome {
    pub(crate) envelope: ChannelMessageEnvelope,
    pub(crate) status: u16,
    pub(crate) error: Option<String>,
}

pub(crate) fn ingest_http(input_json: &[u8]) -> Vec<u8> {
    // Try native greentic-types format first, fall back to operator format
    let request = match serde_json::from_slice::<HttpInV1>(input_json) {
        Ok(req) => req,
        Err(_) => match parse_operator_http_in(input_json) {
            Ok(req) => req,
            Err(err) => return http_out_error(400, &format!("invalid http input: {err}")),
        },
    };
    let body_bytes = match STANDARD.decode(&request.body_b64) {
        Ok(bytes) => bytes,
        Err(err) => return http_out_error(400, &format!("invalid body encoding: {err}")),
    };
    let body_val: Value = serde_json::from_slice(&body_bytes).unwrap_or(Value::Null);
    let cfg = load_config(&json!({})).unwrap_or_default();
    let outcome = handle_webhook_event(&body_val, &cfg);

    let mut normalized = json!({
        "ok": outcome.error.is_none(),
        "event": body_val,
    });
    if let Some(err) = &outcome.error {
        normalized
            .as_object_mut()
            .map(|map| map.insert("error".into(), Value::String(err.clone())));
    }

    let normalized_bytes = serde_json::to_vec(&normalized).unwrap_or_else(|_| b"{}".to_vec());
    let out = HttpOutV1 {
        status: outcome.status,
        headers: Vec::new(),
        body_b64: STANDARD.encode(&normalized_bytes),
        events: vec![outcome.envelope],
    };
    http_out_v1_bytes(&out)
}

pub(crate) fn handle_webhook_event(body: &Value, cfg: &ProviderConfig) -> IngestOutcome {
    let resource = body
        .get("resource")
        .and_then(|s| s.as_str())
        .unwrap_or_default();
    let event = body
        .get("event")
        .and_then(|s| s.as_str())
        .unwrap_or_default();
    let data = body.get("data").unwrap_or(&Value::Null);
    let message_id = data
        .get("id")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());
    let webhook_room = data
        .get("roomId")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());
    let webhook_person_email = data
        .get("personEmail")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());
    let webhook_person_id = data
        .get("personId")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());

    // Handle Adaptive Card button clicks (Action.Submit).
    if resource == "attachmentActions" && event == "created" {
        let action_id = data.get("id").and_then(|v| v.as_str()).unwrap_or_default();
        if action_id.is_empty() {
            let metadata = build_webhook_metadata(
                resource,
                event,
                None,
                None,
                None,
                None,
                None,
                None,
                cfg.default_locale.as_ref(),
                Some(400),
            );
            let envelope = build_webhook_envelope(
                String::new(),
                "webex".into(),
                None,
                metadata,
                Vec::new(),
                None,
            );
            return IngestOutcome {
                envelope,
                status: 400,
                error: Some("attachmentActions missing action id".into()),
            };
        }
        let api_base = cfg
            .api_base_url
            .as_deref()
            .filter(|s| !s.trim().is_empty())
            .unwrap_or(DEFAULT_API_BASE)
            .trim_end_matches('/')
            .to_string();
        let session_id = webhook_room
            .clone()
            .unwrap_or_else(|| action_id.to_string());
        let sender = pick_sender(&webhook_person_email, &webhook_person_id);

        // Fetch action details to get user inputs.
        let inputs = match get_secret_string(DEFAULT_TOKEN_KEY) {
            Ok(token) => match fetch_action_details(action_id, &api_base, &token) {
                Ok(details) => details,
                Err(err) => {
                    eprintln!("webex fetch action details failed: {err}");
                    json!({})
                }
            },
            Err(_) => json!({}),
        };

        let action_id_str = action_id.to_string();
        let route_to_card = inputs
            .get("routeToCardId")
            .and_then(|v| v.as_str())
            .unwrap_or_default()
            .to_string();
        let card_id = inputs
            .get("cardId")
            .and_then(|v| v.as_str())
            .unwrap_or_default()
            .to_string();
        let action_text = if !route_to_card.is_empty() {
            format!("[card:{route_to_card}]")
        } else {
            format!("[action:{card_id}]")
        };

        let mut metadata = build_webhook_metadata(
            resource,
            event,
            Some(&action_id_str),
            webhook_room.as_ref(),
            webhook_person_email.as_ref(),
            webhook_person_id.as_ref(),
            None,
            None,
            cfg.default_locale.as_ref(),
            Some(200),
        );
        if !route_to_card.is_empty() {
            metadata.insert("routeToCardId".into(), route_to_card);
        }
        if !card_id.is_empty() {
            metadata.insert("cardId".into(), card_id);
        }
        // Forward ALL input fields to metadata for MCP routing
        if let Some(obj) = inputs.as_object() {
            for (k, v) in obj {
                let s = match v {
                    Value::String(s) => s.clone(),
                    _ => v.to_string(),
                };
                metadata.insert(k.clone(), s);
            }
        }
        metadata.insert(
            "webex.actionInputs".into(),
            serde_json::to_string(&inputs).unwrap_or_default(),
        );

        let envelope = build_webhook_envelope(
            action_text,
            session_id,
            sender,
            metadata,
            Vec::new(),
            Some(&action_id_str),
        );
        return IngestOutcome {
            envelope,
            status: 200,
            error: None,
        };
    }

    if resource == "messages"
        && event == "created"
        && let Some(message_id) = message_id.clone()
    {
        let api_base = cfg
            .api_base_url
            .as_deref()
            .filter(|s| !s.trim().is_empty())
            .unwrap_or(DEFAULT_API_BASE)
            .trim_end_matches('/')
            .to_string();
        match get_secret_string(DEFAULT_TOKEN_KEY) {
            Ok(token) => match fetch_message_details(&message_id, &api_base, &token) {
                Ok(details) => {
                    let session_id = details
                        .room_id
                        .clone()
                        .or(webhook_room.clone())
                        .unwrap_or_else(|| message_id.clone());
                    let sender = pick_sender(&details.person_email, &details.person_id)
                        .or_else(|| pick_sender(&webhook_person_email, &webhook_person_id));
                    let text = details
                        .markdown
                        .as_deref()
                        .filter(|value| !value.trim().is_empty())
                        .map(ToOwned::to_owned)
                        .or_else(|| details.text.clone())
                        .unwrap_or_default();
                    let attachment_types = if details.attachments.is_empty() {
                        None
                    } else {
                        Some(
                            details
                                .attachments
                                .iter()
                                .map(|a| a.mime_type.clone())
                                .collect::<Vec<_>>()
                                .join(","),
                        )
                    };
                    let metadata = build_webhook_metadata(
                        resource,
                        event,
                        Some(&message_id),
                        details.room_id.as_ref().or(webhook_room.as_ref()),
                        details
                            .person_email
                            .as_ref()
                            .or(webhook_person_email.as_ref()),
                        details.person_id.as_ref().or(webhook_person_id.as_ref()),
                        None,
                        attachment_types.clone(),
                        cfg.default_locale.as_ref(),
                        Some(200),
                    );
                    let envelope = build_webhook_envelope(
                        text,
                        session_id,
                        sender,
                        metadata,
                        details.attachments.clone(),
                        Some(&message_id),
                    );
                    return IngestOutcome {
                        envelope,
                        status: 200,
                        error: None,
                    };
                }
                Err(err) => {
                    println!("webex ingest fetch error for {message_id}: {err}");
                    let session_id = webhook_room.clone().unwrap_or_else(|| message_id.clone());
                    let sender = pick_sender(&webhook_person_email, &webhook_person_id);
                    let metadata = build_webhook_metadata(
                        resource,
                        event,
                        Some(&message_id),
                        webhook_room.as_ref(),
                        webhook_person_email.as_ref(),
                        webhook_person_id.as_ref(),
                        Some(&err),
                        None,
                        cfg.default_locale.as_ref(),
                        Some(502),
                    );
                    let envelope = build_webhook_envelope(
                        "".to_string(),
                        session_id,
                        sender,
                        metadata,
                        Vec::new(),
                        Some(&message_id),
                    );
                    return IngestOutcome {
                        envelope,
                        status: 502,
                        error: Some(err),
                    };
                }
            },
            Err(err) => {
                let session_id = webhook_room.clone().unwrap_or_else(|| message_id.clone());
                let sender = pick_sender(&webhook_person_email, &webhook_person_id);
                let metadata = build_webhook_metadata(
                    resource,
                    event,
                    Some(&message_id),
                    webhook_room.as_ref(),
                    webhook_person_email.as_ref(),
                    webhook_person_id.as_ref(),
                    Some(&err),
                    None,
                    cfg.default_locale.as_ref(),
                    Some(500),
                );
                let envelope = build_webhook_envelope(
                    "".to_string(),
                    session_id,
                    sender,
                    metadata,
                    Vec::new(),
                    Some(&message_id),
                );
                return IngestOutcome {
                    envelope,
                    status: 500,
                    error: Some(err),
                };
            }
        }
    }

    let text = body
        .get("text")
        .or_else(|| body.get("markdown"))
        .and_then(|v| v.as_str())
        .unwrap_or_default()
        .to_string();
    let session_id = webhook_room
        .clone()
        .unwrap_or_else(|| message_id.clone().unwrap_or_else(|| "webex".to_string()));
    let sender = pick_sender(&webhook_person_email, &webhook_person_id);
    let metadata = build_webhook_metadata(
        resource,
        event,
        message_id.as_ref(),
        webhook_room.as_ref(),
        webhook_person_email.as_ref(),
        webhook_person_id.as_ref(),
        None,
        None,
        cfg.default_locale.as_ref(),
        Some(200),
    );
    let envelope = build_webhook_envelope(
        text,
        session_id,
        sender,
        metadata,
        Vec::new(),
        message_id.as_ref(),
    );
    IngestOutcome {
        envelope,
        status: 200,
        error: None,
    }
}
