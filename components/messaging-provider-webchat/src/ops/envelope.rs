//! `ChannelMessageEnvelope` constructors for inbound webchat events.
//!
//! Two variants exist:
//! - `build_webchat_envelope_with_ctx` — used by the Direct Line ingest path,
//!   carries explicit env/tenant IDs that were validated from the JWT.
//! - `build_webchat_envelope` — used by the fallback generic HTTP ingest path,
//!   defaults env/tenant to "default".

use greentic_types::{Actor, ChannelMessageEnvelope, EnvId, MessageMetadata, TenantCtx, TenantId};
use serde_json::Value;
use std::collections::BTreeMap;

/// Build envelope with explicit env/tenant context (used by Direct Line ingest).
pub(super) fn build_webchat_envelope_with_ctx(
    text: String,
    user_id: Option<String>,
    session_id: Option<String>,
    tenant_channel_id: Option<String>,
    env_id: &str,
    tenant_id: &str,
    extensions: BTreeMap<String, Value>,
) -> ChannelMessageEnvelope {
    let env =
        EnvId::try_from(env_id).unwrap_or_else(|_| EnvId::try_from("default").expect("env id"));
    let tenant = TenantId::try_from(tenant_id)
        .unwrap_or_else(|_| TenantId::try_from("default").expect("tenant id"));
    let mut metadata = MessageMetadata::new();
    metadata.insert("universal".to_string(), "true".to_string());
    metadata.insert("env".to_string(), env_id.to_string());
    metadata.insert("tenant".to_string(), tenant_id.to_string());
    if let Some(channel) = &tenant_channel_id {
        metadata.insert("tenant_channel_id".to_string(), channel.clone());
    }
    let channel = session_id
        .clone()
        .or_else(|| tenant_channel_id.clone())
        .unwrap_or_else(|| "webchat".to_string());
    if let Some(sid) = &session_id {
        metadata.insert("route".to_string(), sid.clone());
    }
    if !extensions.is_empty() {
        if let Ok(serialized) = serde_json::to_string(&extensions) {
            metadata.insert("extensions".to_string(), serialized);
        }
        for (key, value) in &extensions {
            let rendered = match value {
                Value::String(s) => s.clone(),
                other => serde_json::to_string(other).unwrap_or_default(),
            };
            metadata.insert(format!("channel.{key}"), rendered);
        }
    }
    ChannelMessageEnvelope {
        id: format!("webchat-{channel}"),
        tenant: TenantCtx::new(env.clone(), tenant.clone()),
        channel: channel.clone(),
        session_id: channel,
        reply_scope: None,
        from: user_id.map(|id| Actor {
            id,
            kind: Some("user".into()),
        }),
        to: Vec::new(),
        correlation_id: None,
        text: Some(text),
        attachments: Vec::new(),
        metadata,
        extensions: Default::default(),
    }
}

pub(super) fn build_webchat_envelope(
    text: String,
    user_id: Option<String>,
    route: Option<String>,
    tenant_channel_id: Option<String>,
) -> ChannelMessageEnvelope {
    let env = EnvId::try_from("default").expect("env id");
    let tenant = TenantId::try_from("default").expect("tenant id");
    let mut metadata = MessageMetadata::new();
    metadata.insert("universal".to_string(), "true".to_string());
    if let Some(route) = &route {
        metadata.insert("route".to_string(), route.clone());
    }
    if let Some(channel) = &tenant_channel_id {
        metadata.insert("tenant_channel_id".to_string(), channel.clone());
    }
    let channel = route
        .clone()
        .or_else(|| tenant_channel_id.clone())
        .unwrap_or_else(|| "webchat".to_string());
    ChannelMessageEnvelope {
        id: format!("webchat-{channel}"),
        tenant: TenantCtx::new(env.clone(), tenant.clone()),
        channel: channel.clone(),
        session_id: channel,
        reply_scope: None,
        from: user_id.map(|id| Actor {
            id,
            kind: Some("user".into()),
        }),
        to: Vec::new(),
        correlation_id: None,
        text: Some(text),
        attachments: Vec::new(),
        metadata,
        extensions: Default::default(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn extensions_are_flattened_into_channel_prefixed_metadata_entries() {
        let mut extensions = BTreeMap::new();
        extensions.insert(
            "channel_data".into(),
            json!({"r1_principals": {"user": "u1", "groups": ["admin"]}}),
        );
        extensions.insert("input_hint".into(), Value::String("acceptingInput".into()));

        let envelope = build_webchat_envelope_with_ctx(
            "hello".into(),
            Some("user-1".into()),
            Some("session-1".into()),
            Some("tenant-channel".into()),
            "dev",
            "demo",
            extensions,
        );

        assert!(
            envelope.metadata.contains_key("extensions"),
            "metadata.extensions JSON blob retained for back-compat consumers"
        );

        let channel_data_entry = envelope
            .metadata
            .get("channel.channel_data")
            .expect("channel_data flattened to metadata.channel.channel_data");
        let parsed: serde_json::Value = serde_json::from_str(channel_data_entry)
            .expect("flattened channel_data is valid JSON for non-string values");
        assert_eq!(
            parsed.pointer("/r1_principals/user"),
            Some(&Value::String("u1".into()))
        );

        assert_eq!(
            envelope
                .metadata
                .get("channel.input_hint")
                .map(String::as_str),
            Some("acceptingInput")
        );
    }
}
