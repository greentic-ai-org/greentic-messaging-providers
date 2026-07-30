// This is the ES256/JWKS verification primitive for Webchat SSO; the
// request-handling wiring that calls it lands in a follow-up task, so
// its public API is unused (outside tests) until then.
#![allow(dead_code)]

use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
use p256::ecdsa::{Signature, VerifyingKey, signature::Verifier};
use serde_json::Value;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VerifiedIdentity {
    pub sub: String,
    pub email: Option<String>,
    pub idp: String,
    pub tenant_id: String,
}

#[derive(Debug, PartialEq, Eq)]
pub enum OidcVerifyError {
    Malformed,
    UnsupportedAlg,
    NoKid,
    UnknownKid,
    BadKey,
    InvalidSignature,
    Expired,
    MissingClaim(&'static str),
    TenantMismatch,
    JwksFetch(String),
    JwksParse,
}

/// The JWKS host is the security boundary; the path is not. Always derive
/// `<scheme>://<host[:port]>/jwks.json` from the issuer, never append to its path.
pub fn jwks_url_for_issuer(iss: &str) -> Result<String, OidcVerifyError> {
    let rest = iss
        .strip_prefix("https://")
        .ok_or(OidcVerifyError::Malformed)?;
    let authority = rest.split('/').next().unwrap_or("");
    if authority.is_empty() {
        return Err(OidcVerifyError::Malformed);
    }
    Ok(format!("https://{authority}/jwks.json"))
}

fn decode_segment(seg: &str) -> Result<Value, OidcVerifyError> {
    let bytes = URL_SAFE_NO_PAD
        .decode(seg)
        .map_err(|_| OidcVerifyError::Malformed)?;
    serde_json::from_slice(&bytes).map_err(|_| OidcVerifyError::Malformed)
}

fn claim_str<'a>(claims: &'a Value, key: &'static str) -> Result<&'a str, OidcVerifyError> {
    claims
        .get(key)
        .and_then(Value::as_str)
        .ok_or(OidcVerifyError::MissingClaim(key))
}

/// Verifies a compact ES256 JWT bearer token and returns the identity it
/// asserts, scoped to `expected_tenant`. `fetch_jwks` is called with the
/// JWKS URL derived from the token's `iss` claim and must return the raw
/// JWKS JSON body (or an `Err(reason)` that becomes `JwksFetch(reason)`).
///
/// # Issuer trust contract
/// The JWKS URL is derived from the token's OWN, still-unverified `iss`
/// claim (via [`jwks_url_for_issuer`]). This function has no notion of
/// which issuers are legitimate for `expected_tenant` — an attacker who
/// stands up their own OIDC provider can mint a token with
/// `iss = <attacker-controlled issuer>` and `tenant_id = <victim tenant>`,
/// and this function will fetch the attacker's JWKS and verify it
/// successfully, since the signature genuinely matches that (attacker's)
/// key.
///
/// This function does **not** establish issuer trust; it only proves the
/// token is well-formed, signed by whatever key its own JWKS advertises,
/// unexpired, and tenant-tagged as claimed. The caller (or `fetch_jwks`
/// itself) MUST validate the derived issuer/URL against a tenant-pinned
/// issuer allowlist and refuse to fetch (return `Err` from the closure)
/// for any issuer not on that allowlist, before this function is even
/// called or before the closure performs the network fetch. That
/// enforcement is deliberately out of scope here and lands in the
/// request-handling wiring task.
pub fn verify_oidc_bearer<F>(
    token: &str,
    expected_tenant: &str,
    now_unix: i64,
    fetch_jwks: F,
) -> Result<VerifiedIdentity, OidcVerifyError>
where
    F: FnOnce(&str) -> Result<String, String>,
{
    let mut parts = token.split('.');
    let header_seg = parts.next().ok_or(OidcVerifyError::Malformed)?;
    let payload_seg = parts.next().ok_or(OidcVerifyError::Malformed)?;
    let sig_seg = parts.next().ok_or(OidcVerifyError::Malformed)?;
    if parts.next().is_some() {
        return Err(OidcVerifyError::Malformed);
    }

    let header = decode_segment(header_seg)?;
    if header.get("alg").and_then(Value::as_str) != Some("ES256") {
        return Err(OidcVerifyError::UnsupportedAlg);
    }
    let kid = header
        .get("kid")
        .and_then(Value::as_str)
        .ok_or(OidcVerifyError::NoKid)?;

    let claims = decode_segment(payload_seg)?;
    let iss = claim_str(&claims, "iss")?.to_string();
    let sub = claim_str(&claims, "sub")?.to_string();
    let tenant_id = claim_str(&claims, "tenant_id")?.to_string();
    let exp = claims
        .get("exp")
        .and_then(Value::as_i64)
        .ok_or(OidcVerifyError::MissingClaim("exp"))?;
    let email = claims
        .get("email")
        .and_then(Value::as_str)
        .map(str::to_string);

    // Fetch JWKS from the issuer host root and locate the signing key by kid.
    let jwks_url = jwks_url_for_issuer(&iss)?;
    let jwks_body = fetch_jwks(&jwks_url).map_err(OidcVerifyError::JwksFetch)?;
    let jwks: Value = serde_json::from_str(&jwks_body).map_err(|_| OidcVerifyError::JwksParse)?;
    let jwk = jwks
        .get("keys")
        .and_then(Value::as_array)
        .and_then(|keys| {
            keys.iter()
                .find(|k| k.get("kid").and_then(Value::as_str) == Some(kid))
        })
        .ok_or(OidcVerifyError::UnknownKid)?;

    let x_b64 = jwk
        .get("x")
        .and_then(Value::as_str)
        .ok_or(OidcVerifyError::BadKey)?;
    let y_b64 = jwk
        .get("y")
        .and_then(Value::as_str)
        .ok_or(OidcVerifyError::BadKey)?;
    let x = URL_SAFE_NO_PAD
        .decode(x_b64)
        .map_err(|_| OidcVerifyError::BadKey)?;
    let y = URL_SAFE_NO_PAD
        .decode(y_b64)
        .map_err(|_| OidcVerifyError::BadKey)?;
    if x.len() != 32 || y.len() != 32 {
        return Err(OidcVerifyError::BadKey);
    }
    let mut sec1 = Vec::with_capacity(65);
    sec1.push(0x04u8);
    sec1.extend_from_slice(&x);
    sec1.extend_from_slice(&y);
    let verifying_key =
        VerifyingKey::from_sec1_bytes(&sec1).map_err(|_| OidcVerifyError::BadKey)?;

    // Deliberate mapping: a corrupt or tampered signature segment
    // (including one that fails base64's trailing-bits check) counts as
    // an invalid signature, not a malformed token — header/payload were
    // already validated as well-formed JSON above, so `Malformed` is
    // reserved for structural/JSON problems and `InvalidSignature` for
    // anything wrong with the signature bytes themselves.
    let sig_bytes = URL_SAFE_NO_PAD
        .decode(sig_seg)
        .map_err(|_| OidcVerifyError::InvalidSignature)?;
    let signature =
        Signature::from_slice(&sig_bytes).map_err(|_| OidcVerifyError::InvalidSignature)?;
    let signing_input = format!("{header_seg}.{payload_seg}");
    verifying_key
        .verify(signing_input.as_bytes(), &signature)
        .map_err(|_| OidcVerifyError::InvalidSignature)?;

    if now_unix >= exp {
        return Err(OidcVerifyError::Expired);
    }
    if tenant_id != expected_tenant {
        return Err(OidcVerifyError::TenantMismatch);
    }

    Ok(VerifiedIdentity {
        sub,
        email,
        idp: iss,
        tenant_id,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use base64::engine::general_purpose::URL_SAFE_NO_PAD;
    use p256::ecdsa::{Signature, SigningKey, signature::Signer};

    // Fixed 32-byte scalar → deterministic test key (never use in prod).
    fn test_key() -> SigningKey {
        SigningKey::from_slice(&[
            0x11, 0x22, 0x33, 0x44, 0x55, 0x66, 0x77, 0x88, 0x11, 0x22, 0x33, 0x44, 0x55, 0x66,
            0x77, 0x88, 0x11, 0x22, 0x33, 0x44, 0x55, 0x66, 0x77, 0x88, 0x11, 0x22, 0x33, 0x44,
            0x55, 0x66, 0x77, 0x89,
        ])
        .expect("valid test scalar")
    }

    fn jwks_for(key: &SigningKey, kid: &str) -> String {
        let vk = key.verifying_key();
        let pt = vk.to_encoded_point(false);
        let x = URL_SAFE_NO_PAD.encode(pt.x().expect("x"));
        let y = URL_SAFE_NO_PAD.encode(pt.y().expect("y"));
        format!(
            r#"{{"keys":[{{"kty":"EC","crv":"P-256","alg":"ES256","kid":"{kid}","x":"{x}","y":"{y}"}}]}}"#
        )
    }

    fn make_jwt(key: &SigningKey, kid: &str, payload_json: &str) -> String {
        let header = format!(r#"{{"alg":"ES256","typ":"JWT","kid":"{kid}"}}"#);
        let h = URL_SAFE_NO_PAD.encode(header.as_bytes());
        let p = URL_SAFE_NO_PAD.encode(payload_json.as_bytes());
        let signing_input = format!("{h}.{p}");
        let sig: Signature = key.sign(signing_input.as_bytes());
        let s = URL_SAFE_NO_PAD.encode(sig.to_bytes());
        format!("{signing_input}.{s}")
    }

    fn payload(iss: &str, sub: &str, tenant: &str, exp: i64, email: Option<&str>) -> String {
        let email_field = email
            .map(|e| format!(r#","email":"{e}""#))
            .unwrap_or_default();
        format!(
            r#"{{"iss":"{iss}","sub":"{sub}","tenant_id":"{tenant}","exp":{exp}{email_field}}}"#
        )
    }

    #[test]
    fn verifies_valid_es256_token() {
        let key = test_key();
        let jwks = jwks_for(&key, "k1");
        let token = make_jwt(
            &key,
            "k1",
            &payload(
                "https://id.acme.example",
                "user-1",
                "acme",
                9999999999,
                Some("u@acme.example"),
            ),
        );
        let vi =
            verify_oidc_bearer(&token, "acme", 1000, |_url| Ok(jwks.clone())).expect("verify ok");
        assert_eq!(vi.sub, "user-1");
        assert_eq!(vi.tenant_id, "acme");
        assert_eq!(vi.email.as_deref(), Some("u@acme.example"));
        assert_eq!(vi.idp, "https://id.acme.example");
    }

    #[test]
    fn rejects_tampered_signature() {
        let key = test_key();
        let jwks = jwks_for(&key, "k1");
        let token = make_jwt(
            &key,
            "k1",
            &payload(
                "https://id.acme.example",
                "user-1",
                "acme",
                9999999999,
                None,
            ),
        );
        let mut bad = token.clone();
        bad.pop();
        bad.push(if token.ends_with('A') { 'B' } else { 'A' });
        let err = verify_oidc_bearer(&bad, "acme", 1000, |_u| Ok(jwks.clone())).unwrap_err();
        assert_eq!(err, OidcVerifyError::InvalidSignature);
    }

    #[test]
    fn rejects_valid_signature_reused_on_different_payload() {
        // Realistic attacker model: tamper the claims, reuse the original
        // signature. Both header and payload segments here decode fine as
        // base64 and parse fine as JSON (unlike `rejects_tampered_signature`,
        // which corrupts the base64 itself and never reaches `verify()`), so
        // this can only be caught by the ECDSA check actually running and
        // actually rejecting a mismatched signature.
        let key = test_key();
        let jwks = jwks_for(&key, "k1");
        let token = make_jwt(
            &key,
            "k1",
            &payload(
                "https://id.acme.example",
                "user-1",
                "acme",
                9999999999,
                None,
            ),
        );
        let mut segments = token.split('.');
        let header_seg = segments.next().expect("header segment");
        let _original_payload_seg = segments.next().expect("payload segment");
        let sig_seg = segments.next().expect("signature segment");

        // Different, still well-formed payload — signature no longer matches.
        let forged_payload = payload(
            "https://id.acme.example",
            "attacker",
            "acme",
            9999999999,
            None,
        );
        let forged_payload_seg = URL_SAFE_NO_PAD.encode(forged_payload.as_bytes());
        let forged_token = format!("{header_seg}.{forged_payload_seg}.{sig_seg}");

        let err =
            verify_oidc_bearer(&forged_token, "acme", 1000, |_u| Ok(jwks.clone())).unwrap_err();
        assert_eq!(err, OidcVerifyError::InvalidSignature);
    }

    #[test]
    fn rejects_expired() {
        let key = test_key();
        let jwks = jwks_for(&key, "k1");
        let token = make_jwt(
            &key,
            "k1",
            &payload("https://id.acme.example", "user-1", "acme", 500, None),
        );
        let err = verify_oidc_bearer(&token, "acme", 1000, |_u| Ok(jwks.clone())).unwrap_err();
        assert_eq!(err, OidcVerifyError::Expired);
    }

    #[test]
    fn rejects_wrong_tenant() {
        let key = test_key();
        let jwks = jwks_for(&key, "k1");
        let token = make_jwt(
            &key,
            "k1",
            &payload(
                "https://id.acme.example",
                "user-1",
                "acme",
                9999999999,
                None,
            ),
        );
        let err = verify_oidc_bearer(&token, "other", 1000, |_u| Ok(jwks.clone())).unwrap_err();
        assert_eq!(err, OidcVerifyError::TenantMismatch);
    }

    #[test]
    fn rejects_unknown_kid() {
        let key = test_key();
        let jwks = jwks_for(&key, "k1");
        let token = make_jwt(
            &key,
            "other-kid",
            &payload(
                "https://id.acme.example",
                "user-1",
                "acme",
                9999999999,
                None,
            ),
        );
        let err = verify_oidc_bearer(&token, "acme", 1000, |_u| Ok(jwks.clone())).unwrap_err();
        assert_eq!(err, OidcVerifyError::UnknownKid);
    }

    #[test]
    fn rejects_non_es256() {
        // HS256 header → UnsupportedAlg before any signature work.
        let h = URL_SAFE_NO_PAD.encode(br#"{"alg":"HS256","typ":"JWT","kid":"k1"}"#);
        let p = URL_SAFE_NO_PAD.encode(
            br#"{"iss":"https://id.acme.example","sub":"u","tenant_id":"acme","exp":9999999999}"#,
        );
        let token = format!("{h}.{p}.AAAA");
        let err = verify_oidc_bearer(&token, "acme", 1000, |_u| Ok("{\"keys\":[]}".to_string()))
            .unwrap_err();
        assert_eq!(err, OidcVerifyError::UnsupportedAlg);
    }

    #[test]
    fn jwks_url_uses_issuer_host_root() {
        assert_eq!(
            jwks_url_for_issuer("https://id.acme.example/foo/bar").unwrap(),
            "https://id.acme.example/jwks.json"
        );
        assert_eq!(
            jwks_url_for_issuer("https://acme.greentic-id.com").unwrap(),
            "https://acme.greentic-id.com/jwks.json"
        );
    }

    #[test]
    fn propagates_jwks_fetch_error() {
        let key = test_key();
        let token = make_jwt(
            &key,
            "k1",
            &payload("https://id.acme.example", "u", "acme", 9999999999, None),
        );
        let err =
            verify_oidc_bearer(&token, "acme", 1000, |_u| Err("boom".to_string())).unwrap_err();
        assert_eq!(err, OidcVerifyError::JwksFetch("boom".to_string()));
    }
}
