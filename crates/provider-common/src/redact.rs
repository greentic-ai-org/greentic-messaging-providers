//! Redaction helpers for telemetry.
//!
//! Provider components emit logs and spans that may be exported to remote
//! collectors. Anything passing through these helpers is sanitized so we never
//! leak tokens, user identifiers, or third-party response bodies while still
//! preserving enough structure for diagnostics (length, type, fingerprint).
//!
//! The helpers are intentionally string-based and allocation-light so they can
//! be called from `wasm32-wasip2` provider components.

use sha2::{Digest, Sha256};

/// Maximum byte length to emit for any redacted snippet.
const SNIPPET_MAX: usize = 64;

/// Replaces a credential value with a short fingerprint that retains its tail
/// for cross-referencing while never exposing the secret.
///
/// Output shape: `"sha256:<8 hex>...<tail>"`. Empty/very short tokens collapse
/// to `"<redacted>"`.
pub fn token(value: &str) -> String {
    let trimmed = value.trim();
    if trimmed.len() < 8 {
        return "<redacted>".to_string();
    }
    let digest = short_sha256(trimmed.as_bytes());
    let tail_len = trimmed.len().min(4);
    let tail = &trimmed[trimmed.len() - tail_len..];
    format!("sha256:{digest}...{tail}")
}

/// Redacts a user identifier (email, phone, opaque user id).
///
/// - Email `name@domain` becomes `<sha8>@domain`.
/// - Phone numbers (anything starting with `+` or 7+ digits) become
///   `tel:<sha8>` with the trailing 2 digits preserved.
/// - Anything else becomes `id:<sha8>`.
pub fn user_id(value: &str) -> String {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return "<redacted>".to_string();
    }

    if let Some((local, domain)) = trimmed.split_once('@') {
        let digest = short_sha256(local.as_bytes());
        return format!("{digest}@{domain}");
    }

    let digit_count = trimmed.chars().filter(|c| c.is_ascii_digit()).count();
    if trimmed.starts_with('+') || digit_count >= 7 {
        let digest = short_sha256(trimmed.as_bytes());
        let tail: String = trimmed
            .chars()
            .rev()
            .take(2)
            .collect::<Vec<_>>()
            .into_iter()
            .rev()
            .collect();
        return format!("tel:{digest}..{tail}");
    }

    let digest = short_sha256(trimmed.as_bytes());
    format!("id:{digest}")
}

/// Summarises a message body so the log carries shape without content.
///
/// Returns `"<len=N sha=ABCDEF12>"` — useful for correlating retries of the
/// same payload while never leaking text.
pub fn body(value: &str) -> String {
    let len = value.len();
    let digest = short_sha256(value.as_bytes());
    format!("<len={len} sha={digest}>")
}

/// Truncates an arbitrary downstream HTTP response body for logging.
///
/// Keeps the first [`SNIPPET_MAX`] bytes (cut at a char boundary), appends a
/// fingerprint of the full payload, and notes the original length. Vendor
/// responses sometimes echo tokens or user content, so callers should only use
/// this for diagnostics — never for forwarding.
pub fn response_snippet(value: &str) -> String {
    let len = value.len();
    let digest = short_sha256(value.as_bytes());
    if len <= SNIPPET_MAX {
        return format!("body=\"{}\" len={len} sha={digest}", escape(value));
    }
    let mut end = SNIPPET_MAX;
    while end > 0 && !value.is_char_boundary(end) {
        end -= 1;
    }
    let head = &value[..end];
    format!("body=\"{}...\" len={len} sha={digest}", escape(head))
}

/// Sanitises a free-form error string before it reaches telemetry.
///
/// Strips common credential-bearing substrings (`Bearer …`, `token=…`,
/// `key=…`) so that an error surfaced by an upstream library cannot smuggle a
/// secret into a log message.
pub fn error_message(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    let mut rest = value;
    while let Some(idx) = find_secret_marker(rest) {
        out.push_str(&rest[..idx.start]);
        out.push_str(idx.replacement);
        rest = &rest[idx.end..];
    }
    out.push_str(rest);
    out
}

