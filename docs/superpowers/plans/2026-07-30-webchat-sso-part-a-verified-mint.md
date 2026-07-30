# Webchat SSO Part A — Verified-Identity DirectLine Mint — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Make the DirectLine token mint optionally accept an OIDC `Authorization: Bearer`, verify it (ES256) against the tenant's Tenant-Manager issuer, and stamp *server-verified* identity into the DirectLine token, its renewal path, and the inbound message envelope — so a flow/agent can trust who the chatting user is.

**Architecture:** A new pure `oidc_verify` module (ES256 + JWKS, no HTTP of its own — the JWKS fetch is injected as a closure) does the verification. `handle_tokens` calls it when a bearer is present, minting a DirectLine JWT whose `sub`/`email`/`idp`/`verified` claims come from the verified OIDC token; with no bearer the current anonymous behavior is byte-for-byte preserved. The new claims flow through `greentic-start`'s sliding-window renewal (preserve-only — renewal can never upgrade `verified`) and are surfaced to the flow via response headers → envelope metadata. This is Part A of a two-part epic; the SDK (Part B) consumes this contract and is planned separately after this lands.

**Tech Stack:** Rust 2024, `wasm32-wasip2` component (`cargo component`), `p256` (RustCrypto, ES256 verify), existing `hmac`/`sha2`/`base64`/`serde_json`, the `greentic:http` `http-client` WIT import for the JWKS GET.

**Spec:** `greentic-messaging-providers/docs/superpowers/specs/2026-07-30-webchat-sso-verified-identity-design.md`

## Global Constraints

- Rust toolchain **1.95.0**, edition **2024** (both repos pin via `rust-toolchain.toml` — do not edit).
- `greentic-messaging-providers` component target: **`wasm32-wasip2`** via `cargo component`. Native crates build for host.
- **No `unwrap()` / `panic!()` in production paths** — use `Result`/`?`. (Test code may `unwrap`.)
- **English only** in source, tests, comments, commit messages.
- **Comments minimal** (repo rule): default none; one short line only when the *why* is non-obvious. Never describe *what* the code does.
- **Conventional Commits** (`feat:`, `fix:`, `refactor:`, `docs:`). **Do NOT add Claude co-author attribution** to commits (repo rule for `greentic-messaging-providers`; apply the same to `greentic-start` commits here).
- Pre-commit hook (`greentic-messaging-providers/.githooks/pre-commit`) runs `rustfmt --check` on staged `.rs` + `cargo clippy --workspace -- -D warnings`. Do **not** bypass with `--no-verify`. Keep code fmt+clippy clean before each commit.
- `Cargo.lock` is committed; adding `p256` will update it — stage it.
- The two repos are **separate git repos**. `greentic-messaging-providers` is on branch `research`; `greentic-start` — check its current branch and stay on it. Cross-repo tasks (Task 8–9) commit in `greentic-start`.
- **Backward compatibility is a hard requirement:** a request with no `Authorization` header must produce exactly today's response. Existing tests must stay green.

## Repos & branches

- **R1 = `/home/bima-pangestu/projects/Works/greentic/greentic-messaging-providers`** (branch `research`). Tasks 1–7.
- **R2 = `/home/bima-pangestu/projects/Works/greentic/greentic-start`**. Tasks 8–9. Confirm branch with `git -C <R2> branch --show-current` before starting; commit on whatever branch is checked out unless told otherwise.

## File Structure

**R1 — `components/messaging-provider-webchat/`:**
- **Create** `src/directline/oidc_verify.rs` — ES256/JWKS verifier (pure; JWKS fetch injected). Owns `VerifiedIdentity`, `OidcVerifyError`, `jwks_url_for_issuer`, `verify_oidc_bearer`.
- **Modify** `src/directline/mod.rs` — add `pub mod oidc_verify;` (or `mod oidc_verify;` + re-export as siblings do).
- **Modify** `src/directline/jwt.rs` — add `TokenIdentity` struct; extend `TokenClaims` with `email`/`idp`/`verified`; change `issue_token` to take `&TokenIdentity`.
- **Modify** `src/directline/http.rs` — extract a testable `mint_token_response(...)`; wire bearer verification into `handle_tokens`; emit identity headers in `handle_conversations`.
- **Modify** `src/ops/ingest.rs` — read identity headers, stamp `envelope.metadata`.
- **Modify** `components/messaging-provider-webchat/Cargo.toml` + workspace `Cargo.toml` — add `p256`.

**R2 — `greentic-start/`:**
- **Modify** `src/directline_token.rs` — extend `DirectLineTokenClaims` (verify-side, partial) with `email`/`idp`/`verified` (`#[serde(default)]`).
- **Modify** `src/directline_session.rs` — extend `DlClaims`; `mint_token` copies the new fields verbatim (preserve-only).

**Docs:**
- **Modify** `greentic-tenant-manager/docs/WEBCHAT_OIDC_INTEGRATION.md` — document the verified-mint contract + new claims. (Task 10.)

---

## Task 1: ES256/JWKS verifier module (`oidc_verify.rs`)

The load-bearing primitive. Pure function: given a compact JWT, the expected tenant, the current time, and a JWKS-fetch closure, it returns a `VerifiedIdentity` or a typed error. No HTTP here — the closure is injected so this is fully unit-testable and reusable.

**Files:**
- Create: `components/messaging-provider-webchat/src/directline/oidc_verify.rs`
- Modify: `components/messaging-provider-webchat/src/directline/mod.rs`
- Modify: `components/messaging-provider-webchat/Cargo.toml`, root `Cargo.toml`
- Test: inline `#[cfg(test)] mod tests` in `oidc_verify.rs`

**Interfaces:**
- Produces:
  - `pub struct VerifiedIdentity { pub sub: String, pub email: Option<String>, pub idp: String, pub tenant_id: String }`
  - `pub enum OidcVerifyError { Malformed, UnsupportedAlg, NoKid, UnknownKid, BadKey, InvalidSignature, Expired, MissingClaim(&'static str), TenantMismatch, JwksFetch(String), JwksParse }`
  - `pub fn jwks_url_for_issuer(iss: &str) -> Result<String, OidcVerifyError>`
  - `pub fn verify_oidc_bearer<F>(token: &str, expected_tenant: &str, now_unix: i64, fetch_jwks: F) -> Result<VerifiedIdentity, OidcVerifyError> where F: FnOnce(&str) -> Result<String, String>`

- [ ] **Step 1: Add the `p256` dependency**

In root `Cargo.toml` `[workspace.dependencies]` add:
```toml
p256 = { version = "0.13", default-features = false, features = ["ecdsa"] }
```
In `components/messaging-provider-webchat/Cargo.toml` `[dependencies]` add:
```toml
p256 = { workspace = true }
```
Rationale: matches Tenant-Manager's `p256 = "0.13"`; `default-features = false` keeps `getrandom`/`std` out of the wasip2 build (verify needs neither). The `ecdsa` feature provides `VerifyingKey`, `Signature`, and — for the tests below — `SigningKey` (deterministic RFC6979 signing, no RNG).

- [ ] **Step 2: Verify the dep compiles for wasm before writing logic**

Run: `cd <R1> && cargo component build -p messaging-provider-webchat`
Expected: builds clean (p256 is pure Rust). If it fails on a missing item like `to_encoded_point`/`from_sec1_bytes`, add `"arithmetic"` to the p256 features and rebuild. Do not proceed until this is green.

