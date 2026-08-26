use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
use chrono::{Duration, Utc};
use hmac::{Hmac, KeyInit, Mac};
use serde::{Deserialize, Serialize};
use sha2::Sha256;

/// Direct Line token lifetime. Matches the Microsoft Direct Line 3.0 reference
/// value of 24 hours — long enough that a chat session outlives a single token,
/// with `/v3/directline/tokens/refresh` available to extend live conversations.
pub const TTL_SECONDS: i64 = 24 * 60 * 60;
const ISS: &str = "greentic.webchat";
const AUD: &str = "directline";

type HmacSha256 = Hmac<Sha256>;

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct DirectLineContext {
    pub env: String,
    pub tenant: String,
    pub team: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct TokenClaims {
    pub iss: String,
    pub aud: String,
    pub sub: String,
    pub iat: i64,
    pub nbf: i64,
    pub exp: i64,
    pub ctx: DirectLineContext,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub conv: Option<String>,
    /// True when `sub` was bound from a verified OIDC bearer, not an
    /// anonymous/self-declared identity.
    #[serde(default)]
    pub verified: bool,
}

#[allow(dead_code)]
#[derive(Debug)]
pub enum JwtError {
    InvalidFormat,
    InvalidSignature,
    Expired,
    NotYetValid,
    InvalidKey,
    Json(serde_json::Error),
    Base64(base64::DecodeError),
}

impl From<serde_json::Error> for JwtError {
    fn from(err: serde_json::Error) -> Self {
        JwtError::Json(err)
    }
}

impl From<base64::DecodeError> for JwtError {
    fn from(err: base64::DecodeError) -> Self {
        JwtError::Base64(err)
    }
}

fn encode_segment<T: Serialize>(value: &T) -> Result<String, JwtError> {
    let json = serde_json::to_string(value)?;
    Ok(URL_SAFE_NO_PAD.encode(json.as_bytes()))
}

fn decode_segment<T: for<'de> Deserialize<'de>>(value: &str) -> Result<T, JwtError> {
    let bytes = URL_SAFE_NO_PAD.decode(value)?;
    let decoded = serde_json::from_slice(&bytes)?;
    Ok(decoded)
}

pub fn issue_token(
    secret: &[u8],
    ctx: DirectLineContext,
    sub: &str,
    conv: Option<String>,
    verified: bool,
) -> Result<(String, i64), JwtError> {
    let now = Utc::now();
    let iat = now.timestamp();
    let exp = (now + Duration::seconds(TTL_SECONDS)).timestamp();
    let claims = TokenClaims {
        iss: ISS.to_string(),
        aud: AUD.to_string(),
        sub: sub.to_string(),
        iat,
        nbf: iat,
        exp,
        ctx,
        conv,
        verified,
    };
    let header = serde_json::json!({"alg":"HS256","typ":"JWT"});
    let header_enc = encode_segment(&header)?;
    let payload_enc = encode_segment(&claims)?;
    let mut mac = HmacSha256::new_from_slice(secret).map_err(|_| JwtError::InvalidKey)?;
    mac.update(header_enc.as_bytes());
    mac.update(b".");
    mac.update(payload_enc.as_bytes());
    let signature = mac.finalize().into_bytes();
    let signature_enc = URL_SAFE_NO_PAD.encode(signature);
    let token = format!("{header_enc}.{payload_enc}.{signature_enc}");
    Ok((token, exp))
}

pub fn verify_token(secret: &[u8], token: &str) -> Result<TokenClaims, JwtError> {
    let mut parts = token.split('.');
    let header = parts.next().ok_or(JwtError::InvalidFormat)?;
    let payload = parts.next().ok_or(JwtError::InvalidFormat)?;
    let signature = parts.next().ok_or(JwtError::InvalidFormat)?;
    if parts.next().is_some() {
        return Err(JwtError::InvalidFormat);
    }
    let mut mac = HmacSha256::new_from_slice(secret).map_err(|_| JwtError::InvalidKey)?;
    mac.update(header.as_bytes());
    mac.update(b".");
    mac.update(payload.as_bytes());
    let expected = mac.finalize().into_bytes();
    let decoded_sig = URL_SAFE_NO_PAD.decode(signature)?;
    if expected.as_slice() != decoded_sig {
        return Err(JwtError::InvalidSignature);
    }
    let claims: TokenClaims = decode_segment(payload)?;
    let now = Utc::now().timestamp();
    // Allow a small clock-skew leeway (30 seconds) to avoid NotYetValid
    // errors when the token is created and validated on systems with
    // slightly different clocks (e.g. WSL2 + external device via tunnel).
    let leeway = 30;
    if now + leeway < claims.nbf {
        return Err(JwtError::NotYetValid);
    }
    if now >= claims.exp {
        return Err(JwtError::Expired);
    }
    Ok(claims)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_ctx() -> DirectLineContext {
        DirectLineContext {
            env: "default".into(),
            tenant: "default".into(),
            team: Some("team-a".into()),
        }
    }

    fn signed_token(signing_key: &[u8], claims: TokenClaims) -> Result<String, JwtError> {
        let header = serde_json::json!({"alg":"HS256","typ":"JWT"});
        let header_enc = encode_segment(&header)?;
        let payload_enc = encode_segment(&claims)?;
        let mut mac = HmacSha256::new_from_slice(signing_key).map_err(|_| JwtError::InvalidKey)?;
        mac.update(header_enc.as_bytes());
        mac.update(b".");
        mac.update(payload_enc.as_bytes());
        let signature_enc = URL_SAFE_NO_PAD.encode(mac.finalize().into_bytes());
        Ok(format!("{header_enc}.{payload_enc}.{signature_enc}"))
    }

    #[test]
    fn token_round_trip() -> Result<(), JwtError> {
        let signing_key = b"test-hmac-key";
        let ctx = sample_ctx();
        let (token, exp) = issue_token(signing_key, ctx.clone(), "user-123", None, false)?;
        assert!(token.split('.').count() == 3);
        assert!(exp > Utc::now().timestamp());
        let claims = verify_token(signing_key, &token)?;
        assert_eq!(claims.sub, "user-123");
        assert_eq!(claims.ctx, ctx);
        assert!(claims.conv.is_none());
        Ok(())
    }

    #[test]
    fn verify_rejects_malformed_tokens() {
        let signing_key = b"test-hmac-key";

        assert!(matches!(
            verify_token(signing_key, "not-enough.parts"),
            Err(JwtError::InvalidFormat)
        ));
        assert!(matches!(
            verify_token(signing_key, "too.many.parts.here"),
            Err(JwtError::InvalidFormat)
        ));
        assert!(matches!(
            verify_token(signing_key, "header.payload.not-base64!"),
            Err(JwtError::Base64(_))
        ));
    }

    #[test]
    fn verify_rejects_wrong_signature() -> Result<(), JwtError> {
        let signing_key = b"test-hmac-key";
        let (token, _) = issue_token(signing_key, sample_ctx(), "user-123", None, false)?;

        assert!(matches!(
            verify_token(b"wrong-hmac-key", &token),
            Err(JwtError::InvalidSignature)
        ));
        Ok(())
    }

    #[test]
    fn verify_rejects_expired_and_not_yet_valid_claims() -> Result<(), JwtError> {
        let signing_key = b"test-hmac-key";
        let now = Utc::now().timestamp();
        let base = TokenClaims {
            iss: ISS.to_string(),
            aud: AUD.to_string(),
            sub: "user-123".to_string(),
            iat: now,
            nbf: now,
            exp: now + TTL_SECONDS,
            ctx: sample_ctx(),
            conv: None,
            verified: false,
        };

        let expired = TokenClaims {
            exp: now - 1,
            ..base
        };
        let expired_token = signed_token(signing_key, expired)?;
        assert!(matches!(
            verify_token(signing_key, &expired_token),
            Err(JwtError::Expired)
        ));

        let future = TokenClaims {
            iat: now + 120,
            nbf: now + 120,
            exp: now + TTL_SECONDS,
            ctx: sample_ctx(),
            conv: None,
            iss: ISS.to_string(),
            aud: AUD.to_string(),
            sub: "user-123".to_string(),
            verified: false,
        };
        let future_token = signed_token(signing_key, future)?;
        assert!(matches!(
            verify_token(signing_key, &future_token),
            Err(JwtError::NotYetValid)
        ));
        Ok(())
    }

    #[test]
    fn token_with_conv_claim() -> Result<(), JwtError> {
        let signing_key = b"another-test-hmac-key";
        let ctx = DirectLineContext {
            env: "prod".into(),
            tenant: "tenant-a".into(),
            team: None,
        };
        let (token, _) = issue_token(
            signing_key,
            ctx.clone(),
            "user-x",
            Some("conv-99".into()),
            true,
        )?;
        let claims = verify_token(signing_key, &token)?;
        assert_eq!(claims.conv.as_deref(), Some("conv-99"));
        assert_eq!(claims.ctx, ctx);
        Ok(())
    }
}
