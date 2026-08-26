use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
use p256::EncodedPoint;
use p256::ecdsa::{Signature, VerifyingKey, signature::Verifier};
use serde::Deserialize;

#[derive(Debug)]
pub struct VerifiedIdentity {
    pub sub: String,
}

#[derive(Debug)]
pub enum OidcError {
    InvalidFormat,
    UnsupportedAlg,
    UnknownKey,
    InvalidSignature,
    Expired,
    NotYetValid,
    IssuerMismatch,
    AudienceMismatch,
    MissingScope,
}

#[derive(Deserialize)]
struct JwtHeader {
    alg: String,
    #[serde(default)]
    kid: Option<String>,
}

#[derive(Deserialize)]
struct AccessClaims {
    iss: String,
    sub: String,
    aud: String,
    #[serde(default)]
    scope: String,
    exp: i64,
    #[serde(default)]
    nbf: Option<i64>,
}

#[derive(Deserialize)]
struct Jwk {
    kty: String,
    #[serde(default)]
    crv: Option<String>,
    #[serde(default)]
    kid: Option<String>,
    #[serde(default)]
    x: Option<String>,
    #[serde(default)]
    y: Option<String>,
}

#[derive(Deserialize)]
struct Jwks {
    keys: Vec<Jwk>,
}

fn decode_part<T: for<'de> Deserialize<'de>>(part: &str) -> Result<T, OidcError> {
    let bytes = URL_SAFE_NO_PAD
        .decode(part)
        .map_err(|_| OidcError::InvalidFormat)?;
    serde_json::from_slice(&bytes).map_err(|_| OidcError::InvalidFormat)
}

fn verifying_key(jwk: &Jwk) -> Result<VerifyingKey, OidcError> {
    if jwk.kty != "EC" || jwk.crv.as_deref() != Some("P-256") {
        return Err(OidcError::UnsupportedAlg);
    }
    let x = jwk.x.as_deref().ok_or(OidcError::UnknownKey)?;
    let y = jwk.y.as_deref().ok_or(OidcError::UnknownKey)?;
    let x_bytes = URL_SAFE_NO_PAD
        .decode(x)
        .map_err(|_| OidcError::UnknownKey)?;
    let y_bytes = URL_SAFE_NO_PAD
        .decode(y)
        .map_err(|_| OidcError::UnknownKey)?;
    if x_bytes.len() != 32 || y_bytes.len() != 32 {
        return Err(OidcError::UnknownKey);
    }
    let point = EncodedPoint::from_affine_coordinates(
        x_bytes.as_slice().into(),
        y_bytes.as_slice().into(),
        false,
    );
    VerifyingKey::from_encoded_point(&point).map_err(|_| OidcError::UnknownKey)
}

pub fn verify_access_token(
    token: &str,
    jwks_json: &str,
    expected_iss: &str,
    expected_aud: &str,
    required_scope: &str,
    now: i64,
) -> Result<VerifiedIdentity, OidcError> {
    let mut parts = token.split('.');
    let header_enc = parts.next().ok_or(OidcError::InvalidFormat)?;
    let claims_enc = parts.next().ok_or(OidcError::InvalidFormat)?;
    let sig_enc = parts.next().ok_or(OidcError::InvalidFormat)?;
    if parts.next().is_some() {
        return Err(OidcError::InvalidFormat);
    }

    let header: JwtHeader = decode_part(header_enc)?;
    if header.alg != "ES256" {
        return Err(OidcError::UnsupportedAlg);
    }

    let jwks: Jwks = serde_json::from_str(jwks_json).map_err(|_| OidcError::UnknownKey)?;
    let jwk = jwks
        .keys
        .iter()
        .find(|k| match (&header.kid, &k.kid) {
            (Some(want), Some(have)) => want == have,
            (None, _) => true,
            _ => false,
        })
        .ok_or(OidcError::UnknownKey)?;
    let key = verifying_key(jwk)?;

    let sig_bytes = URL_SAFE_NO_PAD
        .decode(sig_enc)
        .map_err(|_| OidcError::InvalidFormat)?;
    let signature = Signature::from_slice(&sig_bytes).map_err(|_| OidcError::InvalidSignature)?;
    let signing_input = format!("{header_enc}.{claims_enc}");
    key.verify(signing_input.as_bytes(), &signature)
        .map_err(|_| OidcError::InvalidSignature)?;

    let claims: AccessClaims = decode_part(claims_enc)?;
    if claims.iss != expected_iss {
        return Err(OidcError::IssuerMismatch);
    }
    if claims.aud != expected_aud {
        return Err(OidcError::AudienceMismatch);
    }
    if now >= claims.exp {
        return Err(OidcError::Expired);
    }
    if let Some(nbf) = claims.nbf
        && now < nbf
    {
        return Err(OidcError::NotYetValid);
    }
    if !required_scope.is_empty() && !claims.scope.split_whitespace().any(|s| s == required_scope) {
        return Err(OidcError::MissingScope);
    }

    Ok(VerifiedIdentity { sub: claims.sub })
}