- [ ] **Step 3: Register the module**

In `src/directline/mod.rs`, add alongside the existing submodule declarations:
```rust
pub mod oidc_verify;
```

- [ ] **Step 4: Write the failing tests**

Create `src/directline/oidc_verify.rs` with only the test module first (types/functions referenced won't exist yet → compile-fail, which counts as the failing test). Paste:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
    use p256::ecdsa::{SigningKey, signature::Signer, Signature};

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
        format!(r#"{{"iss":"{iss}","sub":"{sub}","tenant_id":"{tenant}","exp":{exp}{email_field}}}"#)
    }

    #[test]
    fn verifies_valid_es256_token() {
        let key = test_key();
        let jwks = jwks_for(&key, "k1");
        let token = make_jwt(
            &key,
            "k1",
            &payload("https://id.acme.example", "user-1", "acme", 9999999999, Some("u@acme.example")),
        );
        let vi = verify_oidc_bearer(&token, "acme", 1000, |_url| Ok(jwks.clone())).expect("verify ok");
        assert_eq!(vi.sub, "user-1");
        assert_eq!(vi.tenant_id, "acme");
        assert_eq!(vi.email.as_deref(), Some("u@acme.example"));
        assert_eq!(vi.idp, "https://id.acme.example");
    }

    #[test]
    fn rejects_tampered_signature() {
        let key = test_key();
        let jwks = jwks_for(&key, "k1");
        let token = make_jwt(&key, "k1", &payload("https://id.acme.example", "user-1", "acme", 9999999999, None));
        let mut bad = token.clone();
        bad.pop();
        bad.push(if token.ends_with('A') { 'B' } else { 'A' });
        let err = verify_oidc_bearer(&bad, "acme", 1000, |_u| Ok(jwks.clone())).unwrap_err();
        assert_eq!(err, OidcVerifyError::InvalidSignature);
    }

    #[test]
    fn rejects_expired() {
        let key = test_key();
        let jwks = jwks_for(&key, "k1");
        let token = make_jwt(&key, "k1", &payload("https://id.acme.example", "user-1", "acme", 500, None));
        let err = verify_oidc_bearer(&token, "acme", 1000, |_u| Ok(jwks.clone())).unwrap_err();
        assert_eq!(err, OidcVerifyError::Expired);
    }

    #[test]
    fn rejects_wrong_tenant() {
        let key = test_key();
        let jwks = jwks_for(&key, "k1");
        let token = make_jwt(&key, "k1", &payload("https://id.acme.example", "user-1", "acme", 9999999999, None));
        let err = verify_oidc_bearer(&token, "other", 1000, |_u| Ok(jwks.clone())).unwrap_err();
        assert_eq!(err, OidcVerifyError::TenantMismatch);
    }

    #[test]
    fn rejects_unknown_kid() {
        let key = test_key();
        let jwks = jwks_for(&key, "k1");
        let token = make_jwt(&key, "other-kid", &payload("https://id.acme.example", "user-1", "acme", 9999999999, None));
        let err = verify_oidc_bearer(&token, "acme", 1000, |_u| Ok(jwks.clone())).unwrap_err();
        assert_eq!(err, OidcVerifyError::UnknownKid);
    }

    #[test]
    fn rejects_non_es256() {
        // HS256 header → UnsupportedAlg before any signature work.
        let h = URL_SAFE_NO_PAD.encode(br#"{"alg":"HS256","typ":"JWT","kid":"k1"}"#);
        let p = URL_SAFE_NO_PAD.encode(br#"{"iss":"https://id.acme.example","sub":"u","tenant_id":"acme","exp":9999999999}"#);
        let token = format!("{h}.{p}.AAAA");
        let err = verify_oidc_bearer(&token, "acme", 1000, |_u| Ok("{\"keys\":[]}".to_string())).unwrap_err();
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
        let token = make_jwt(&key, "k1", &payload("https://id.acme.example", "u", "acme", 9999999999, None));
        let err = verify_oidc_bearer(&token, "acme", 1000, |_u| Err("boom".to_string())).unwrap_err();
        assert_eq!(err, OidcVerifyError::JwksFetch("boom".to_string()));
    }
}
```

- [ ] **Step 5: Run tests to verify they fail**

Run: `cd <R1> && cargo test -p messaging-provider-webchat --lib oidc_verify`
Expected: FAIL to compile (`verify_oidc_bearer`, `VerifiedIdentity`, `OidcVerifyError`, `jwks_url_for_issuer` not found).

- [ ] **Step 6: Write the implementation**

Prepend to `src/directline/oidc_verify.rs` (above the test module):

```rust
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
    let rest = iss.strip_prefix("https://").ok_or(OidcVerifyError::Malformed)?;
    let authority = rest.split('/').next().unwrap_or("");
    if authority.is_empty() {
        return Err(OidcVerifyError::Malformed);
    }
    Ok(format!("https://{authority}/jwks.json"))
}

fn decode_segment(seg: &str) -> Result<Value, OidcVerifyError> {
    let bytes = URL_SAFE_NO_PAD.decode(seg).map_err(|_| OidcVerifyError::Malformed)?;
    serde_json::from_slice(&bytes).map_err(|_| OidcVerifyError::Malformed)
}

fn claim_str<'a>(claims: &'a Value, key: &'static str) -> Result<&'a str, OidcVerifyError> {
    claims
        .get(key)
        .and_then(Value::as_str)
        .ok_or(OidcVerifyError::MissingClaim(key))
}

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
    let email = claims.get("email").and_then(Value::as_str).map(str::to_string);

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

    let x_b64 = jwk.get("x").and_then(Value::as_str).ok_or(OidcVerifyError::BadKey)?;
    let y_b64 = jwk.get("y").and_then(Value::as_str).ok_or(OidcVerifyError::BadKey)?;
    let x = URL_SAFE_NO_PAD.decode(x_b64).map_err(|_| OidcVerifyError::BadKey)?;
    let y = URL_SAFE_NO_PAD.decode(y_b64).map_err(|_| OidcVerifyError::BadKey)?;
    if x.len() != 32 || y.len() != 32 {
        return Err(OidcVerifyError::BadKey);
    }
    let mut sec1 = Vec::with_capacity(65);
    sec1.push(0x04u8);
    sec1.extend_from_slice(&x);
    sec1.extend_from_slice(&y);
    let verifying_key = VerifyingKey::from_sec1_bytes(&sec1).map_err(|_| OidcVerifyError::BadKey)?;

    let sig_bytes = URL_SAFE_NO_PAD.decode(sig_seg).map_err(|_| OidcVerifyError::Malformed)?;
    let signature = Signature::from_slice(&sig_bytes).map_err(|_| OidcVerifyError::InvalidSignature)?;
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

    Ok(VerifiedIdentity { sub, email, idp: iss, tenant_id })
}
```

Note: signature verification runs before the `exp`/tenant checks so a forged token can never reach claim logic. `VerifyingKey::verify` hashes the input with SHA-256 internally (ES256), so no explicit `sha2` call is needed.

- [ ] **Step 7: Run tests to verify they pass**

Run: `cd <R1> && cargo test -p messaging-provider-webchat --lib oidc_verify`
Expected: PASS (8 tests). If `to_encoded_point`/`x()`/`y()` don't resolve in the test helper, add `"arithmetic"` to the p256 features (Step 1) and re-run.

- [ ] **Step 8: fmt + clippy**

Run: `cd <R1> && cargo fmt -p messaging-provider-webchat && cargo clippy -p messaging-provider-webchat --all-targets -- -D warnings`
Expected: clean.

- [ ] **Step 9: Commit**

```bash
cd <R1>
git add components/messaging-provider-webchat/src/directline/oidc_verify.rs \
        components/messaging-provider-webchat/src/directline/mod.rs \
        components/messaging-provider-webchat/Cargo.toml Cargo.toml Cargo.lock
