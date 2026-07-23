// HMAC-SHA256 GitHub webhook signature verification.
//
// Lifted from greentic-sorx `crates/greentic-sorx-core/src/ghcr_webhook.rs`
// (`verify_github_signature`, `GithubWebhookHeaders`, and the HMAC/hex helpers).
// Copied rather than depended upon: sorx is a separate deploy-plane crate. The
// error type is adapted to `String` to fit the ingress `handle-webhook` result.

use serde_json::{Map, Value};
use sha2::{Digest, Sha256};

/// The GitHub webhook headers this component reads.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GithubWebhookHeaders {
    pub signature_256: String,
    pub event: String,
    pub delivery: String,
}

impl GithubWebhookHeaders {
    /// Extract the relevant GitHub headers from a case-insensitive header map.
    pub fn from_map(headers: &Map<String, Value>) -> Self {
        Self {
            signature_256: header_value(headers, "x-hub-signature-256").unwrap_or_default(),
            event: header_value(headers, "x-github-event").unwrap_or_default(),
            delivery: header_value(headers, "x-github-delivery").unwrap_or_default(),
        }
    }
}

/// Verify an `X-Hub-Signature-256` value (`sha256=<hex>`) against the HMAC of
/// `body` keyed by `secret`. Constant-time comparison, no early return on the
/// digest bytes.
pub fn verify_github_signature(secret: &[u8], body: &[u8], signature: &str) -> Result<(), String> {
    let Some(hex_signature) = signature.strip_prefix("sha256=") else {
        return Err("validation error: X-Hub-Signature-256 must start with sha256=".to_string());
    };
    let expected = hmac_sha256_hex(secret, body);
    if constant_time_eq(expected.as_bytes(), hex_signature.as_bytes()) {
        Ok(())
    } else {
        Err("validation error: invalid signature".to_string())
    }
}

pub fn hmac_sha256_hex(secret: &[u8], body: &[u8]) -> String {
    const BLOCK_SIZE: usize = 64;
    let mut key = if secret.len() > BLOCK_SIZE {
        Sha256::digest(secret).to_vec()
    } else {
        secret.to_vec()
    };
    key.resize(BLOCK_SIZE, 0);

    let mut outer_key_pad = [0x5c; BLOCK_SIZE];
    let mut inner_key_pad = [0x36; BLOCK_SIZE];
    for (index, byte) in key.iter().enumerate() {
        outer_key_pad[index] ^= byte;
        inner_key_pad[index] ^= byte;
    }

    let mut inner = Sha256::new();
    inner.update(inner_key_pad);
    inner.update(body);
    let inner_hash = inner.finalize();

    let mut outer = Sha256::new();
    outer.update(outer_key_pad);
    outer.update(inner_hash);
    hex_lower(&outer.finalize())
}

fn constant_time_eq(left: &[u8], right: &[u8]) -> bool {
    if left.len() != right.len() {
        return false;
    }
    left.iter()
        .zip(right)
        .fold(0u8, |acc, (left, right)| acc | (left ^ right))
        == 0
}

fn hex_lower(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut out = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        out.push(HEX[(byte >> 4) as usize] as char);
        out.push(HEX[(byte & 0x0f) as usize] as char);
    }
    out
}

fn header_value(headers: &Map<String, Value>, key: &str) -> Option<String> {
    headers
        .get(key)
        .and_then(|v| v.as_str())
        .map(|s| s.to_string())
        .or_else(|| {
            headers
                .iter()
                .find(|(k, _)| k.eq_ignore_ascii_case(key))
                .and_then(|(_, v)| v.as_str())
                .map(|s| s.to_string())
        })
}