struct Marker<'a> {
    start: usize,
    end: usize,
    replacement: &'a str,
}

fn find_secret_marker(haystack: &str) -> Option<Marker<'static>> {
    const PATTERNS: &[(&str, &str)] = &[
        ("Bearer ", "Bearer <redacted>"),
        ("bearer ", "bearer <redacted>"),
        ("token=", "token=<redacted>"),
        ("Token=", "Token=<redacted>"),
        ("api_key=", "api_key=<redacted>"),
        ("apikey=", "apikey=<redacted>"),
        ("authorization=", "authorization=<redacted>"),
        ("password=", "password=<redacted>"),
    ];

    for (needle, replacement) in PATTERNS {
        if let Some(start) = haystack.find(needle) {
            let after = start + needle.len();
            let mut end = after;
            for (i, ch) in haystack[after..].char_indices() {
                if ch.is_whitespace() || ch == '&' || ch == '"' || ch == '\'' || ch == ',' {
                    break;
                }
                end = after + i + ch.len_utf8();
            }
            return Some(Marker {
                start,
                end,
                replacement,
            });
        }
    }
    None
}

fn short_sha256(bytes: &[u8]) -> String {
    let digest = Sha256::digest(bytes);
    let mut s = String::with_capacity(8);
    for byte in &digest[..4] {
        use std::fmt::Write as _;
        let _ = write!(s, "{byte:02x}");
    }
    s
}

fn escape(value: &str) -> String {
    value
        .chars()
        .flat_map(|c| match c {
            '"' => vec!['\\', '"'],
            '\\' => vec!['\\', '\\'],
            '\n' => vec!['\\', 'n'],
            '\r' => vec!['\\', 'r'],
            '\t' => vec!['\\', 't'],
            c if c.is_control() => format!("\\x{:02x}", c as u32).chars().collect(),
            c => vec![c],
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn token_hides_value_but_keeps_tail() {
        let red = token("ya29.A0ARrdaM_secret_value_xyz1234");
        assert!(red.starts_with("sha256:"));
        assert!(red.ends_with("1234"));
        assert!(!red.contains("secret"));
    }

    #[test]
    fn short_tokens_fully_redacted() {
        assert_eq!(token("abc"), "<redacted>");
        assert_eq!(token(""), "<redacted>");
    }

    #[test]
    fn email_keeps_domain() {
        let red = user_id("alice@example.com");
        assert!(red.ends_with("@example.com"));
        assert!(!red.contains("alice"));
    }

    #[test]
    fn phone_keeps_last_two() {
        let red = user_id("+14155551234");
        assert!(red.starts_with("tel:"));
        assert!(red.ends_with("..34"));
    }

    #[test]
    fn opaque_id_is_hashed() {
        let red = user_id("U07ABC");
        assert!(red.starts_with("id:"));
        assert!(!red.contains("U07ABC"));
    }

    #[test]
    fn body_emits_length_and_hash() {
        let red = body("hello world");
        assert!(red.contains("len=11"));
        assert!(red.contains("sha="));
        assert!(!red.contains("hello"));
    }

    #[test]
    fn response_snippet_truncates_long_bodies() {
        let big = "x".repeat(500);
        let red = response_snippet(&big);
        assert!(red.contains("len=500"));
        assert!(red.contains("..."));
    }

    #[test]
    fn error_message_strips_bearer() {
        let red = error_message("failed: Bearer abc.def.ghi expired");
        assert!(red.contains("Bearer <redacted>"));
        assert!(!red.contains("abc.def.ghi"));
        assert!(red.contains("expired"));
    }

    #[test]
    fn error_message_strips_token_param() {
        let red = error_message("got 401 with token=xyz123&user=42");
        assert!(red.contains("token=<redacted>"));
        assert!(red.contains("user=42"));
    }
}
