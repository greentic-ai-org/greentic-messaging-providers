use base64::{Engine, engine::general_purpose::STANDARD};
use greentic_types::messaging::universal_dto::{
    HttpInV1, HttpOutV1, ProviderPayloadV1, SendPayloadInV1,
};
use greentic_types::{
    Actor, Attachment, ChannelMessageEnvelope, Destination, EnvId, MessageMetadata, TenantCtx,
    TenantId,
};
use provider_common::helpers::{
    PlannerCapabilities, RenderPlanConfig, decode_encode_message, encode_error, json_bytes,
    render_plan_common, send_payload_error,
};
use provider_common::http_compat::{http_out_error, http_out_v1_bytes, parse_operator_http_in};
use serde_json::{Value, json};
use std::collections::BTreeMap;

use crate::bindings::greentic::http::http_client as client;
use crate::config::{
    build_send_envelope_from_input, detect_destination_kind, get_secret_string, get_token,
    load_config, override_config_from_metadata,
};
use crate::{DEFAULT_API_BASE, DEFAULT_TOKEN_KEY, PROVIDER_TYPE, ProviderConfig};

pub(crate) fn handle_send(input_json: &[u8]) -> Vec<u8> {
    let parsed: Value = match serde_json::from_slice(input_json) {
        Ok(val) => val,
        Err(err) => {
            return json_bytes(&json!({"ok": false, "error": format!("invalid json: {err}")}));
        }
    };

    let mut cfg = match load_config(&parsed) {
        Ok(cfg) => cfg,
        Err(err) => return json_bytes(&json!({"ok": false, "error": err})),
    };
    if !cfg.enabled {
        return json_bytes(&json!({"ok": false, "error": "provider disabled by config"}));
    }

    let envelope: ChannelMessageEnvelope = match serde_json::from_slice(input_json) {
        Ok(env) => env,
        Err(err) => match build_send_envelope_from_input(&parsed, &cfg) {
            Ok(env) => env,
            Err(message) => {
                return json_bytes(
                    &json!({"ok": false, "error": format!("invalid envelope: {message}: {err}")}),
                );
            }
        },
    };

    override_config_from_metadata(&mut cfg, &envelope.metadata);

    println!(
        "webex encoded envelope {}",
        serde_json::to_string(&envelope).unwrap_or_default()
    );
    if !envelope.attachments.is_empty() {
        return json_bytes(&json!({"ok": false, "error": "attachments not supported"}));
    }

    let text = envelope
        .text
        .as_ref()
        .map(|value| value.trim())
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned);
    let text = match text {
        Some(value) => value,
        None => return json_bytes(&json!({"ok": false, "error": "text required"})),
    };

    let destination = envelope.to.first().cloned().or_else(|| {
        cfg.default_to_person_email
            .clone()
            .map(|email| Destination {
                id: email,
                kind: Some("email".into()),
            })
    });
    println!("webex envelope to={:?}", envelope.to);
    let destination = match destination {
        Some(dest) => dest,
        None => return json_bytes(&json!({"ok": false, "error": "destination required"})),
    };

    let dest_id = destination.id.trim();
    if dest_id.is_empty() {
        return json_bytes(&json!({"ok": false, "error": "destination id required"}));
    }
    let dest_id = dest_id.to_string();
    let kind = destination
        .kind
        .as_deref()
        .unwrap_or_else(|| detect_destination_kind(&dest_id));

    // Check for AC card in metadata — send as native Webex attachment.
    let card_payload = envelope
        .metadata
        .get("adaptive_card")
        .and_then(|ac_raw| serde_json::from_str::<Value>(ac_raw).ok());
    let markdown_value = card_payload
        .as_ref()
        .and_then(summarize_card_text)
        .unwrap_or_else(|| text.clone());
    // Reply-in-thread: check for reply_to_id or parentId.
    let parent_id = parsed
        .get("reply_to_id")
        .and_then(Value::as_str)
        .or_else(|| parsed.get("parentId").and_then(Value::as_str))
        .or_else(|| envelope.metadata.get("reply_to_id").map(|s| s.as_str()))
        .map(|s| s.trim_matches('"'))
        .filter(|s| !s.is_empty())
        .map(|s| s.to_string());

    let api_base = cfg
        .api_base_url
        .clone()
        .unwrap_or_else(|| DEFAULT_API_BASE.to_string());
    let url = format!("{}/messages", api_base);
    let mut body_map = build_webex_body(card_payload.as_ref(), Some(&text), &markdown_value);
    if let Some(pid) = &parent_id {
        body_map.insert("parentId".into(), Value::String(pid.clone()));
    }
    let mut body = Value::Object(body_map);
    let body_obj = body.as_object_mut().expect("body object");
    match kind {
        "room" => {
            body_obj.insert("roomId".into(), Value::String(dest_id));
        }
        "person" | "user" => {
            body_obj.insert("toPersonId".into(), Value::String(dest_id));
        }
        "email" | "" => {
            body_obj.insert("toPersonEmail".into(), Value::String(dest_id));
        }
        other => {
            return json_bytes(&json!({
                "ok": false,
                "error": format!("unsupported destination kind: {other}")
            }));
        }
    }

    let token = match get_token(&cfg) {
        Ok(token) => token,
        Err(err) => return json_bytes(&json!({"ok": false, "error": err})),
    };

    println!(
        "webex send url={} body={}",
        url,
        serde_json::to_string(&body).unwrap_or_default()
    );
    let request = client::Request {
        method: "POST".into(),
        url,
        headers: vec![
            ("Content-Type".into(), "application/json".into()),
            ("Authorization".into(), format!("Bearer {token}")),
        ],
        body: Some(serde_json::to_vec(&body).unwrap_or_else(|_| b"{}".to_vec())),
    };

    let resp = match client::send(&request, None, None) {
        Ok(resp) => resp,
        Err(err) => {
            return json_bytes(
                &json!({"ok": false, "error": format!("transport error: {}", err.message)}),
            );
        }
    };

    if resp.status < 200 || resp.status >= 300 {
        let err_body = resp.body.unwrap_or_default();
        let detail = format_webex_error(resp.status, &err_body);
        return json_bytes(&json!({"ok": false, "error": detail}));
    }

    let body_bytes = resp.body.unwrap_or_default();
    let body_json: Value = serde_json::from_slice(&body_bytes).unwrap_or(Value::Null);
    let msg_id = body_json
        .get("id")
        .and_then(|v| v.as_str())
        .unwrap_or("webex-message")
        .to_string();
    let provider_message_id = format!("webex:{msg_id}");

    json_bytes(&json!({
        "ok": true,
        "status": "sent",
        "provider_type": PROVIDER_TYPE,
        "public_base_url": cfg.public_base_url,
        "message_id": msg_id,
        "provider_message_id": provider_message_id,
        "response": body_json
    }))
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

    let text = parsed
        .get("text")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();
    if text.is_empty() {
        return json_bytes(&json!({"ok": false, "error": "text required"}));
    }
    let thread_id = parsed
        .get("reply_to_id")
        .or_else(|| parsed.get("thread_id"))
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();
    if thread_id.is_empty() {
        return json_bytes(&json!({"ok": false, "error": "reply_to_id or thread_id required"}));
    }

    let token = match get_token(&cfg) {
        Ok(token) => token,
        Err(err) => return json_bytes(&json!({"ok": false, "error": err})),
    };
    let api_base = cfg
        .api_base_url
        .clone()
        .unwrap_or_else(|| DEFAULT_API_BASE.to_string());
    let url = format!("{}/messages", api_base);
    let payload = json!({
        "parentId": thread_id,
        "markdown": text,
    });
    let request = client::Request {
        method: "POST".into(),
        url,
        headers: vec![
            ("Content-Type".into(), "application/json".into()),
            ("Authorization".into(), format!("Bearer {token}")),
        ],
        body: Some(serde_json::to_vec(&payload).unwrap_or_else(|_| b"{}".to_vec())),
    };

    let resp = match client::send(&request, None, None) {
        Ok(resp) => resp,
        Err(err) => {
            return json_bytes(&json!({
                "ok": false,
                "error": format!("transport error: {}", err.message),
            }));
        }
    };
    if resp.status < 200 || resp.status >= 300 {
        return json_bytes(&json!({
            "ok": false,
            "error": format!("webex returned status {}", resp.status),
        }));
    }
    let body_bytes = resp.body.unwrap_or_default();
    let body_json: Value = serde_json::from_slice(&body_bytes).unwrap_or(Value::Null);
    let msg_id = body_json
        .get("id")
        .and_then(|v| v.as_str())
        .unwrap_or("webex-reply")
        .to_string();
    let provider_message_id = format!("webex:{msg_id}");

    json_bytes(&json!({
        "ok": true,
        "status": "replied",
        "provider_type": PROVIDER_TYPE,
        "public_base_url": cfg.public_base_url,
        "message_id": msg_id,
        "provider_message_id": provider_message_id,
        "response": body_json
    }))
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