git commit -m "feat(webchat): add ES256/JWKS OIDC bearer verifier"
```

---

## Task 2: Extend DirectLine `TokenClaims` with verified identity

Add `email`/`idp`/`verified` to the minted DirectLine JWT, threaded through `issue_token` via a small `TokenIdentity` struct. New fields are optional/defaulted so existing tokens still deserialize (`verified` absent → `false`).

**Files:**
- Modify: `components/messaging-provider-webchat/src/directline/jwt.rs`
- Test: inline `#[cfg(test)] mod tests` in `jwt.rs`

**Interfaces:**
- Consumes: nothing new.
- Produces:
  - `pub struct TokenIdentity { pub sub: String, pub email: Option<String>, pub idp: Option<String>, pub verified: bool }`
  - `pub fn issue_token(secret: &[u8], ctx: DirectLineContext, identity: &TokenIdentity, conv: Option<String>) -> Result<(String, i64), JwtError>` (signature change: `sub: &str` → `identity: &TokenIdentity`)
  - `TokenClaims` gains: `pub email: Option<String>`, `pub idp: Option<String>`, `pub verified: bool`.

- [ ] **Step 1: Write the failing tests**

Add to the existing `#[cfg(test)] mod tests` in `jwt.rs`:

```rust
#[test]
fn issue_token_carries_verified_identity() -> Result<(), JwtError> {
    let key = b"test-hmac-key";
    let id = TokenIdentity {
        sub: "user-9".into(),
        email: Some("u@acme.example".into()),
        idp: Some("https://id.acme.example".into()),
        verified: true,
    };
    let (token, _exp) = issue_token(key, sample_ctx(), &id, None)?;
    let claims = verify_token(key, &token)?;
    assert_eq!(claims.sub, "user-9");
    assert_eq!(claims.email.as_deref(), Some("u@acme.example"));
    assert_eq!(claims.idp.as_deref(), Some("https://id.acme.example"));
    assert!(claims.verified);
    Ok(())
}

#[test]
fn legacy_token_without_new_fields_deserializes_unverified() -> Result<(), JwtError> {
    // A token minted before this change (no email/idp/verified in payload) must
    // still verify, with verified defaulting to false.
    let key = b"test-hmac-key";
    let legacy_claims = serde_json::json!({
        "iss": "greentic.webchat", "aud": "directline", "sub": "old",
        "iat": Utc::now().timestamp(), "nbf": Utc::now().timestamp(),
        "exp": Utc::now().timestamp() + 60,
        "ctx": {"env":"default","tenant":"default","team":null}
    });
    let header = serde_json::json!({"alg":"HS256","typ":"JWT"});
    let header_enc = encode_segment(&header)?;
    let payload_enc = encode_segment(&legacy_claims)?;
    let mut mac = HmacSha256::new_from_slice(key).map_err(|_| JwtError::InvalidKey)?;
    mac.update(header_enc.as_bytes());
    mac.update(b".");
    mac.update(payload_enc.as_bytes());
    let sig = URL_SAFE_NO_PAD.encode(mac.finalize().into_bytes());
    let token = format!("{header_enc}.{payload_enc}.{sig}");
    let claims = verify_token(key, &token)?;
    assert_eq!(claims.sub, "old");
    assert!(!claims.verified);
    assert!(claims.email.is_none());
    Ok(())
}
```

Also update the existing `signed_token` test helper and any current `issue_token(...)` call in the tests to the new signature, e.g. replace `issue_token(signing_key, ctx.clone(), "user-123", None)?` with:
```rust
issue_token(signing_key, ctx.clone(), &TokenIdentity { sub: "user-123".into(), email: None, idp: None, verified: false }, None)?
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cd <R1> && cargo test -p messaging-provider-webchat --lib jwt`
Expected: FAIL to compile (`TokenIdentity` missing; `issue_token` arity/signature mismatch; `TokenClaims` has no `verified`).

- [ ] **Step 3: Write the implementation**

In `jwt.rs`, extend `TokenClaims` (keep field order; add after `conv` or before — serde is name-based):
```rust
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
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub email: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub idp: Option<String>,
    #[serde(default, skip_serializing_if = "is_false")]
    pub verified: bool,
}

fn is_false(b: &bool) -> bool {
    !*b
}

#[derive(Debug, Clone)]
pub struct TokenIdentity {
    pub sub: String,
    pub email: Option<String>,
    pub idp: Option<String>,
    pub verified: bool,
}
```

Change `issue_token`:
```rust
pub fn issue_token(
    secret: &[u8],
    ctx: DirectLineContext,
    identity: &TokenIdentity,
    conv: Option<String>,
) -> Result<(String, i64), JwtError> {
    let now = Utc::now();
    let iat = now.timestamp();
    let exp = (now + Duration::seconds(TTL_SECONDS)).timestamp();
    let claims = TokenClaims {
        iss: ISS.to_string(),
        aud: AUD.to_string(),
        sub: identity.sub.clone(),
        iat,
        nbf: iat,
        exp,
        ctx,
        conv,
        email: identity.email.clone(),
        idp: identity.idp.clone(),
        verified: identity.verified,
    };
    // ... unchanged from here (header/payload encode, HMAC, format) ...
}
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cd <R1> && cargo test -p messaging-provider-webchat --lib jwt`
Expected: PASS (existing jwt tests + 2 new). This will still fail to compile the *crate* because `http.rs` calls the old `issue_token` — that is fixed in Task 3; running with `--lib jwt` compiles the whole lib, so expect a compile error at the `http.rs` call site. To keep this task self-contained, update that one call site now to the new signature (minimal shim; full wiring in Task 3):

In `http.rs` `handle_tokens`, replace:
```rust
match issue_token(&signing_key, ctx.clone(), subject.token_subject(), None) {
```
with:
```rust
let identity = TokenIdentity { sub: subject.token_subject().to_string(), email: None, idp: None, verified: false };
match issue_token(&signing_key, ctx.clone(), &identity, None) {
```
and add `use crate::directline::jwt::TokenIdentity;` (or reference via the existing jwt import path). Re-run: PASS.

- [ ] **Step 5: fmt + clippy + commit**

```bash
cd <R1>
cargo fmt -p messaging-provider-webchat
cargo clippy -p messaging-provider-webchat --all-targets -- -D warnings
git add components/messaging-provider-webchat/src/directline/jwt.rs components/messaging-provider-webchat/src/directline/http.rs
git commit -m "feat(webchat): thread verified identity through DirectLine token claims"
```

---

## Task 3: Wire bearer verification into the mint (`handle_tokens`) — with tenant-pinned issuer

Refactor the mint body into a testable inner function that takes an already-resolved identity, then have `handle_tokens` resolve identity from the bearer (verifying via Task 1) or fall back to anonymous. On any bearer-verification failure → `401`. On no bearer → today's behavior.

