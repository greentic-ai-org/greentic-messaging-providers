//! Legacy `run`/`send` op: persists an outbound webchat message into the
//! host state store. Used for the non-universal, direct-invocation path.
//!
//! Also contains the small deterministic hashing helpers used to derive
//! pseudo-UUID message IDs from the payload contents.

use provider_common::helpers::json_bytes;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};

use crate::PROVIDER_TYPE;
use crate::config::load_config;
use crate::directline::HostStateStore;
use crate::directline::store::StateStore as _;

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

    let route = parsed
        .get("route")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string())
        .or_else(|| cfg.route.clone());
    let tenant_channel_id = parsed
        .get("tenant_channel_id")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string())
        .or_else(|| cfg.tenant_channel_id.clone());

    if route.is_none() && tenant_channel_id.is_none() {
        return json_bytes(&json!({"ok": false, "error": "route or tenant_channel_id required"}));
    }

    let text = parsed
        .get("text")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();
    if text.is_empty() {
        return json_bytes(&json!({"ok": false, "error": "text required"}));
    }

    let payload = json!({
        "route": route,
        "tenant_channel_id": tenant_channel_id,
        "public_base_url": cfg.public_base_url,
        "mode": cfg.mode,
        "base_url": cfg.base_url,
        "text": text,
    });
    let payload_bytes = json_bytes(&payload);
    let key = route
        .clone()
        .or(tenant_channel_id.clone())
        .unwrap_or_else(|| "webchat".to_string());

    let mut state_store = HostStateStore;
    let write_result = state_store.write(&key, &payload_bytes);
    if let Err(err) = write_result {
        return json_bytes(&json!({"ok": false, "error": err}));
    }

    let hash_hex = hex_sha256(&payload_bytes);
    let message_id = pseudo_uuid_from_hex(&hash_hex);
    let provider_message_id = format!("webchat:{hash_hex}");

    json_bytes(&json!({
        "ok": true,
        "status": "sent",
        "provider_type": PROVIDER_TYPE,
        "public_base_url": cfg.public_base_url,
        "message_id": message_id,
        "provider_message_id": provider_message_id,
        "payload": payload
    }))
}

fn hex_sha256(bytes: &[u8]) -> String {
    let digest = Sha256::digest(bytes);
    let mut out = String::with_capacity(digest.len() * 2);
    for byte in digest {
        use std::fmt::Write;
        let _ = write!(&mut out, "{:02x}", byte);
    }
    out
}

fn pseudo_uuid_from_hex(hex: &str) -> String {
    let padded = if hex.len() < 32 {
        format!("{hex:0<32}")
    } else {
        hex[..32].to_string()
    };
    format!(
        "{}-{}-{}-{}-{}",
        &padded[0..8],
        &padded[8..12],
        &padded[12..16],
        &padded[16..20],
        &padded[20..32]
    )
}