pub(crate) fn render_plan(input_json: &[u8]) -> Vec<u8> {
    render_plan_common(
        input_json,
        &RenderPlanConfig {
            capabilities: PlannerCapabilities {
                supports_adaptive_cards: true,
                supports_markdown: true,
                supports_html: true,
                supports_images: true,
                supports_buttons: false,
                max_text_len: None,
                max_payload_bytes: None,
            },
            default_summary: "webex message",
        },
    )
}

pub(crate) fn encode_op(input_json: &[u8]) -> Vec<u8> {
    let envelope = match decode_encode_message(input_json) {
        Ok(value) => value,
        Err(err) => return encode_error(&err),
    };
    let body_bytes = serde_json::to_vec(&envelope).unwrap_or_else(|_| b"{}".to_vec());
    let payload = ProviderPayloadV1 {
        content_type: "application/json".to_string(),
        body_b64: STANDARD.encode(&body_bytes),
        metadata: BTreeMap::new(),
    };
    json_bytes(&json!({"ok": true, "payload": payload}))
}

pub(crate) fn send_payload(input_json: &[u8]) -> Vec<u8> {
    let send_in = match serde_json::from_slice::<SendPayloadInV1>(input_json) {
        Ok(value) => value,
        Err(err) => {
            return send_payload_error(&format!("invalid send_payload input: {err}"), false);
        }
    };
    if send_in.provider_type != PROVIDER_TYPE {
        return send_payload_error("provider type mismatch", false);
    }
    let ProviderPayloadV1 {
        content_type,
        body_b64,
        metadata,
    } = send_in.payload;
    let api_base = metadata
        .get("api_base_url")
        .and_then(|value| value.as_str())
        .map(|s| s.to_string())
        .unwrap_or_else(|| DEFAULT_API_BASE.to_string());
    let url = format!("{}/messages", api_base);
    let method = metadata
        .get("method")
        .and_then(|value| value.as_str())
        .map(|s| s.to_string())
        .unwrap_or_else(|| "POST".to_string());
    let body_bytes = match STANDARD.decode(&body_b64) {
        Ok(bytes) => bytes,
        Err(err) => return send_payload_error(&format!("payload decode failed: {err}"), false),
    };
    let envelope = match serde_json::from_slice::<ChannelMessageEnvelope>(&body_bytes) {
        Ok(env) => env,
        Err(err) => {
            eprintln!("webex send_payload invalid envelope: {err}");
            return send_payload_error(&format!("invalid envelope: {err}"), false);
        }
    };
    if !envelope.attachments.is_empty() {
        eprintln!(
            "webex send_payload rejected attachments {:?}",
            envelope.attachments
        );
        return send_payload_error("attachments not supported", false);
    }
    let text = envelope
        .text
        .as_ref()
        .map(|value| value.trim())
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned);
    let card_payload = envelope
        .metadata
        .get("adaptive_card")
        .and_then(|value| serde_json::from_str::<Value>(value).ok());
    let card_summary = card_payload.as_ref().and_then(summarize_card_text);
    if card_payload.is_none() && text.is_none() {
        eprintln!(
            "webex send_payload missing text envelope metadata={:?}",
            envelope.metadata
        );
        return send_payload_error("text required", false);
    }
    let destination = envelope.to.first().cloned().or_else(|| {
        metadata
            .get("default_to_person_email")
            .and_then(|value| value.as_str())
            .map(|s| Destination {
                id: s.to_string(),
                kind: Some("email".into()),
            })
    });
    let destination = match destination {
        Some(dest) => dest,
        None => {
            return send_payload_error(
                &format!("destination required (envelope to={:?})", envelope.to),
                false,
            );
        }
    };
    let dest_id = destination.id.trim();
    if dest_id.is_empty() {
        return send_payload_error("destination id required", false);
    }
    // Reply-in-thread: check for reply_to_id / parentId in envelope metadata.
    let parent_id = envelope
        .metadata
        .get("reply_to_id")
        .or_else(|| envelope.metadata.get("parentId"))
        .map(|s| s.trim_matches('"'))
        .filter(|s| !s.is_empty())
        .map(|s| s.to_string());

    let summary_text = text.clone().or(card_summary.clone());
    let markdown_value = summary_text.clone().unwrap_or_else(|| " ".to_string());
    let mut body_map = build_webex_body(card_payload.as_ref(), text.as_ref(), &markdown_value);
    if let Some(pid) = &parent_id {
        body_map.insert("parentId".into(), Value::String(pid.clone()));
    }
    let kind = destination
        .kind
        .as_deref()
        .unwrap_or_else(|| detect_destination_kind(dest_id));
    match kind {
        "room" => {
            body_map.insert("roomId".into(), Value::String(dest_id.to_string()));
        }
        "person" | "user" => {
            body_map.insert("toPersonId".into(), Value::String(dest_id.to_string()));
        }
        "email" | "" => {
            body_map.insert("toPersonEmail".into(), Value::String(dest_id.to_string()));
        }
        other => {
            return send_payload_error(&format!("unsupported destination kind: {other}"), false);
        }
    }
    let body_req = Value::Object(body_map);
    println!(
        "webex send url={}/messages body={}",
        api_base,
        serde_json::to_string(&body_req).unwrap_or_default()
    );
    let token = match get_secret_string(DEFAULT_TOKEN_KEY) {
        Ok(value) => value,
        Err(err) => return send_payload_error(&err, false),
    };
    let request = client::Request {
        method,
        url,
        headers: vec![
            ("Content-Type".into(), content_type.clone()),
            ("Authorization".into(), format!("Bearer {token}")),
        ],
        body: Some(serde_json::to_vec(&body_req).unwrap_or_else(|_| b"{}".to_vec())),
    };
    let resp = match client::send(&request, None, None) {
        Ok(value) => value,
        Err(err) => {
            return send_payload_error(&format!("transport error: {}", err.message), true);
        }
    };
    if resp.status < 200 || resp.status >= 300 {
        let body = resp.body.unwrap_or_default();
        let detail = format_webex_error(resp.status, &body);
        return send_payload_error(&detail, resp.status >= 500);
    }
    // Forward message_id so callers can use it for replies/threading.
    let resp_body = resp.body.unwrap_or_default();
    let resp_json: Value = serde_json::from_slice(&resp_body).unwrap_or(Value::Null);
    let msg_id = resp_json
        .get("id")
        .and_then(Value::as_str)
        .unwrap_or_default();
    json_bytes(&json!({
        "ok": true,
        "message": msg_id,
        "retryable": false
    }))
}

