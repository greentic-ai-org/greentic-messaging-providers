//! Small value-extraction helpers shared across webchat ops modules.
//!
//! These are pure functions that inspect `serde_json::Value` trees and return
//! trimmed/optional strings. They have no I/O, no state, and no provider
//! dependencies — safe to use from any ops submodule.

use base64::{Engine as _, engine::general_purpose};
use greentic_types::messaging::Attachment;
use serde_json::{Map, Value};

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

/// Convert a `ChannelMessageEnvelope::Attachment` into a DirectLine-shaped JSON
/// attachment object (`{contentType, contentUrl, name?}`).
///
/// Envelope attachments carry the platform's generic shape; DirectLine clients
/// (WebChat SDK, Bot Framework) expect camelCase keys and `contentUrl` rather
/// than `url`. Inline content (e.g. Adaptive Cards) is delivered through the
/// separate `extensions` / `metadata.adaptive_card` paths, not this helper.
pub(crate) fn attachment_to_directline(attachment: &Attachment) -> Value {
    let mut map = Map::new();
    map.insert(
        "contentType".to_string(),
        Value::String(attachment.mime_type.clone()),
    );
    map.insert(
        "contentUrl".to_string(),
        Value::String(attachment.url.clone()),
    );
    if let Some(name) = &attachment.name {
        map.insert("name".to_string(), Value::String(name.clone()));
    }
    Value::Object(map)
}

/// Convert a slice of envelope attachments into a JSON array ready to embed in
/// payload bodies or DirectLine activities.
pub(crate) fn envelope_attachments_to_directline(attachments: &[Attachment]) -> Value {
    Value::Array(attachments.iter().map(attachment_to_directline).collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_attachment() -> Attachment {
        Attachment {
            mime_type: "image/png".to_string(),
            url: "https://cdn.example.com/img.png".to_string(),
            name: Some("diagram.png".to_string()),
            size_bytes: Some(1024),
        }
    }

    #[test]
    fn attachment_to_directline_maps_known_fields() {
        let directline = attachment_to_directline(&sample_attachment());
        assert_eq!(directline["contentType"], "image/png");
        assert_eq!(directline["contentUrl"], "https://cdn.example.com/img.png");
        assert_eq!(directline["name"], "diagram.png");
        // size_bytes is not a DirectLine attachment field — intentionally dropped.
        assert!(directline.get("sizeBytes").is_none());
        assert!(directline.get("size_bytes").is_none());
    }

    #[test]
    fn attachment_to_directline_omits_missing_name() {
        let mut att = sample_attachment();
        att.name = None;
        let directline = attachment_to_directline(&att);
        assert!(directline.get("name").is_none());
    }

    #[test]
    fn envelope_attachments_to_directline_preserves_order() {
        let attachments = vec![
            Attachment {
                mime_type: "image/png".to_string(),
                url: "https://e/1".to_string(),
                name: None,
                size_bytes: None,
            },
            Attachment {
                mime_type: "application/pdf".to_string(),
                url: "https://e/2".to_string(),
                name: Some("doc.pdf".to_string()),
                size_bytes: None,
            },
        ];
        let arr = envelope_attachments_to_directline(&attachments);
        let items = arr.as_array().expect("array");
        assert_eq!(items.len(), 2);
        assert_eq!(items[0]["contentType"], "image/png");
        assert_eq!(items[1]["name"], "doc.pdf");
    }
}
