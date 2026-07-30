# Webchat SSO — Verified Identity + JS SDK

**Date:** 2026-07-30
**Status:** Design approved, pending implementation plan
**Scope:** Cross-repo epic — `greentic-messaging-providers`, `greentic-start`, (possibly) `greentic-types`, and a new `@greentic/webchat-sso` TypeScript SDK repo.

## Motivation

Maarten's request: *"make sure the webchat GUI (the default option for SSO) supports Greentic SSO so that any webchat can reuse SSO login with passkey."* Bima committed to shipping an SDK to make integration easy.

The target is **third-party / custom webchats** — any web chat UI, not only the Greentic-provided GUI, should be able to log a user in via Greentic SSO (with passkey) and have that identity bound to the running chat session.

### The load-bearing finding

The webchat GUI already carries a full OIDC/PKCE gate (`packs/messaging-webchat-gui/assets/webchat-gui/oauth.js`), and Tenant Manager (TM) is a standards OIDC issuer where passkey/WebAuthn login happens inside its hosted `/login` page. However, **today there is no server-side binding of an OIDC identity to a DirectLine chat token**:

- `handle_tokens` (`components/messaging-provider-webchat/src/directline/http.rs:89-126`) sets the DirectLine JWT `sub` from a **client-supplied, unverified** `user.id` (fallback: hashed IP / `"anonymous"`). It never reads `Authorization` and never verifies OIDC.
- `POST /oauth/token-exchange` (`src/ops/oauth.rs`) is a pure PKCE code-exchange proxy (keeps `client_secret` off the browser) and forwards the raw provider token back to the browser. It does not associate identity with any conversation/session.
- Identity reaches the flow only **in-band**, as a literal `oauth_login_success` chat message. The flow learns "someone clicked login" but the `sub` is spoofable.

Therefore an SDK alone is necessary but not sufficient: SSO+passkey that lives only in the browser is "login theater" — the flow/agent cannot trust who the user is. Making "bind chat session" meaningful requires a **new server contract**. This design covers both parts, unified by that contract.

## The unifying contract (the seam)

> The DirectLine token mint accepts an optional `Authorization: Bearer <OIDC access_token>`. When present and valid, the mint verifies it against the tenant's TM issuer and issues a DirectLine token that carries **server-verified** identity claims. When absent, the current anonymous behavior is preserved unchanged.

Part A provides this contract; Part B consumes it. It is the single source of truth both parts agree on and must be frozen before Part B implementation begins.

### Mint request/response contract

`POST /v1/messaging/webchat/{tenant}/token` (rewritten to `/v3/directline/tokens/generate`):

- **Headers:** optional `Authorization: Bearer <oidc_access_token>`.
- **Verification (when bearer present):** ES256 signature against the tenant issuer JWKS (`GET <issuer>/jwks.json`); `iss` equals the tenant issuer; `exp`/`nbf` valid; **the token's `tenant_id` claim equals the route `{tenant}`**. On any failure → `401` (do not silently downgrade to anonymous — a caller that presents a bearer is asserting an authenticated intent).
- **On success:** DirectLine JWT `sub` = verified OIDC `sub`; adds `email`, `idp`, `verified: true`.
- **When no bearer:** unchanged — `verified: false`, `sub` from client `user.id` / hashed IP / `"anonymous"`. Anonymous webchats keep working (backward compatible).

## Part A — Verified-identity DirectLine mint (Rust)

**Repos:** `greentic-messaging-providers` (primary), `greentic-start` (renewal), possibly `greentic-types` (`Actor`).

### A1 — Mint accepts and verifies OIDC bearer
`handle_tokens` (`components/messaging-provider-webchat/src/directline/http.rs:89-126`, single-sourced — also serves webchat-gui via `webchat_directline_core`): read `Authorization: Bearer`, verify per the contract above, and set the verified subject at the one spot the subject enters the JWT (`http.rs:112`) instead of `subject.token_subject()`.