pub(crate) fn summarize_card_text(card: &Value) -> Option<String> {
    if let Some(text) = card
        .get("text")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|t| !t.is_empty())
    {
        return Some(text.to_string());
    }

    if let Some(body_array) = card.get("body").and_then(Value::as_array) {
        let mut segments = Vec::new();
        for block in body_array {
            if let Some(text) = block.get("text").and_then(Value::as_str) {
                let trimmed = text.trim();
                if !trimmed.is_empty() {
                    segments.push(trimmed.to_string());
                }
            }
        }
        if !segments.is_empty() {
            return Some(segments.join(" "));
        }
    }

    None
}

pub(crate) fn build_webex_body(
    card_payload: Option<&Value>,
    text_value: Option<&String>,
    markdown: &str,
) -> serde_json::Map<String, Value> {
    let mut map = serde_json::Map::new();
    if let Some(card) = card_payload {
        // Webex supports AC up to v1.3 — cap the version.
        let mut card = card.clone();
        if let Some(obj) = card.as_object_mut() {
            let ver = obj.get("version").and_then(Value::as_str).unwrap_or("1.0");
            if ver != "1.0" && ver != "1.1" && ver != "1.2" && ver != "1.3" {
                obj.insert("version".into(), Value::String("1.3".to_string()));
            }
        }
        let attachment = json!({
            "contentType": "application/vnd.microsoft.card.adaptive",
            "content": card,
        });
        map.insert("attachments".into(), Value::Array(vec![attachment]));
    } else if let Some(text_val) = text_value {
        map.insert("text".into(), Value::String(text_val.clone()));
    }
    map.insert("markdown".into(), Value::String(markdown.to_string()));
    map
}