**SECURITY — tenant-pinned issuer (required, from the Task 1 review):** `verify_oidc_bearer` fetches JWKS from a URL derived from the token's OWN unverified `iss`. Without pinning, an attacker can host their own JWKS and sign `iss=attacker, tenant_id=<victim>` → forged identity. This task MUST pin the issuer: the mint loads the tenant's expected OIDC issuer from config/secrets (`oidc_issuer`, mirroring how `load_signing_key` reads `jwt_signing_key`), and rejects any token whose issuer-derived JWKS URL does not exactly match the expected issuer's JWKS URL — before any network fetch. If a bearer is presented but no `oidc_issuer` is configured, reject `401` (cannot establish trust). Anonymous (no bearer) is unaffected.

The expected issuer is per-tenant and dynamic (custom-domain tenants break any slug pattern), stored by Tenant Manager as the `oidc_issuer` value; the webchat provider config for a tenant carries it. This is a single-tenant-per-route deployment (one provider config per tenant), so the config's `oidc_issuer` is the trust anchor. (Pre-existing, out of scope: `ctx.tenant` comes from the client-supplied `?tenant=` query param and is not independently host-authenticated — documented in the ledger, not fixed here.)

**Follow-up (NOT this task):** formally declaring `oidc_issuer` as a validated provider config field (`config_schema()`, `I18N_KEYS`/`PAIRS`, setup question, schema-hash + fixture regen) is deferred. This task reads it via the config/secret lookup, which works without the schema plumbing.

