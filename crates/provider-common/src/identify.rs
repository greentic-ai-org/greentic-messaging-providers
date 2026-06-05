//! Shared `identify-instance` body-extraction protocol.
//!
//! Providers exporting `greentic:provider-instance-identity@0.1.0` extract
//! the per-instance discriminator from the inbound payload. The host wraps
//! the request as `{ "headers": [...], "body": <parsed JSON | null> }` per
//! M1 IID.4d, but older hosts / direct test harnesses still pass the raw
//! body. The body itself may also arrive as a base64 string in `body_b64`
//! or as a JSON array of u8 bytes in `body` (HttpInV1-style wrappers).
//!
//! [`extract_from_wrapper`] handles all four shapes uniformly, so each
//! provider's identify implementation collapses to one call passing only
//! the leaf field extractor.

use serde_json::Value;

/// Run `leaf` against the inbound payload in this order, returning the
/// first `Some` match:
///
/// 1. The top-level value (legacy bare body), or
/// 2. The wrapper's `body` field if it is an object (M1 IID.4d shape), or
/// 3. The bytes decoded from `body` (JSON array of u8) or `body_b64`
///    (base64 string), parsed as JSON.
///
/// Returns `None` when no shape yields a match or the input is not JSON.
/// Per the WIT contract, `None` lets the host fall through to the
/// admit-table single-instance fallback (or poison to Ambiguous in
/// multi-endpoint revisions).
pub fn extract_from_wrapper<F>(input_json: &[u8], leaf: F) -> Option<String>
where
    F: Fn(&Value) -> Option<String>,
{
    let value: Value = serde_json::from_slice(input_json).ok()?;
    if let Some(id) = leaf(&value) {
        return Some(id);
    }
    if let Some(body) = value.get("body").filter(|b| b.is_object()) {
        return leaf(body);
    }
    let bytes = http_body_bytes(&value)?;
    let parsed: Value = serde_json::from_slice(&bytes).ok()?;
    leaf(&parsed)
}

/// Decode the wrapper's `body` field as raw bytes when the host did not
/// supply a pre-parsed JSON object. Accepts `body: [u8, u8, ...]` (JSON
/// array of bytes) or `body_b64: "<base64>"`.
pub fn http_body_bytes(value: &Value) -> Option<Vec<u8>> {
    if let Some(Value::Array(arr)) = value.get("body") {
        return arr.iter().map(|v| u8::try_from(v.as_u64()?).ok()).collect();
    }
    let b64 = value.get("body_b64").and_then(Value::as_str)?;
    use base64::Engine;
    base64::engine::general_purpose::STANDARD.decode(b64).ok()
}

/// Inert test helpers shared by every provider's `identify.rs` regression
/// suite. Always-available so downstream `#[cfg(test)]` modules can call
/// them from `cargo test`; the linker removes them from release binaries
/// as dead code.
pub mod test_utils {
    use super::Value;
    use base64::Engine;

    /// Base64-encode a JSON `Value`'s wire bytes — convenience for tests
    /// asserting the `body_b64` wrapper branch.
    pub fn b64_body(body: &Value) -> String {
        base64::engine::general_purpose::STANDARD.encode(serde_json::to_vec(body).unwrap())
    }

    /// Parse `IDENTIFY_HINT_JSON` bytes and assert the canonical shape:
    /// `{"version": 1, "sources": [...]}` with a non-empty `sources` array.
    /// Used by every provider's identify.rs hint-validation regression test.
    pub fn assert_valid_hint(hint_json: &[u8]) {
        let value: Value = serde_json::from_slice(hint_json).expect("parse hint");
        assert_eq!(value.get("version").and_then(Value::as_u64), Some(1));
        let sources = value
            .get("sources")
            .and_then(Value::as_array)
            .expect("sources array");
        assert!(!sources.is_empty(), "hint sources must be non-empty");
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn id_from_recipient(value: &Value) -> Option<String> {
        value
            .get("recipient")
            .and_then(|r| r.get("id"))
            .and_then(Value::as_str)
            .map(str::to_owned)
    }

    #[test]
    fn matches_top_level_legacy_bare_body() {
        let bytes = serde_json::to_vec(&json!({ "recipient": { "id": "bot-1" } })).unwrap();
        assert_eq!(
            extract_from_wrapper(&bytes, id_from_recipient).as_deref(),
            Some("bot-1")
        );
    }

    #[test]
    fn matches_m1_iid4d_wrapper_with_decoded_body_object() {
        let bytes = serde_json::to_vec(&json!({
            "headers": [{ "name": "x-forwarded-for", "value": "203.0.113.42" }],
            "body": { "recipient": { "id": "bot-2" } }
        }))
        .unwrap();
        assert_eq!(
            extract_from_wrapper(&bytes, id_from_recipient).as_deref(),
            Some("bot-2")
        );
    }

    #[test]
    fn matches_httpinv1_wrapper_with_body_b64() {
        let body = json!({ "recipient": { "id": "bot-3" } });
        let bytes = serde_json::to_vec(&json!({
            "headers": [],
            "body_b64": test_utils::b64_body(&body)
        }))
        .unwrap();
        assert_eq!(
            extract_from_wrapper(&bytes, id_from_recipient).as_deref(),
            Some("bot-3")
        );
    }

    #[test]
    fn matches_httpinv1_wrapper_with_body_byte_array() {
        let body_bytes = serde_json::to_vec(&json!({ "recipient": { "id": "bot-4" } })).unwrap();
        let bytes = serde_json::to_vec(&json!({
            "headers": [],
            "body": body_bytes.iter().map(|b| *b as u64).collect::<Vec<_>>()
        }))
        .unwrap();
        assert_eq!(
            extract_from_wrapper(&bytes, id_from_recipient).as_deref(),
            Some("bot-4")
        );
    }

    #[test]
    fn returns_none_when_leaf_finds_nothing() {
        let bytes = serde_json::to_vec(&json!({ "other": "data" })).unwrap();
        assert!(extract_from_wrapper(&bytes, id_from_recipient).is_none());
    }

    #[test]
    fn returns_none_for_unparseable_input() {
        assert!(extract_from_wrapper(b"not json", id_from_recipient).is_none());
    }

    #[test]
    fn returns_none_when_body_b64_is_garbage() {
        let bytes = serde_json::to_vec(&json!({
            "headers": [],
            "body_b64": "@@@not-base64@@@"
        }))
        .unwrap();
        assert!(extract_from_wrapper(&bytes, id_from_recipient).is_none());
    }
}