pub(crate) fn format_webex_error(status: u16, body: &[u8]) -> String {
    let trimmed = String::from_utf8_lossy(body).trim().to_string();
    if trimmed.is_empty() {
        format!("webex returned status {}", status)
    } else {
        format!("webex returned status {} body={}", status, trimmed)
    }
}

pub(crate) struct IngestOutcome {
    pub(crate) envelope: ChannelMessageEnvelope,
    pub(crate) status: u16,
    pub(crate) error: Option<String>,
}

pub(crate) struct MessageDetails {
    pub(crate) markdown: Option<String>,
    pub(crate) text: Option<String>,
    pub(crate) room_id: Option<String>,
    pub(crate) person_email: Option<String>,
    pub(crate) person_id: Option<String>,
    pub(crate) attachments: Vec<Attachment>,
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

/// Fetch Webex attachment action details to retrieve user inputs.
fn fetch_action_details(action_id: &str, api_base: &str, token: &str) -> Result<Value, String> {
    let url = format!("{api_base}/attachment/actions/{action_id}");
    println!("webex ingest fetching action {action_id} from {url}");
    let request = client::Request {
        method: "GET".to_string(),
        url: url.clone(),
        headers: vec![("Authorization".into(), format!("Bearer {token}"))],
        body: None,
    };
    let resp = client::send(&request, None, None)
        .map_err(|err| format!("transport error: {}", err.message))?;
    println!(
        "webex ingest fetch action {action_id} status={}",
        resp.status
    );
    if resp.status < 200 || resp.status >= 300 {
        let body = resp.body.unwrap_or_default();
        return Err(format_webex_error(resp.status, &body));
    }
    let body = resp.body.unwrap_or_default();
    let parsed: Value =
        serde_json::from_slice(&body).map_err(|err| format!("json parse error: {err}"))?;
    Ok(parsed.get("inputs").cloned().unwrap_or(json!({})))
}

fn fetch_message_details(
    message_id: &str,
    api_base: &str,
    token: &str,
) -> Result<MessageDetails, String> {
    let url = format!("{api_base}/messages/{message_id}");
    println!("webex ingest fetching message {message_id} from {url}");
    let request = client::Request {
        method: "GET".to_string(),
        url: url.clone(),
        headers: vec![("Authorization".into(), format!("Bearer {token}"))],
        body: None,
    };
    let resp = client::send(&request, None, None)
        .map_err(|err| format!("transport error: {}", err.message))?;
    println!("webex ingest fetch {message_id} status={}", resp.status);
    if resp.status < 200 || resp.status >= 300 {
        let body = resp.body.unwrap_or_default();
        return Err(format_webex_error(resp.status, &body));
    }
    let body = resp.body.unwrap_or_default();
    let message_json: Value =
        serde_json::from_slice(&body).map_err(|err| format!("invalid message JSON: {err}"))?;
    let data = message_json
        .get("result")
        .cloned()
        .unwrap_or_else(|| message_json.clone());
    let attachments = convert_webex_attachments(message_id, &data);
    Ok(MessageDetails {
        markdown: data
            .get("markdown")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string()),
        text: data
            .get("text")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string()),
        room_id: data
            .get("roomId")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string()),
        person_email: data
            .get("personEmail")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string()),
        person_id: data
            .get("personId")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string()),
        attachments,
    })
}