**Files:**
- Modify: `components/messaging-provider-webchat/src/directline/http.rs`
- Modify: `components/messaging-provider-webchat/src/directline/oidc_verify.rs` (remove the module-level `#![allow(dead_code)]` — the verifier is now wired in; deferred minor #4 from Task 1)
- Test: inline tests in `http.rs`

**Interfaces:**
- Consumes: `oidc_verify::{verify_oidc_bearer, jwks_url_for_issuer, VerifiedIdentity, OidcVerifyError}`, `jwt::{issue_token, TokenIdentity}`, existing `extract_bearer`, `parse_context`, `load_signing_key`, `SecretStore`, `respond_json`, `respond_error`, `TTL_SECONDS`, `base64::engine::general_purpose::STANDARD` (already imported in `http.rs` for `load_signing_key`).
- Produces:
  - `const OIDC_ISSUER_KEY: &str = "oidc_issuer";`
  - `fn load_expected_issuer<SE: SecretStore>(request: &HttpInV1, secrets: &SE) -> Option<String>` — config `oidc_issuer_b64` (base64 STANDARD) first, then `secrets.get("oidc_issuer")`; trims; empty → `None`.
  - `fn verify_bearer_pinned<F>(token: &str, tenant: &str, now: i64, expected_issuer: &str, fetch: F) -> Result<VerifiedIdentity, OidcVerifyError> where F: FnOnce(&str) -> Result<String, String>` — rejects any JWKS URL not equal to `jwks_url_for_issuer(expected_issuer)` before calling `fetch`.
  - `fn resolve_identity<V>(bearer: Option<&str>, tenant: &str, now: i64, verify: V, anon_sub: &str) -> Result<TokenIdentity, HttpOutV1>` where `V: FnOnce(&str,&str,i64) -> Result<VerifiedIdentity, OidcVerifyError>`.
  - `fn mint_response(signing_key: &[u8], ctx: DirectLineContext, identity: &TokenIdentity) -> HttpOutV1`.
  - `fn fetch_jwks_http(url: &str) -> Result<String, String>`.

- [ ] **Step 1: Write the failing tests**

Add to `http.rs` tests. First a helper that builds an unsigned token with a chosen `iss` (the pin check runs before signature verification, so a dummy signature is fine for pin tests):

```rust
fn fake_bearer(iss: &str, tenant: &str) -> String {
    use base64::engine::general_purpose::URL_SAFE_NO_PAD;
    let h = URL_SAFE_NO_PAD.encode(br#"{"alg":"ES256","typ":"JWT","kid":"k"}"#);
    let payload = format!(
        r#"{{"iss":"{iss}","sub":"u","tenant_id":"{tenant}","exp":9999999999}}"#
    );
    let p = URL_SAFE_NO_PAD.encode(payload.as_bytes());
    format!("{h}.{p}.AAAA")
}

#[test]
fn resolve_identity_no_bearer_is_anonymous() {
    let id = resolve_identity(None, "acme", 1000, |_, _, _| unreachable!(), "anon-sub").expect("ok");
    assert_eq!(id.sub, "anon-sub");
    assert!(!id.verified);
    assert!(id.email.is_none());
}

#[test]
fn resolve_identity_valid_bearer_is_verified() {
    let vi = crate::directline::oidc_verify::VerifiedIdentity {
        sub: "user-1".into(),
        email: Some("u@acme.example".into()),
        idp: "https://id.acme.example".into(),
        tenant_id: "acme".into(),
    };
    let id = resolve_identity(Some("tok"), "acme", 1000, move |_, _, _| Ok(vi.clone()), "anon-sub").expect("ok");
    assert_eq!(id.sub, "user-1");
    assert!(id.verified);
    assert_eq!(id.email.as_deref(), Some("u@acme.example"));
    assert_eq!(id.idp.as_deref(), Some("https://id.acme.example"));
}

#[test]
fn resolve_identity_bad_bearer_is_401() {
    let out = resolve_identity(
        Some("tok"),
        "acme",
        1000,
        |_, _, _| Err(crate::directline::oidc_verify::OidcVerifyError::InvalidSignature),
        "anon-sub",
    )
    .unwrap_err();
    assert_eq!(out.status, 401);
}

#[test]
fn resolve_identity_wrong_tenant_is_401() {
    let out = resolve_identity(
        Some("tok"),
        "acme",
        1000,
        |_, _, _| Err(crate::directline::oidc_verify::OidcVerifyError::TenantMismatch),
        "anon-sub",
    )
    .unwrap_err();
    assert_eq!(out.status, 401);
}

#[test]
fn pin_rejects_issuer_not_matching_config() {
    // Token's iss differs from the configured expected issuer → the fetch stub
    // must never be called; the pin rejects before any network access.
    let token = fake_bearer("https://evil.example", "acme");
    let err = verify_bearer_pinned(
        &token,
        "acme",
        1000,
        "https://id.acme.example",
        |_url| panic!("fetch must not run when issuer is pinned out"),
    )
    .unwrap_err();
    assert_eq!(err, crate::directline::oidc_verify::OidcVerifyError::JwksFetch("issuer not allowed".to_string()));
}

#[test]
fn pin_allows_matching_issuer_reaches_fetch() {
    // Matching issuer → pin passes → fetch runs. Stub returns non-JSON so we
    // land on JwksParse, proving execution got past the pin gate.
    let token = fake_bearer("https://id.acme.example", "acme");
    let err = verify_bearer_pinned(
        &token,
        "acme",
        1000,
        "https://id.acme.example",
        |_url| Ok("not json".to_string()),
    )
    .unwrap_err();
    assert_eq!(err, crate::directline::oidc_verify::OidcVerifyError::JwksParse);
}

#[test]
fn load_expected_issuer_reads_secret() {
    let mut secrets = TestSecretStore::new();
    secrets.insert(OIDC_ISSUER_KEY, b"https://id.acme.example");
    let req = build_request("POST", "/x", None, None, vec![]).expect("req");
    assert_eq!(
        load_expected_issuer(&req, &secrets).as_deref(),
        Some("https://id.acme.example")
    );
}

#[test]
fn load_expected_issuer_none_when_absent() {
    let secrets = TestSecretStore::new();
    let req = build_request("POST", "/x", None, None, vec![]).expect("req");
    assert!(load_expected_issuer(&req, &secrets).is_none());
}

#[test]
fn mint_with_bearer_but_no_issuer_config_is_401() {
    let mut state = InMemoryStateStore::new();
    let mut secrets = TestSecretStore::new();
    secrets.insert(TOKEN_SECRET_KEY, b"test-secret");
    // No oidc_issuer configured, but a bearer is presented → cannot establish trust → 401.
    let req = build_request(
        "POST",
        "/v3/directline/tokens/generate",
        Some("env=default&tenant=acme"),
        Some(&json!({"user": {"id": "alice"}})),
        vec![Header { name: "Authorization".into(), value: "Bearer some.jwt.token".into() }],
    )
    .expect("req");
    let resp = handle_directline_request(&req, &mut state, &secrets);
    assert_eq!(resp.status, 401);
}
```

Note the existing `directline_polling_flow` test (no `Authorization` header) must remain green — it exercises the anonymous path end-to-end.

- [ ] **Step 2: Run tests to verify they fail**

Run: `cd <R1> && cargo test -p messaging-provider-webchat --lib http`
Expected: FAIL to compile (`resolve_identity`, `verify_bearer_pinned`, `load_expected_issuer`, `OIDC_ISSUER_KEY` not found).

- [ ] **Step 3: Write the implementation**

First, in `oidc_verify.rs`, remove the module-level `#![allow(dead_code)]` line added in Task 1 (the verifier is now used).

In `http.rs`, add the const near `TOKEN_SECRET_KEY` (line 17):
```rust
const OIDC_ISSUER_KEY: &str = "oidc_issuer";
```

Add the helpers:
```rust
fn load_expected_issuer<SE: SecretStore>(request: &HttpInV1, secrets: &SE) -> Option<String> {
    if let Some(config) = &request.config {
        let config_key = format!("{OIDC_ISSUER_KEY}_b64");
        if let Some(b64) = config.get(&config_key).and_then(|v| v.as_str()) {
            if let Ok(bytes) = general_purpose::STANDARD.decode(b64) {
                let s = String::from_utf8(bytes).ok()?.trim().to_string();
                if !s.is_empty() {
                    return Some(s);
                }
            }
        }
    }
    match secrets.get(OIDC_ISSUER_KEY) {
        Ok(Some(bytes)) if !bytes.is_empty() => {
            let s = String::from_utf8(bytes).ok()?.trim().to_string();
            if s.is_empty() { None } else { Some(s) }
        }
        _ => None,
    }
}

fn verify_bearer_pinned<F>(
    token: &str,
    tenant: &str,
    now: i64,
    expected_issuer: &str,
    fetch: F,
) -> Result<VerifiedIdentity, OidcVerifyError>
where
    F: FnOnce(&str) -> Result<String, String>,
{
    let expected_jwks = oidc_verify::jwks_url_for_issuer(expected_issuer)
        .map_err(|_| OidcVerifyError::JwksFetch("invalid configured issuer".to_string()))?;
    oidc_verify::verify_oidc_bearer(token, tenant, now, |url| {
        if url != expected_jwks {
            return Err("issuer not allowed".to_string());
        }
        fetch(url)
    })
}

fn resolve_identity<V>(
    bearer: Option<&str>,
    tenant: &str,
    now: i64,
    verify: V,
    anon_sub: &str,
) -> Result<TokenIdentity, HttpOutV1>
where
    V: FnOnce(&str, &str, i64) -> Result<VerifiedIdentity, OidcVerifyError>,
{
    match bearer {
        None => Ok(TokenIdentity {
            sub: anon_sub.to_string(),
            email: None,
            idp: None,
            verified: false,
        }),
        Some(token) => match verify(token, tenant, now) {
            Ok(vi) => Ok(TokenIdentity {
                sub: vi.sub,
                email: vi.email,
                idp: Some(vi.idp),
                verified: true,
            }),
            Err(_) => Err(respond_error(401, "unauthorized", "invalid or unverifiable bearer token")),
        },
    }
}

fn mint_response(signing_key: &[u8], ctx: DirectLineContext, identity: &TokenIdentity) -> HttpOutV1 {
    match issue_token(signing_key, ctx, identity, None) {
        Ok((token, _exp)) => respond_json(200, json!({ "token": token, "expires_in": TTL_SECONDS })),
        Err(err) => respond_error(500, "token_issue_failed", format!("failed to mint token: {err:?}")),
    }
}
```

Rewrite `handle_tokens` (keep rate-limit + signing-key loading exactly as-is; swap the mint tail). The verify closure loads the expected issuer lazily — so when there is no bearer it is never consulted, and when a bearer is present but no issuer is configured, the `None → JwksFetch` maps to `401` via `resolve_identity`:
```rust
fn handle_tokens<S, SE>(request: &HttpInV1, state_store: &mut S, secrets: &SE) -> HttpOutV1
where
    S: StateStore,
    SE: SecretStore,
{
    let ctx = parse_context(request.query.as_deref());
    let body = match decode_json_body(request) {
        Ok(payload) => payload,
        Err(resp) => return resp,
    };
    let subject = determine_rate_limit_subject(request, &body);
    let now = Utc::now().timestamp();
    let cfg = RateLimitConfig::from_request(request);
    let rate_key = rate_limit_key(&ctx, &subject);
    if let Err(resp) = enforce_rate_limit(state_store, &rate_key, now, &cfg) {
        return resp;
    }

    let signing_key = match load_signing_key(request, secrets) {
        Ok(key) => key,
        Err(resp) => return resp,
    };

    let bearer = extract_bearer(&request.headers);
    let tenant = ctx.tenant.clone();
    let anon_sub = subject.token_subject().to_string();
    let identity = match resolve_identity(
        bearer.as_deref(),
        &tenant,
        now,
        |token, tenant, now| {
            let expected_issuer = load_expected_issuer(request, secrets)
                .ok_or_else(|| OidcVerifyError::JwksFetch("oidc issuer not configured".to_string()))?;
            verify_bearer_pinned(token, tenant, now, &expected_issuer, fetch_jwks_http)
        },
        &anon_sub,
    ) {
        Ok(id) => id,
        Err(resp) => return resp,
    };

    mint_response(&signing_key, ctx, &identity)
}
```

Add the HTTP-backed JWKS fetch (mirror of `ops/oauth.rs`'s `client::send`):
```rust
fn fetch_jwks_http(url: &str) -> Result<String, String> {
    use crate::bindings::greentic::http::http_client as client;
    let req = client::Request {
        method: "GET".into(),
        url: url.to_string(),
        headers: vec![],
        body: None,
    };
    match client::send(&req, None, None) {
        Ok(resp) if (200..300).contains(&resp.status) => {
            let bytes = resp.body.unwrap_or_default();
            String::from_utf8(bytes).map_err(|_| "jwks body not utf-8".to_string())
        }
        Ok(resp) => Err(format!("jwks endpoint status {}", resp.status)),
        Err(err) => Err(format!("jwks request failed: {}", err.message)),
    }
}
```

Add imports at the top of `http.rs`:
```rust
use crate::directline::jwt::TokenIdentity;
use crate::directline::oidc_verify::{self, OidcVerifyError, VerifiedIdentity};
```
(and remove the temporary shim added in Task 2, Step 4 for `handle_tokens` — it is now replaced. Leave the claims-forwarding in `handle_conversations`/`handle_refresh_token`/`handle_reconnect_conversation` intact; those correctly preserve verified identity across re-mint and are not part of this rewrite.)

- [ ] **Step 4: Run tests to verify they pass**

Run: `cd <R1> && cargo test -p messaging-provider-webchat --lib http` and `cargo test -p webchat-directline-core --lib`
Expected: PASS — the new `resolve_identity_*`, `pin_*`, `load_expected_issuer_*`, and `mint_with_bearer_but_no_issuer_config_is_401` tests, plus existing `directline_polling_flow` and all others.

- [ ] **Step 5: Build for wasm**

Run: `cd <R1> && cargo build -p messaging-provider-webchat --target wasm32-wasip2`
Expected: clean (confirms the `client::send` JWKS path compiles for wasip2). Note: `cargo component build` currently fails on a pre-existing, unrelated WIT gap (`greentic:provider-instance-identity@0.1.0`); the plain `--target wasm32-wasip2` build is the working gate until Task 6 rechecks it.

- [ ] **Step 6: fmt + clippy + commit**

`jwt.rs`/`http.rs` are shared into `webchat-directline-core` via `include!`, so `cargo fmt` does not reach them; format the physical files directly the way the pre-commit hook does:
```bash
cd <R1>
rustfmt --edition 2024 components/messaging-provider-webchat/src/directline/http.rs components/messaging-provider-webchat/src/directline/oidc_verify.rs
cargo clippy -p messaging-provider-webchat --all-targets -- -D warnings
git add components/messaging-provider-webchat/src/directline/http.rs components/messaging-provider-webchat/src/directline/oidc_verify.rs
git commit -m "feat(webchat): verify OIDC bearer at DirectLine mint with tenant-pinned issuer"
```

---

## Task 4: Emit verified-identity response headers on conversation start

`handle_conversations` already sets `X-Greentic-User` from `claims.sub`. Add `X-Greentic-Email` and `X-Greentic-Verified` from the (now richer) verified claims so the ingest path can carry them into the envelope.

**Files:**
- Modify: `components/messaging-provider-webchat/src/directline/http.rs`
- Test: inline tests in `http.rs`

**Interfaces:**
- Consumes: the `TokenClaims` returned by `verify_token` (now with `email`/`verified`).
- Produces: response headers `X-Greentic-Email` (only when `Some`) and `X-Greentic-Verified` (`"true"`/`"false"`) alongside the existing `X-Greentic-User`.

- [ ] **Step 1: Write the failing test**

The header block is inside `handle_conversations`, which needs a valid conv-bound token. Reuse the harness idiom. Add:

```rust
#[test]
fn conversation_start_emits_verified_identity_headers() -> Result<(), String> {
    use crate::directline::jwt::{issue_token, TokenIdentity, DirectLineContext};
    let mut state = InMemoryStateStore::new();
    let mut secrets = TestSecretStore::new();
    secrets.insert(TOKEN_SECRET_KEY, b"test-secret");

    // Mint a verified bootstrap token directly, then POST /conversations with it.
    let ctx = DirectLineContext { env: "default".into(), tenant: "acme".into(), team: None };
    let id = TokenIdentity { sub: "user-1".into(), email: Some("u@acme.example".into()), idp: Some("https://id.acme.example".into()), verified: true };
    let (token, _) = issue_token(b"test-secret", ctx, &id, None).map_err(|e| format!("{e:?}"))?;

    let req = build_request(
        "POST",
        "/v3/directline/conversations",
        Some("env=default&tenant=acme"),
        None,
        vec![Header { name: "Authorization".into(), value: format!("Bearer {token}") }],
    )?;
    let resp = handle_directline_request(&req, &mut state, &secrets);
    assert_eq!(resp.status, 201);
    let email = resp.headers.iter().find(|h| h.name.eq_ignore_ascii_case("X-Greentic-Email"));
    assert_eq!(email.map(|h| h.value.as_str()), Some("u@acme.example"));
    let verified = resp.headers.iter().find(|h| h.name.eq_ignore_ascii_case("X-Greentic-Verified"));
    assert_eq!(verified.map(|h| h.value.as_str()), Some("true"));
    Ok(())
}
```

(If `handle_conversations`' exact request shape differs, align the `build_request` args to what `directline_polling_flow` uses for its conversation POST — copy that call's path/query/headers.)

- [ ] **Step 2: Run test to verify it fails**

Run: `cd <R1> && cargo test -p messaging-provider-webchat --lib conversation_start_emits_verified_identity_headers`
Expected: FAIL (`X-Greentic-Email` header absent).

- [ ] **Step 3: Write the implementation**

In `handle_conversations`, next to the existing `X-Greentic-User` push (currently `http.rs:185-188`):
```rust
headers.push(Header {
    name: "X-Greentic-User".to_string(),
    value: claims.sub.clone(),
});
if let Some(email) = &claims.email {
    headers.push(Header {
        name: "X-Greentic-Email".to_string(),
        value: email.clone(),
    });
}
headers.push(Header {
    name: "X-Greentic-Verified".to_string(),
    value: if claims.verified { "true" } else { "false" }.to_string(),
});
```

- [ ] **Step 4: Run test to verify it passes**

Run: `cd <R1> && cargo test -p messaging-provider-webchat --lib conversation_start_emits_verified_identity_headers`
Expected: PASS.

- [ ] **Step 5: fmt + clippy + commit**

```bash
cd <R1>
cargo fmt -p messaging-provider-webchat
cargo clippy -p messaging-provider-webchat --all-targets -- -D warnings
git add components/messaging-provider-webchat/src/directline/http.rs
git commit -m "feat(webchat): emit verified email/flag headers on conversation start"
```

---

## Task 5: Stamp verified identity into the inbound envelope metadata

Read `X-Greentic-Email` / `X-Greentic-Verified` in the ingest conversation-start branch and write them into `envelope.metadata` (`auth.email`, `auth.verified`). This keeps `greentic-types` unchanged — `from.id` is already the verified `sub` once Task 3 lands. Only the auto-start (`/conversations`) branch carries these headers; the `/activities` branch keeps its body-derived `from.id` unchanged.

**Files:**
- Modify: `components/messaging-provider-webchat/src/ops/ingest.rs`
- Test: inline tests in `ingest.rs`

**Interfaces:**
- Consumes: `out.headers` containing `X-Greentic-Email` / `X-Greentic-Verified` (from Task 4); `build_webchat_envelope_with_ctx` (unchanged).
- Produces: `envelope.metadata["auth.verified"]` and (when present) `envelope.metadata["auth.email"]`.

- [ ] **Step 1: Write the failing test**

`handle_directline_path` is hard to unit-test in isolation (it invokes the inner handler). Extract a pure helper and test that:

Add to `ingest.rs`:
```rust
#[cfg(test)]
mod verified_identity_tests {
    use super::*;
    use greentic_types::MessageMetadata;

    #[test]
    fn stamps_auth_metadata_from_headers() {
        let headers = vec![
            crate::_h("X-Greentic-Email", "u@acme.example"),
            crate::_h("X-Greentic-Verified", "true"),
        ];
        let mut meta = MessageMetadata::new();
        stamp_auth_metadata(&mut meta, &headers);
        assert_eq!(meta.get("auth.verified").map(String::as_str), Some("true"));
        assert_eq!(meta.get("auth.email").map(String::as_str), Some("u@acme.example"));
    }

    #[test]
    fn omits_email_when_absent_defaults_unverified() {
        let mut meta = MessageMetadata::new();
        stamp_auth_metadata(&mut meta, &[]);
        assert_eq!(meta.get("auth.verified").map(String::as_str), Some("false"));
        assert!(meta.get("auth.email").is_none());
    }
}
```

If there is no existing `_h` header constructor helper, inline a local one in the test module instead:
```rust
fn h(name: &str, value: &str) -> greentic_types::messaging::universal_dto::Header {
    greentic_types::messaging::universal_dto::Header { name: name.into(), value: value.into() }
}
```
and use `h(...)` in place of `crate::_h(...)`.

- [ ] **Step 2: Run test to verify it fails**

Run: `cd <R1> && cargo test -p messaging-provider-webchat --lib stamps_auth_metadata_from_headers`
Expected: FAIL (`stamp_auth_metadata` not found).

- [ ] **Step 3: Write the implementation**

Add the helper to `ingest.rs`:
```rust
fn stamp_auth_metadata(metadata: &mut MessageMetadata, headers: &[Header]) {
    let verified = headers
        .iter()
        .find(|h| h.name.eq_ignore_ascii_case("X-Greentic-Verified"))
        .map(|h| h.value.trim() == "true")
        .unwrap_or(false);
    metadata.insert("auth.verified".to_string(), verified.to_string());
    if let Some(email) = headers
        .iter()
        .find(|h| h.name.eq_ignore_ascii_case("X-Greentic-Email"))
        .map(|h| h.value.trim())
        .filter(|v| !v.is_empty())
    {
        metadata.insert("auth.email".to_string(), email.to_string());
    }
}
```
Ensure `Header` and `MessageMetadata` are in scope (add `use` if needed — `Header` from `greentic_types::messaging::universal_dto`, `MessageMetadata` from `greentic_types`).

Then call it in the `/conversations` branch right after the envelope is built (around `ingest.rs:223-234`), using `out.headers` (which carry the identity headers from Task 4):
```rust
let mut envelope = build_webchat_envelope_with_ctx(
    String::new(),
    user_id,
    conv_id,
    None,
    &env_id,
    &tenant_id,
    BTreeMap::new(),
);
stamp_auth_metadata(&mut envelope.metadata, &out.headers);
envelope
    .metadata
    .insert("autoStart".to_string(), "true".to_string());
```

- [ ] **Step 4: Run test to verify it passes**

Run: `cd <R1> && cargo test -p messaging-provider-webchat --lib verified_identity_tests`
Expected: PASS.

- [ ] **Step 5: fmt + clippy + commit**

```bash
cd <R1>
cargo fmt -p messaging-provider-webchat
cargo clippy -p messaging-provider-webchat --all-targets -- -D warnings
git add components/messaging-provider-webchat/src/ops/ingest.rs
git commit -m "feat(webchat): stamp verified auth identity into envelope metadata"
```

---

## Task 6: Repo-wide green — build wasm + full local CI (R1)

Gate the whole R1 change set before crossing to `greentic-start`.

**Files:** none (verification only).

- [ ] **Step 1: Build all components for wasm**

Run: `cd <R1> && ./tools/build_components.sh`
Expected: all components build for `wasm32-wasip2`, including the new p256 verify path.

- [ ] **Step 2: Full workspace tests**

Run: `cd <R1> && cargo test --workspace`
Expected: PASS (no regression in provider-tests / other providers).

- [ ] **Step 3: Local CI**

Run: `cd <R1> && ./ci/local_check.sh`
Expected: fmt + clippy(-D warnings) + tests + repo extras all green. If a failure is outside this change's scope, note it in the PR summary rather than "fixing" unrelated code.

- [ ] **Step 4: (No commit unless local_check auto-formats something)**

If `local_check.sh` reformatted files, `git add -u && git commit -m "style: fmt"`.

---

## Task 7: (R2 `greentic-start`) Carry verified claims through renewal — session mint

`greentic-start` re-mints DirectLine tokens on a sliding window. Add the new fields to `DlClaims` and copy them verbatim in `mint_token`. **Preserve-only:** renewal must never set `verified` — it only copies what the parsed token had, so a `false`/absent token stays `false`.

**Files:**
- Modify: `greentic-start/src/directline_session.rs`
- Test: inline tests in `directline_session.rs`

**Interfaces:**
- Consumes: parsed `DlClaims` from an existing token.
- Produces: `DlClaims` gains `email: Option<String>`, `idp: Option<String>`, `verified: bool` (all `#[serde(default)]`); `mint_token` copies `template.email`/`template.idp`/`template.verified`.

- [ ] **Step 1: Write the failing test**

Using the module's existing `make_token`/`mint_token` idiom, add:
```rust
#[test]
fn renewal_preserves_verified_identity() {
    // A verified token, when re-minted, keeps email/idp/verified.
    let template = DlClaims {
        iss: TOKEN_ISS.to_string(),
        aud: TOKEN_AUD.to_string(),
        sub: "user-1".to_string(),
        iat: now_secs(),
        nbf: now_secs(),
        exp: now_secs() + 60,
        ctx: DlContext { env: Some("prod".into()), tenant: "acme".into(), team: None },
        conv: Some("conv-1".into()),
        email: Some("u@acme.example".into()),
        idp: Some("https://id.acme.example".into()),
        verified: true,
    };
    let key = b"k";
    let token = mint_token(&template, key, 60);
    // Re-parse the freshly minted token and confirm the fields survived.
    let payload = token.split('.').nth(1).unwrap();
    use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
    let claims: DlClaims = serde_json::from_slice(&URL_SAFE_NO_PAD.decode(payload).unwrap()).unwrap();
    assert!(claims.verified);
    assert_eq!(claims.email.as_deref(), Some("u@acme.example"));
    assert_eq!(claims.idp.as_deref(), Some("https://id.acme.example"));
}

#[test]
fn renewal_cannot_upgrade_unverified() {
    // A legacy/unverified template stays unverified after re-mint.
    let template = DlClaims {
        iss: TOKEN_ISS.to_string(),
        aud: TOKEN_AUD.to_string(),
        sub: "anon".to_string(),
        iat: now_secs(),
        nbf: now_secs(),
        exp: now_secs() + 60,
        ctx: DlContext { env: Some("prod".into()), tenant: "acme".into(), team: None },
        conv: Some("conv-1".into()),
        email: None,
        idp: None,
        verified: false,
    };
    let token = mint_token(&template, b"k", 60);
    let payload = token.split('.').nth(1).unwrap();
    use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
    let claims: DlClaims = serde_json::from_slice(&URL_SAFE_NO_PAD.decode(payload).unwrap()).unwrap();
    assert!(!claims.verified);
    assert!(claims.email.is_none());
}
```

Adjust `DlContext` field construction to match its exact definition (the extraction shows `DlContext { env, tenant, team }` with `env: Option<String>`). If `now_secs()` is private but in-module, it is reachable from the in-file test module.

- [ ] **Step 2: Run test to verify it fails**

Run: `cd <R2> && cargo test --lib renewal_preserves_verified_identity`
Expected: FAIL to compile (`DlClaims` has no `email`/`idp`/`verified`).

- [ ] **Step 3: Write the implementation**

Extend `DlClaims` (`directline_session.rs:193-204`):
```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
struct DlClaims {
    iss: String,
    aud: String,
    sub: String,
    iat: i64,
    nbf: i64,
    exp: i64,
    ctx: DlContext,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    conv: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    email: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    idp: Option<String>,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    verified: bool,
}
```

Update `mint_token` (`directline_session.rs:245-263`) to copy the new fields:
```rust
let claims = DlClaims {
    iss: TOKEN_ISS.to_string(),
    aud: TOKEN_AUD.to_string(),
    sub: template.sub.clone(),
    iat: now,
    nbf: now,
    exp: now + ttl_secs as i64,
    ctx: template.ctx.clone(),
    conv: template.conv.clone(),
    email: template.email.clone(),
    idp: template.idp.clone(),
    verified: template.verified,
};
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cd <R2> && cargo test --lib directline_session`
Expected: PASS (new tests + existing `mint_round_trips_through_parse` etc.). Existing tests that build `DlClaims` literals will now fail to compile until they include the new fields — update each existing `DlClaims { ... }` literal in the test module to add `email: None, idp: None, verified: false`.

- [ ] **Step 5: fmt + clippy + commit**

```bash
cd <R2>
cargo fmt
cargo clippy --all-targets --all-features -- -D warnings
git add src/directline_session.rs
git commit -m "feat(start): preserve verified identity through DirectLine renewal"
```

---

## Task 8: (R2 `greentic-start`) Verify-side claim struct tolerates new fields

`DirectLineTokenClaims` (the partial verify-side struct) deserializes provider tokens. It must not break on the new claims and should expose `verified`/`email` for any downstream consumer. Since it uses named deserialization, unknown fields are already ignored — but add the fields explicitly (defaulted) so `greentic-start` can read verified identity if needed.

**Files:**
- Modify: `greentic-start/src/directline_token.rs`
- Test: inline tests in `directline_token.rs`

**Interfaces:**
- Produces: `DirectLineTokenClaims` gains `#[serde(default)] pub email: Option<String>`, `#[serde(default)] pub idp: Option<String>`, `#[serde(default)] pub verified: bool`.

- [ ] **Step 1: Write the failing test**

Using the module's `make_token`/`make_claims` idiom, add a claims JSON that includes the new fields and assert they round-trip:
```rust
#[test]
fn verify_reads_verified_identity_claims() {
    let key = b"test-key";
    let exp = chrono::Utc::now().timestamp() + 60;
    let claims_json = format!(
        r#"{{"sub":"user1","exp":{exp},"ctx":{{"env":"prod","tenant":"t1"}},"conv":"conv1","email":"u@acme.example","idp":"https://id.acme.example","verified":true}}"#
    );
    let token = make_token(&claims_json, key);
    let claims = verify_directline_token(&token, "conv1", "t1", key).expect("verify ok");
    assert!(claims.verified);
    assert_eq!(claims.email.as_deref(), Some("u@acme.example"));
    assert_eq!(claims.idp.as_deref(), Some("https://id.acme.example"));
}

#[test]
fn verify_defaults_unverified_when_absent() {
    let key = b"test-key";
    let exp = chrono::Utc::now().timestamp() + 60;
    let token = make_token(&make_claims("conv1", "t1", exp), key); // legacy claims, no new fields
    let claims = verify_directline_token(&token, "conv1", "t1", key).expect("verify ok");
    assert!(!claims.verified);
    assert!(claims.email.is_none());
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cd <R2> && cargo test --lib verify_reads_verified_identity_claims`
Expected: FAIL to compile (`claims.verified` / `.email` don't exist).

- [ ] **Step 3: Write the implementation**

Extend `DirectLineTokenClaims` (`directline_token.rs:24-31`):
```rust
#[derive(Debug, Deserialize)]
pub struct DirectLineTokenClaims {
    pub sub: String,
    pub exp: i64,
    pub ctx: DirectLineTokenContext,
    #[serde(default)]
    pub conv: Option<String>,
    #[serde(default)]
    pub email: Option<String>,
    #[serde(default)]
    pub idp: Option<String>,
    #[serde(default)]
    pub verified: bool,
}
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cd <R2> && cargo test --lib directline_token`
Expected: PASS (new tests + existing `valid_token_passes` etc.).

- [ ] **Step 5: fmt + clippy + full local CI + commit**

```bash
cd <R2>
cargo fmt
cargo clippy --all-targets --all-features -- -D warnings
bash ci/local_check.sh   # if present in greentic-start; else: cargo test --all-targets --all-features
git add src/directline_token.rs
git commit -m "feat(start): expose verified identity on DirectLine verify claims"
```

---

## Task 9: Update the OIDC integration contract doc

**Files:**
- Modify: `greentic-tenant-manager/docs/WEBCHAT_OIDC_INTEGRATION.md`

- [ ] **Step 1: Document the verified-mint contract**

Add a section describing:
- The mint accepts optional `Authorization: Bearer <OIDC access_token>` on `POST /v1/messaging/webchat/{tenant}/token`.
- Verification: ES256 against the tenant issuer JWKS (`GET <issuer>/jwks.json`, host-root path), `iss` = tenant issuer, `exp` valid, and **`tenant_id` claim must equal the route `{tenant}`** or the request is rejected `401`.
- On success the minted DirectLine token carries `sub` (verified), `email`, `idp`, `verified: true`; the conversation-start response emits `X-Greentic-Verified: true` and `X-Greentic-Email`; the inbound envelope carries `metadata["auth.verified"]` / `metadata["auth.email"]`.
- With no bearer, behavior is unchanged (anonymous, `verified: false`).
- Renewal preserves identity and never upgrades `verified`.

- [ ] **Step 2: Commit**

```bash
cd /home/bima-pangestu/projects/Works/greentic/greentic-tenant-manager
git add docs/WEBCHAT_OIDC_INTEGRATION.md
git commit -m "docs: document webchat verified-identity mint contract"
```

---

## Self-Review (completed during authoring)

- **Spec coverage:** A1 mint-accepts-bearer → Tasks 1+3; A2 claim shape → Task 2; A3 renewal preserve/no-upgrade → Tasks 7+8; A4 envelope stamping → Tasks 4+5; A5 invariants (401 on mismatch, anon preserved, no self-assert) → Tasks 3 (401 + anon) and 2 (`verified` only settable by the mint, not client body). Docs → Task 9. `greentic-types` change: **descoped** — `from.id` already derives from `claims.sub`; email/verified ride in metadata (rationale in Task 5). This is an intentional deviation from the spec's "possibly greentic-types" note and is safe (no breaking `Actor` change).
- **Placeholder scan:** every code step has concrete code; test bodies are complete; no TBD/TODO.
- **Type consistency:** `TokenIdentity { sub, email, idp, verified }` used identically in Tasks 2 & 3; `VerifiedIdentity { sub, email, idp, tenant_id }` in Tasks 1 & 3; header names `X-Greentic-Email` / `X-Greentic-Verified` and metadata keys `auth.email` / `auth.verified` consistent across Tasks 4 & 5; `DlClaims`/`DirectLineTokenClaims` new fields consistent across Tasks 7 & 8.
- **Open risk to watch during execution:** the exact `handle_conversations` request/response wiring (Task 4) and the `/conversations` ingest branch (Task 5) — align the test `build_request` calls with the existing `directline_polling_flow` harness if the assumed shapes differ. p256 feature set (`ecdsa` vs `ecdsa`+`arithmetic`) — resolve at Task 1 Step 2/7.
