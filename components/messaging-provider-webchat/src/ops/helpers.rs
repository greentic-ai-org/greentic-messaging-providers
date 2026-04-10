//! Small value-extraction helpers shared across webchat ops modules.
//!
//! These are pure functions that inspect `serde_json::Value` trees and return
//! trimmed/optional strings. They have no I/O, no state, and no provider
//! dependencies — safe to use from any ops submodule.

use base64::{Engine as _, engine::general_purpose};
use serde_json::Value;

pub(super) fn extract_text(value: &Value) -> String {
    value
        .get("text")
        .or_else(|| value.get("message"))
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string()
}

pub(super) fn extract_activity_text(value: &Value) -> String {
    let direct = extract_text(value);
    if !direct.is_empty() {
        return direct;
    }
    value
        .get("activity")
        .and_then(|activity| activity.get("text").or_else(|| activity.get("message")))
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string()
}

pub(super) fn decode_body_json(body_b64: &str) -> Option<Value> {
    if body_b64.trim().is_empty() {
        return None;
    }
    let decoded = general_purpose::STANDARD
        .decode(body_b64)
        .or_else(|_| general_purpose::URL_SAFE.decode(body_b64))
        .or_else(|_| general_purpose::URL_SAFE_NO_PAD.decode(body_b64))
        .ok()?;
    serde_json::from_slice::<Value>(&decoded).ok()
}

pub(super) fn user_from_value(value: &Value) -> Option<String> {
    value
        .get("user_id")
        .or_else(|| value.get("from"))
        .and_then(|v| v.as_str())
        .and_then(|s| non_empty_string(Some(s)))
}

pub(super) fn route_from_value(value: &Value) -> Option<String> {
    value_as_trimmed_string(value.get("route"))
}

pub(super) fn tenant_channel_from_value(value: &Value) -> Option<String> {
    value_as_trimmed_string(value.get("tenant_channel_id"))
}

pub(super) fn public_base_url_from_value(value: &Value) -> Option<String> {
    value_as_trimmed_string(value.get("public_base_url"))
}

pub(super) fn value_as_trimmed_string(value: Option<&Value>) -> Option<String> {
    value
        .and_then(|v| v.as_str())
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(|s| s.to_string())
}

pub(super) fn non_empty_string(value: Option<&str>) -> Option<String> {
    value
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(|s| s.to_string())
}