fn convert_webex_attachments(message_id: &str, data: &Value) -> Vec<Attachment> {
    data.get("attachments")
        .and_then(Value::as_array)
        .map(|array| {
            array
                .iter()
                .enumerate()
                .filter_map(|(idx, attachment)| build_webex_attachment(message_id, idx, attachment))
                .collect()
        })
        .unwrap_or_default()
}

fn build_webex_attachment(message_id: &str, idx: usize, value: &Value) -> Option<Attachment> {
    let mime_type = value
        .get("contentType")
        .and_then(|v| v.as_str())
        .unwrap_or("application/octet-stream")
        .to_string();
    let url = value
        .get("contentUrl")
        .and_then(|v| v.as_str())
        .or_else(|| {
            value
                .get("content")
                .and_then(|content| content.get("url"))
                .and_then(|v| v.as_str())
        })
        .map(|s| s.to_string())
        .unwrap_or_else(|| format!("webex:{message_id}:attachment:{idx}"));
    let name = value
        .get("name")
        .or_else(|| value.get("displayName"))
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());
    let size_bytes = value
        .get("size")
        .and_then(|v| v.as_u64())
        .or_else(|| value.get("sizeBytes").and_then(|v| v.as_u64()));
    Some(Attachment {
        mime_type,
        url,
        name,
        size_bytes,
    })
}

#[allow(clippy::too_many_arguments)]
fn build_webhook_metadata(
    resource: &str,
    event: &str,
    message_id: Option<&String>,
    room_id: Option<&String>,
    person_email: Option<&String>,
    person_id: Option<&String>,
    error: Option<&String>,
    attachment_types: Option<String>,
    default_locale: Option<&String>,
    status: Option<u16>,
) -> MessageMetadata {
    let mut metadata = MessageMetadata::new();
    metadata.insert("webex.resource".to_string(), resource.to_string());
    metadata.insert("webex.event".to_string(), event.to_string());
    if let Some(msg) = message_id {
        metadata.insert("webex.messageId".to_string(), msg.clone());
    }
    if let Some(room) = room_id {
        metadata.insert("webex.roomId".to_string(), room.clone());
    }
    if let Some(email) = person_email {
        metadata.insert("webex.personEmail".to_string(), email.clone());
    }
    if let Some(id) = person_id {
        metadata.insert("webex.personId".to_string(), id.clone());
    }
    if let Some(err) = error {
        metadata.insert("webex.ingestError".to_string(), err.clone());
    }
    if let Some(status) = status {
        metadata.insert("webex.fetchStatus".to_string(), status.to_string());
    }
    metadata.insert(
        "webex.hasAttachments".to_string(),
        attachment_types.is_some().to_string(),
    );
    if let Some(types) = attachment_types {
        metadata.insert("webex.attachmentTypes".to_string(), types);
    }
    // Webex doesn't include locale in webhooks; use provider config default.
    if let Some(locale) = default_locale
        && !locale.is_empty()
    {
        metadata.insert("locale".to_string(), locale.clone());
    }
    metadata
}

