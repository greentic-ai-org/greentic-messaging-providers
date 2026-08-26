#![cfg(test)]

use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
use p256::ecdsa::{Signature, SigningKey, signature::Signer};

/// Fresh P-256 keypair per call; each call mints a distinct `kid`.
pub fn signed_fixture(iss: &str, aud: &str, sub: &str, scope: &str, exp: i64) -> (String, String) {
    let signing_key = SigningKey::random(&mut rand_core::OsRng);
    let point = signing_key.verifying_key().to_encoded_point(false);
    let kid = URL_SAFE_NO_PAD.encode(&point.as_bytes()[1..9]);

    let header = serde_json::json!({"alg": "ES256", "typ": "JWT", "kid": kid});
    let claims = serde_json::json!({
        "iss": iss, "aud": aud, "sub": sub, "scope": scope,
        "exp": exp, "iat": exp - 3600,
    });
    let header_enc = URL_SAFE_NO_PAD.encode(serde_json::to_vec(&header).expect("header json"));
    let claims_enc = URL_SAFE_NO_PAD.encode(serde_json::to_vec(&claims).expect("claims json"));
    let signing_input = format!("{header_enc}.{claims_enc}");
    let signature: Signature = signing_key.sign(signing_input.as_bytes());
    let token = format!(
        "{signing_input}.{}",
        URL_SAFE_NO_PAD.encode(signature.to_bytes())
    );

    let jwks = serde_json::json!({"keys": [{
        "kty": "EC", "crv": "P-256", "alg": "ES256", "kid": kid,
        "x": URL_SAFE_NO_PAD.encode(&point.as_bytes()[1..33]),
        "y": URL_SAFE_NO_PAD.encode(&point.as_bytes()[33..65]),
    }]});

    (token, jwks.to_string())
}