- Outbound HTTP to fetch TM JWKS is already available in this component (`src/ops/oauth.rs` performs outbound HTTP for the OIDC token exchange). Add a small JWKS cache (keyed by issuer, TTL) to avoid a fetch per mint.
- **Reuse-first:** before writing new ES256/JWKS verification, evaluate reusing an existing verifier — the shared `greentic-oauth` crate (listed in this repo's reuse-first policy) first, then `greentic-designer-admin/src/auth/oidc/{verifier,jwks_cache,claims}.rs` or the TM `oidc_crypto*` modules. If none is consumable from a `wasm32-wasip2` component, add a minimal ES256 P-256 verify + JWKS cache and document the justification.

### A2 — Claim shape
`TokenClaims` (`components/messaging-provider-webchat/src/directline/jwt.rs:23-34`) and `issue_token` (`jwt.rs:71-101`): add `email: Option<String>`, `idp: Option<String>`, `verified: bool` (defaults to `false`). Serialization must keep existing tokens deserializable (new fields optional / `#[serde(default)]`).

### A3 — Renewal carries claims, cannot upgrade
`greentic-start`: `DirectLineTokenClaims` (`src/directline_token.rs:24-31`) and `DlClaims` / `mint_token` (`src/directline_session.rs:193-263`) thread the new fields through sliding-window renewal so refreshed tokens keep the verified identity.

- **Security invariant:** renewal must **never** upgrade `verified` from `false` to `true`, nor change `sub`/`email`/`idp`. Only the original OIDC-bearing mint (A1) may set `verified: true`. Renewal preserves verbatim.

### A4 — Envelope stamping
`components/messaging-provider-webchat/src/ops/envelope.rs` (`build_webchat_envelope*`, ~`:59`/`:98`) and `src/ops/ingest.rs:163-234`: when the DirectLine token has `verified: true`, source the envelope `from` identity from the verified token `sub`/`email` rather than the client-supplied activity `from.id` / `X-Greentic-User` header. When unverified, keep current behavior.

- **Reuse-first:** inspect `ChannelMessageEnvelope` / `Actor` in `greentic-types/src/messaging.rs:77` first. If `Actor` cannot express "verified identity + email", add minimal fields there (with justification) rather than forking a local type.

### A5 — Security invariants (summary)
- A client cannot self-assert `verified: true` — the server sets it only after successful OIDC verification.
- Tenant mismatch between the OIDC token and the route tenant → `401`.
- Absence of a bearer → anonymous path preserved (no behavioral regression for existing webchats).
- Renewal is preservation-only for identity claims.

## Part B — `@greentic/webchat-sso` SDK (TypeScript)

**Repo:** new, published under the `@greentic/` npm scope (the only scope in use in the ecosystem). Core is zero-dependency TypeScript built in library mode (tsup → ESM + CJS + IIFE + `.d.ts`). A thin React wrapper ships via a `/react` subpath export. Vue is out of scope for v1 (YAGNI; add later if demanded).

### B1 — Core API surface
```ts
const sso = createGreenticSso({
  tenant: "acme",                             // tenant slug
  issuer?: "https://id.acme.greentic-id.com", // else derived from tenant
  clientId: "webchat-gui",                    // registered OIDC client id
  redirectUri: window.location.origin + "/gt-sso-callback",
  chatApiBase: "https://.../v1/messaging/webchat/acme", // for the identity-bound mint
  scope?: "openid profile email greentic.webchat",
});

await sso.login();                 // opens popup → passkey at TM → resolves identity
sso.onIdentity(cb);                // subscribe: { sub, email, name, verified }
const dlToken = await sso.getChatToken(); // identity-bound DirectLine token (Part-A mint + bearer)
sso.getSession();                  // current session (tokens + identity) or null
await sso.logout();                // clears session; optional TM end_session
```

### B2 — Popup PKCE flow (public client)
- Generate PKCE `code_verifier` + `code_challenge` (`S256`) and a random `state`/`nonce`.
- Open a popup to `<issuer>/oauth/authorize?response_type=code&client_id=...&code_challenge=...&code_challenge_method=S256&state=...&redirect_uri=...&scope=...`.
- TM's hosted `/login` runs the passkey ceremony (discoverable/conditional-UI or typed-email). The SDK never calls `navigator.credentials` itself — passkey stays at the IdP.
- The `redirectUri` page calls the SDK helper `completeCallbackFromPopup()`, which `postMessage`s `{ type: 'greentic-sso', code, state }` back to the opener (origin-checked). Mirror the existing `greentic-designer-admin/web/src/features/auth/ssoPopup.ts` convention (`postMessage {type:'greentic-sso'}`).
- The opener validates `state`, exchanges `code` → tokens at `<issuer>/oauth/token` (public client, PKCE, no secret).

### B3 — Chat-session binding
`getChatToken()` calls the Part-A mint: `POST <chatApiBase>/token` with `Authorization: Bearer <access_token>`. Returns an identity-bound DirectLine token, cached and refreshed before `exp`. This is the seam that makes the flow/agent trust the user.

### B4 — Session management
Tokens held in memory by default (optional `sessionStorage` persistence, opt-in). Silent refresh via `refresh_token` (or re-auth) tracked by `expires_at`. `logout()` clears local state and optionally hits TM `end_session`.

### B5 — Security
- Public client; access/id tokens live in the browser — consistent with the existing `WEBCHAT_OIDC_INTEGRATION.md` contract.
- CSRF via `state`; replay protection via `nonce`.
- Popup `postMessage` origin validation (only accept messages from the configured issuer origin).
- `redirectUri` must be an exact-match registered redirect URI.

### B6 — Integrator prerequisite (non-code)
The integrator registers their webchat origin as an exact-match redirect URI via the admin self-service "Managed-SSO redirect-URIs" surface (`greentic-designer-admin/web/src/features/tenants/tabs/sso/`). Wildcards are rejected; production must be HTTPS.

## Testing

### Part A (Rust)
- Valid bearer → `verified: true`, `sub`/`email` from OIDC claims.
- Expired / bad-signature / wrong-issuer bearer → `401`.
- Wrong-tenant bearer (`tenant_id` != route tenant) → `401`.
- No bearer → anonymous path unchanged (`verified: false`).
- A client-supplied `verified: true` in the body/claims is ignored — server is authoritative.
- Renewal preserves identity claims and **cannot** upgrade `verified` `false` → `true`.
- Inbound envelope carries verified `from` identity when the token is verified; unchanged otherwise.

### Part B (vitest)
- PKCE verifier/challenge generation (S256 correctness).
- `state` validation rejects mismatched/missing state.
- Popup `postMessage` handler ignores foreign origins; accepts the configured issuer origin.
- Code → token exchange (mocked issuer).
- `getChatToken()` attaches the bearer and caches/refreshes on expiry.
- `logout()` clears session.
- Optional Playwright e2e against a dummy TM issuer.

## Documentation
- Update `greentic-tenant-manager/docs/WEBCHAT_OIDC_INTEGRATION.md` with the verified-mint contract and the `verified`/`email`/`idp` claims.
- Add a `greentic-docs` page for the SDK (install, config, popup callback wiring, redirect-URI registration).
- Ship a runnable example (vanilla + React) wiring the SDK to a webchat.

## Implementation sequencing

One design doc (this file); the contract above is the frozen seam. The implementation plan is **phased**:

1. **Phase A** — server verified-mint (`greentic-messaging-providers` + `greentic-start` + `greentic-types` if needed). Land tests. **Freeze the mint contract.**
2. **Phase B** — `@greentic/webchat-sso` SDK against the frozen contract.
3. **Phase C** — docs + example.

Part B begins only after the Phase A contract is frozen and merged (or at least interface-stable behind tests).

## Non-goals (v1)
- Inline passkey inside the webchat (calling `navigator.credentials` cross-origin to TM). Passkey stays at the TM hosted `/login`.
- A bundled backend-for-frontend (BFF). The SDK is a browser public client; no mandatory integrator backend.
- Vue/Svelte wrappers (React only for v1).
- Full-page-redirect transport (popup only for v1; redirect can be added later).

## Key anchor files
- Mint / subject assignment: `components/messaging-provider-webchat/src/directline/http.rs:89-126` (`:112`)
- Token claims: `components/messaging-provider-webchat/src/directline/jwt.rs:23-34`, `:71-101`
- OIDC proxy (existing): `components/messaging-provider-webchat/src/ops/oauth.rs`
- Envelope: `components/messaging-provider-webchat/src/ops/envelope.rs`, `src/ops/ingest.rs:163-234`
- Renewal: `greentic-start/src/directline_token.rs:24-31`, `src/directline_session.rs:193-263`
- Envelope type: `greentic-types/src/messaging.rs:77`
- Existing OIDC contract to update: `greentic-tenant-manager/docs/WEBCHAT_OIDC_INTEGRATION.md`
- SDK house-style references: `greentic-designer-admin/web/src/features/auth/{publicAuth.ts,ssoPopup.ts,ssoErrors.ts}`, `greentic-designer/web/src/api/client.ts`
- Embed precedent: `greentic-messaging-providers/docs/guides/webchat-gui-embed-webcomponent.md`, `packs/messaging-webchat-gui/assets/webchat-gui/embed.js`