fn build_webhook_envelope(
    text: String,
    session_id: String,
    from: Option<Actor>,
    metadata: MessageMetadata,
    attachments: Vec<Attachment>,
    message_id: Option<&String>,
) -> ChannelMessageEnvelope {
    let env = EnvId::try_from("default").expect("env id");
    let tenant = TenantId::try_from("default").expect("tenant id");
    let destinations = if !session_id.is_empty() {
        vec![Destination {
            id: session_id.clone(),
            kind: None,
        }]
    } else {
        Vec::new()
    };
    ChannelMessageEnvelope {
        id: message_id
            .map(|id| format!("webex-{id}"))
            .unwrap_or_else(|| format!("webex-ingress-{session_id}")),
        tenant: TenantCtx::new(env.clone(), tenant.clone()),
        channel: "webex".to_string(),
        session_id: session_id.clone(),
        reply_scope: None,
        from,
        to: destinations,
        correlation_id: None,
        text: Some(text),
        attachments,
        metadata,
    }
}

fn pick_sender(person_email: &Option<String>, person_id: &Option<String>) -> Option<Actor> {
    if let Some(email) = person_email {
        return Some(Actor {
            id: email.clone(),
            kind: Some("person".into()),
        });
    }
    if let Some(id) = person_id {
        return Some(Actor {
            id: id.clone(),
            kind: Some("person".into()),
        });
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use greentic_types::messaging::universal_dto::{ProviderPayloadV1, SendPayloadInV1};
    use greentic_types::{Attachment, EnvId, TenantCtx, TenantId};
    use std::collections::BTreeMap;

    fn envelope() -> ChannelMessageEnvelope {
        ChannelMessageEnvelope {
            id: "msg-1".to_string(),
            tenant: TenantCtx::new(
                EnvId::try_from("default").expect("env"),
                TenantId::try_from("default").expect("tenant"),
            ),
            channel: "webex".to_string(),
            session_id: "session-1".to_string(),
            reply_scope: None,
            from: None,
            to: vec![Destination {
                id: "person@example.com".to_string(),
                kind: Some("email".to_string()),
            }],
            correlation_id: None,
            text: Some("hello".to_string()),
            attachments: Vec::new(),
            metadata: MessageMetadata::new(),
        }
    }

    fn with_config(mut value: Value) -> Vec<u8> {
        let obj = value.as_object_mut().expect("object");
        obj.insert(
            "public_base_url".to_string(),
            Value::String("https://example.com".to_string()),
        );
        obj.insert("bot_token".to_string(), Value::String("token".to_string()));
        serde_json::to_vec(&value).expect("bytes")
    }

    fn parse_json(bytes: Vec<u8>) -> Value {
        serde_json::from_slice(&bytes).expect("json")
    }

    fn result_message(value: &Value) -> Option<&str> {
        value
            .get("error")
            .and_then(Value::as_str)
            .or_else(|| value.get("message").and_then(Value::as_str))
    }

    #[test]
    fn handle_send_rejects_attachments_and_missing_destination() {
        let mut with_attachment = serde_json::to_value(envelope()).expect("value");
        with_attachment.as_object_mut().expect("object").insert(
            "attachments".to_string(),
            serde_json::to_value(vec![Attachment {
                mime_type: "image/png".to_string(),
                url: "https://example.com/image.png".to_string(),
                name: None,
                size_bytes: None,
            }])
            .expect("attachments"),
        );
        let body = parse_json(handle_send(&with_config(with_attachment)));
        assert_eq!(
            body.get("error").and_then(Value::as_str),
            Some("attachments not supported")
        );

        let mut no_destination = serde_json::to_value(envelope()).expect("value");
        no_destination
            .as_object_mut()
            .expect("object")
            .insert("to".to_string(), Value::Array(Vec::new()));
        let body = parse_json(handle_send(&with_config(no_destination)));
        assert_eq!(
            body.get("error").and_then(Value::as_str),
            Some("destination required")
        );
    }

    #[test]
    fn handle_reply_requires_text_and_thread_id() {
        let body = parse_json(handle_reply(&with_config(json!({
            "reply_to_id": "thread-1"
        }))));
        assert_eq!(
            body.get("error").and_then(Value::as_str),
            Some("text required")
        );

        let body = parse_json(handle_reply(&with_config(json!({
            "text": "hello"
        }))));
        assert_eq!(
            body.get("error").and_then(Value::as_str),
            Some("reply_to_id or thread_id required")
        );
    }

    #[test]
    fn send_payload_rejects_provider_mismatch_and_attachments() {
        let mismatch = SendPayloadInV1 {
            provider_type: "other".to_string(),
            tenant_id: None,
            auth_user: None,
            payload: ProviderPayloadV1 {
                content_type: "application/json".to_string(),
                body_b64: STANDARD.encode(b"{}"),
                metadata: BTreeMap::new(),
            },
        };
        let body = parse_json(send_payload(
            &serde_json::to_vec(&mismatch).expect("payload bytes"),
        ));
        assert_eq!(result_message(&body), Some("provider type mismatch"));

        let mut envelope = envelope();
        envelope.attachments.push(Attachment {
            mime_type: "image/png".to_string(),
            url: "https://example.com/image.png".to_string(),
            name: None,
            size_bytes: None,
        });
        let attachments = SendPayloadInV1 {
            provider_type: PROVIDER_TYPE.to_string(),
            tenant_id: None,
            auth_user: None,
            payload: ProviderPayloadV1 {
                content_type: "application/json".to_string(),
                body_b64: STANDARD.encode(serde_json::to_vec(&envelope).expect("envelope bytes")),
                metadata: BTreeMap::new(),
            },
        };
        let body = parse_json(send_payload(
            &serde_json::to_vec(&attachments).expect("payload bytes"),
        ));
        assert_eq!(result_message(&body), Some("attachments not supported"));
    }
}

/// Handle `setup_webhook` op — manages Webex webhook registrations.
///
/// Creates or updates two webhooks for messages and attachment actions.
///
/// Input JSON:
/// ```json
/// {
///   "bot_token": "...",
///   "public_base_url": "https://example.ngrok-free.app",
///   "api_base_url": "https://webexapis.com/v1",  // optional
///   "provider_id": "messaging-webex",
///   "tenant": "demo",
///   "team": "default"
/// }
/// ```
pub(crate) fn setup_webhook(input_json: &[u8]) -> Vec<u8> {
    let parsed: Value = match serde_json::from_slice(input_json) {
        Ok(val) => val,
        Err(err) => {
            return json_bytes(&json!({"ok": false, "error": format!("invalid json: {err}")}));
        }
    };

    let bot_token = parsed
        .get("bot_token")
        .or_else(|| parsed.get("webex_bot_token"))
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|s| !s.is_empty());
    let bot_token = match bot_token {
        Some(t) => t,
        None => {
            return json_bytes(
                &json!({"ok": false, "error": "bot_token or webex_bot_token required"}),
            );
        }
    };

    let public_base_url = parsed
        .get("public_base_url")
        .and_then(Value::as_str)
        .unwrap_or("");
    if public_base_url.is_empty() || !public_base_url.starts_with("https://") {
        return json_bytes(
            &json!({"ok": false, "error": "public_base_url must be an https:// URL"}),
        );
    }

    let api_base = parsed
        .get("api_base_url")
        .and_then(Value::as_str)
        .filter(|s| !s.is_empty())
        .unwrap_or("https://webexapis.com/v1");
    let provider_id = parsed
        .get("provider_id")
        .and_then(Value::as_str)
        .unwrap_or("messaging-webex");
    let tenant = parsed
        .get("tenant")
        .and_then(Value::as_str)
        .unwrap_or("default");
    let team = parsed
        .get("team")
        .and_then(Value::as_str)
        .unwrap_or("default");

    let webhook_url = format!(
        "{}/v1/messaging/ingress/{}/{}/{}",
        public_base_url.trim_end_matches('/'),
        provider_id,
        tenant,
        team,
    );

    let base_name = format!("greentic:{tenant}:{team}:webex");

    // List existing webhooks
    let existing = match list_webhooks(api_base, bot_token) {
        Ok(list) => list,
        Err(err) => {
            return json_bytes(
                &json!({"ok": false, "error": format!("list webhooks failed: {err}")}),
            );
        }
    };

    // Reconcile two webhook types
    let webhook_specs = [
        (&base_name, "messages", "created"),
        (
            &format!("{base_name}:cards"),
            "attachmentActions",
            "created",
        ),
    ];
    let mut results = Vec::new();
    let mut all_ok = true;
    for (name, resource, event) in webhook_specs {
        let result = reconcile_webhook(
            api_base,
            bot_token,
            &existing,
            name,
            &webhook_url,
            resource,
            event,
        );
        let ok = result.get("ok").and_then(Value::as_bool).unwrap_or(false);
        if !ok {
            all_ok = false;
        }
        results.push(json!({
            "resource": resource,
            "event": event,
            "name": name,
            "result": result,
        }));
    }

    json_bytes(&json!({
        "ok": all_ok,
        "webhook_url": webhook_url,
        "webhooks": results,
    }))
}