#[cfg(test)]
mod tests {
    use super::*;

    const ISS: &str = "https://acme.greentic-id.com";
    const AUD: &str = "webchat-gui";
    const SCOPE: &str = "greentic.webchat";

    fn fixture() -> (String, String) {
        crate::directline::oidc_test_support::signed_fixture(
            ISS,
            AUD,
            "acme:users:1",
            "openid profile email greentic.webchat",
            2_000_000_000,
        )
    }

    #[test]
    fn accepts_a_valid_token() {
        let (token, jwks) = fixture();
        let id = verify_access_token(&token, &jwks, ISS, AUD, SCOPE, 1_900_000_000)
            .expect("token accepted");
        assert_eq!(id.sub, "acme:users:1");
    }

    #[test]
    fn rejects_an_expired_token() {
        let (token, jwks) = fixture();
        let err = verify_access_token(&token, &jwks, ISS, AUD, SCOPE, 2_000_000_001).unwrap_err();
        assert!(matches!(err, OidcError::Expired));
    }

    #[test]
    fn rejects_a_foreign_issuer() {
        let (token, jwks) = fixture();
        let err = verify_access_token(
            &token,
            &jwks,
            "https://evil.example",
            AUD,
            SCOPE,
            1_900_000_000,
        )
        .unwrap_err();
        assert!(matches!(err, OidcError::IssuerMismatch));
    }

    #[test]
    fn rejects_a_foreign_audience() {
        let (token, jwks) = fixture();
        let err = verify_access_token(&token, &jwks, ISS, "someone-else", SCOPE, 1_900_000_000)
            .unwrap_err();
        assert!(matches!(err, OidcError::AudienceMismatch));
    }

    #[test]
    fn rejects_a_token_without_the_required_scope() {
        let (token, jwks) = crate::directline::oidc_test_support::signed_fixture(
            ISS,
            AUD,
            "acme:users:1",
            "openid profile email",
            2_000_000_000,
        );
        let err = verify_access_token(&token, &jwks, ISS, AUD, SCOPE, 1_900_000_000).unwrap_err();
        assert!(matches!(err, OidcError::MissingScope));
    }

    #[test]
    fn rejects_a_tampered_signature() {
        let (token, jwks) = fixture();
        let mut parts: Vec<&str> = token.split('.').collect();
        let tampered_sig = "AAAA".to_string() + &parts[2][4..];
        parts[2] = &tampered_sig;
        let bad = parts.join(".");
        let err = verify_access_token(&bad, &jwks, ISS, AUD, SCOPE, 1_900_000_000).unwrap_err();
        assert!(matches!(err, OidcError::InvalidSignature));
    }

    #[test]
    fn rejects_a_key_the_jwks_does_not_carry() {
        let (token, _) = fixture();
        let (_, other_jwks) = crate::directline::oidc_test_support::signed_fixture(
            ISS,
            AUD,
            "acme:users:2",
            "greentic.webchat",
            2_000_000_000,
        );
        let err =
            verify_access_token(&token, &other_jwks, ISS, AUD, SCOPE, 1_900_000_000).unwrap_err();
        assert!(matches!(err, OidcError::UnknownKey));
    }
}
