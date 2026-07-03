//! Twilio `X-Twilio-Signature` verification.
//!
//! Twilio scheme: `base64(HMAC_SHA1(auth_token, url + concat(sorted_key + value)))`
//! over the exact URL Twilio POSTed to plus every POST param, sorted by key and
//! concatenated as `key + value` with no separators.

use base64::{Engine as _, engine::general_purpose::STANDARD};
use hmac::{Hmac, KeyInit, Mac};
use std::collections::BTreeMap;
// Twilio's signing scheme is defined as HMAC-SHA1; changing this breaks provider verification.
// foxguard: ignore[rs/no-weak-hash]
use sha1::Sha1;

pub(crate) fn valid_twilio_signature(
    auth_token: &str,
    url: &str,
    params: &BTreeMap<String, String>,
    header_sig: &str,
) -> bool {
    match compute_signature(auth_token, url, params) {
        Some(expected) => constant_time_eq(expected.as_bytes(), header_sig.as_bytes()),
        None => false,
    }
}

fn compute_signature(
    auth_token: &str,
    url: &str,
    params: &BTreeMap<String, String>,
) -> Option<String> {
    let mut signed_data = String::from(url);
    for (key, value) in params {
        signed_data.push_str(key);
        signed_data.push_str(value);
    }
    // foxguard: ignore[rs/no-weak-hash]
    let mut mac = Hmac::<Sha1>::new_from_slice(auth_token.as_bytes()).ok()?;
    mac.update(signed_data.as_bytes());
    Some(STANDARD.encode(mac.finalize().into_bytes()))
}

fn constant_time_eq(actual: &[u8], expected: &[u8]) -> bool {
    if actual.len() != expected.len() {
        return false;
    }
    actual
        .iter()
        .zip(expected)
        .fold(0u8, |acc, (a, b)| acc | (a ^ b))
        == 0
}

/// Test-only signer, shared with `ops::ingest`'s tests so both suites build
/// signed fixtures against the exact same wire format without duplicating it.
#[cfg(test)]
pub(crate) fn sign_for_test(
    auth_token: &str,
    url: &str,
    params: &BTreeMap<String, String>,
) -> String {
    compute_signature(auth_token, url, params).expect("hmac accepts any key length")
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Independent reference implementation of Twilio's signing algorithm,
    /// written directly against the hmac/sha1 crates (not calling
    /// `compute_signature`) so this test actually exercises the wire format.
    fn expected_sig(token: &str, url: &str, params: &BTreeMap<String, String>) -> String {
        let mut data = url.to_string();
        for (key, value) in params {
            data.push_str(key);
            data.push_str(value);
        }
        let mut mac = Hmac::<Sha1>::new_from_slice(token.as_bytes()).expect("hmac key");
        mac.update(data.as_bytes());
        STANDARD.encode(mac.finalize().into_bytes())
    }

    #[test]
    fn validates_known_twilio_signature() {
        let token = "12345";
        let url = "https://example.com/v1/messaging/ingress/sms/t1";
        let mut params = BTreeMap::new();
        params.insert("Body".to_string(), "hi".to_string());
        params.insert("From".to_string(), "+15551230001".to_string());
        let expected = expected_sig(token, url, &params);
        assert!(valid_twilio_signature(token, url, &params, &expected));
        assert!(!valid_twilio_signature(token, url, &params, "tampered=="));
    }

    #[test]
    fn rejects_signature_computed_over_a_different_url() {
        let token = "12345";
        let signed_url = "https://example.com/v1/messaging/ingress/sms/t1";
        let other_url = "https://example.com/v1/messaging/ingress/sms/t2";
        let mut params = BTreeMap::new();
        params.insert("Body".to_string(), "hi".to_string());
        let sig = expected_sig(token, signed_url, &params);
        assert!(!valid_twilio_signature(token, other_url, &params, &sig));
    }

    #[test]
    fn rejects_signature_when_a_param_is_tampered() {
        let token = "12345";
        let url = "https://example.com/v1/messaging/ingress/sms/t1";
        let mut params = BTreeMap::new();
        params.insert("Body".to_string(), "hi".to_string());
        let sig = expected_sig(token, url, &params);
        params.insert("Body".to_string(), "hijacked".to_string());
        assert!(!valid_twilio_signature(token, url, &params, &sig));
    }

    #[test]
    fn empty_header_signature_never_matches() {
        let params = BTreeMap::new();
        assert!(!valid_twilio_signature(
            "12345",
            "https://example.com/x",
            &params,
            ""
        ));
    }
}