fn list_webhooks(api_base: &str, token: &str) -> Result<Vec<Value>, String> {
    let resp = client::send(
        &client::Request {
            method: "GET".to_string(),
            url: format!("{api_base}/webhooks"),
            headers: vec![("Authorization".to_string(), format!("Bearer {token}"))],
            body: None,
        },
        None,
        None,
    )
    .map_err(|e| format!("http error: {}", e.message))?;
    let body: Value = serde_json::from_slice(&resp.body.unwrap_or_default()).unwrap_or(Value::Null);
    Ok(body
        .get("items")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default())
}

fn reconcile_webhook(
    api_base: &str,
    token: &str,
    existing: &[Value],
    name: &str,
    target_url: &str,
    resource: &str,
    event: &str,
) -> Value {
    // Find existing webhook by name
    let found = existing.iter().find(|wh| {
        wh.get("name")
            .and_then(Value::as_str)
            .map(|n| n == name)
            .unwrap_or(false)
    });

    match found {
        Some(wh) => {
            let current_url = wh.get("targetUrl").and_then(Value::as_str).unwrap_or("");
            let webhook_id = wh.get("id").and_then(Value::as_str).unwrap_or("");
            if current_url == target_url {
                json!({"ok": true, "action": "noop", "webhook_id": webhook_id})
            } else {
                update_webhook(api_base, token, webhook_id, name, target_url)
            }
        }
        None => create_webhook(api_base, token, name, target_url, resource, event),
    }
}

fn create_webhook(
    api_base: &str,
    token: &str,
    name: &str,
    target_url: &str,
    resource: &str,
    event: &str,
) -> Value {
    let body = json!({
        "name": name,
        "targetUrl": target_url,
        "resource": resource,
        "event": event,
    });
    let resp = match client::send(
        &client::Request {
            method: "POST".to_string(),
            url: format!("{api_base}/webhooks"),
            headers: vec![
                ("Authorization".to_string(), format!("Bearer {token}")),
                ("Content-Type".to_string(), "application/json".to_string()),
            ],
            body: Some(serde_json::to_vec(&body).unwrap_or_default()),
        },
        None,
        None,
    ) {
        Ok(r) => r,
        Err(e) => return json!({"ok": false, "error": format!("create failed: {}", e.message)}),
    };
    let resp_body: Value =
        serde_json::from_slice(&resp.body.unwrap_or_default()).unwrap_or(Value::Null);
    let ok = (200..300).contains(&(resp.status as u32));
    let webhook_id = resp_body.get("id").and_then(Value::as_str).unwrap_or("");
    json!({
        "ok": ok,
        "action": "create",
        "webhook_id": webhook_id,
        "http_status": resp.status,
    })
}

fn update_webhook(
    api_base: &str,
    token: &str,
    webhook_id: &str,
    name: &str,
    target_url: &str,
) -> Value {
    let body = json!({
        "name": name,
        "targetUrl": target_url,
    });
    let resp = match client::send(
        &client::Request {
            method: "PUT".to_string(),
            url: format!("{api_base}/webhooks/{webhook_id}"),
            headers: vec![
                ("Authorization".to_string(), format!("Bearer {token}")),
                ("Content-Type".to_string(), "application/json".to_string()),
            ],
            body: Some(serde_json::to_vec(&body).unwrap_or_default()),
        },
        None,
        None,
    ) {
        Ok(r) => r,
        Err(e) => return json!({"ok": false, "error": format!("update failed: {}", e.message)}),
    };
    let ok = (200..300).contains(&(resp.status as u32));
    json!({
        "ok": ok,
        "action": "update",
        "webhook_id": webhook_id,
        "http_status": resp.status,
    })
}
