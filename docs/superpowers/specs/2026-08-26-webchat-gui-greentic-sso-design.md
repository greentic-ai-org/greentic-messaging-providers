# WebChat GUI — Greentic SSO via `@greentic/sso`

**Date:** 2026-08-26
**Status:** Approved design, pending implementation plan
**Scope:** `packs/messaging-webchat-gui`, `components/messaging-provider-webchat{,-gui}`, root `package.json`, `tools/`, `tests/webchat-gui/`

## Goal

Replace the hand-rolled OAuth/PKCE implementation in the WebChat GUI with the
`@greentic/sso` SDK, and make Greentic SSO the first and default-enabled login
option. Bind the DirectLine chat token to the verified SSO identity.

## Background

The GUI ships a prebuilt SPA plus a hand-maintained `runtime-bootstrap.js`
that inlines its own PKCE implementation, login overlay, and session handling.
Provider entries reach the browser from `GET /v1/messaging/webchat/{tenant}/auth/config`,
composed in `compose_oauth_providers()`. The DirectLine mint at
`/v1/messaging/webchat/{tenant}/token` is unauthenticated: `handle_tokens`
never reads `Authorization`, and no OIDC verification exists anywhere in this
workspace.

The SDK (`@greentic/sso` v0.1.0, repo `greenticai/greentic-webchat-sdk`, not
published to npm) is a zero-dependency PKCE public client. Its `/webchat` entry
adds `getChatToken()`, which POSTs the OIDC access token to
`<chatApiBase>/token` and expects `{token, expires_in}` back — the "Part A
verified mint" contract that this workspace does not yet implement.

Greentic SSO is served by greentic-tenant-manager: ES256 (P-256) signatures,
JWKS at `<issuer>/jwks.json`, access tokens are stateless JWTs carrying
`iss`, `sub` (`{did_web}:users:{user_id}`), `aud` (= `client_id`), `scope`,
`exp`, `iat`, TTL 3600s.

## Decisions

| # | Decision | Rationale |
|---|---|---|
| D1 | Vendor a committed IIFE bundle built by esbuild from a git-pinned dep | `ci/steps/` has no Node; `cargo test` and pack build must run without `npm install` |
| D2 | Keep the existing redirect PKCE path as the `popup_blocked` fallback | The SDK is popup-only and `await`s before `window.open`, so Safari/Firefox will block it |
| D3 | Let the `greentic` provider appear in both webchat packs | `describe.rs` and `ops/` are shared via `#[path]`; Greentic SSO is meaningful for both |
| D4 | Ship as two PRs from one spec | Layer D is roughly half the work and touches the shared provider |
| D5 | `oauth_enabled` keeps its `false` default | "Default option" means default *among providers*, not "turn auth on for every tenant" |
| D6 | Verify ES256 locally against JWKS, not via `/oauth/introspect` | Introspection needs a per-tenant `client_secret`; local verify needs none |

## Architecture

Four layers. A–C ship in PR 1, D in PR 2.

### Layer A — build pipeline

- Root `package.json`: dependency
  `"@greentic/sso": "github:greenticai/greentic-webchat-sdk#2696c1083f90886f82b65aab4573d1b7458925c0"`,
  devDependency `esbuild`.
- `tools/webchat-sso/entry.js` imports the core **and** the `/webchat` entry and
  assigns both to `window.GreenticSso`. Required because only the core entry has
  an IIFE build (`tsup.config.ts` gives `webchat` and `react` esm+cjs only).
- `tools/build_webchat_sso_bundle.sh` runs esbuild (`--bundle --format=iife`)
  and writes `packs/messaging-webchat-gui/assets/webchat-gui/greentic-sso.js`.
  The file sits at the top level of the asset root because
  `tools/import_webchat_gui_assets.sh` rsyncs `assets/`, `config/`, `i18n/`
  and `js/` with `--delete`.
- The bundle is committed. A drift check (rebuild, then `git diff --exit-code`)
  runs in `.github/workflows/webchat-gui-playwright.yml`, which already
  provisions Node and already triggers on `packs/messaging-webchat-gui/**`.
- The `<script src="./greentic-sso.js">` tag is added to the `index.html`
  heredoc inside `tools/import_webchat_gui_assets.sh`, not to `index.html`,
  which that script regenerates on every import.
- CI installing the git dependency must not pass `--omit=dev` or
  `--ignore-scripts`: the SDK's `dist/` is gitignored and produced by its
  `prepare` script.

### Layer B — client auth gate

`applyAuthConfig()` in `runtime-bootstrap.js` branches on `provider.type`:

- `"greentic"` builds a client via
  `GreenticSso.createGreenticWebchatSso({tenant, issuer, clientId, redirectUri, chatApiBase})`
  and calls `login()`. On success `getChatToken()` becomes the DirectLine token
  source. `chatApiBase` is set explicitly to the same-origin
  `/v1/messaging/webchat/<tenant>`; the SDK's default derives it from the
  issuer, which its own README warns is rarely correct.
- Every other type keeps the existing `initiateOAuthFlow()` path unchanged.

Supporting changes:

- **New callback page** `assets/webchat-gui/sso-callback.html`, a minimal page
  whose only job is `completeCallbackFromPopup()`. Its absolute URL is the
  `redirectUri` that must be registered per tenant in the Greentic admin
  managed-SSO redirect-URIs surface (exact string match, no wildcards).
- **`popup_blocked` fallback (D2).** `login()` is wrapped; a `GreenticSsoError`
  with code `popup_blocked` falls through to the existing redirect PKCE flow
  pointed at `<issuer>/oauth/authorize` and `<issuer>/oauth/token`. The SDK's
  own session store is not used on this path; the existing `greentic_oauth_*`
  session keys are, so both paths converge on one session shape. Because there
  is no SDK client on this path there is no `getChatToken()` either, so the
  chat token is minted by calling the SDK's exported
  `mintChatToken(chatApiBase, accessToken)` directly with the access token from
  the redirect exchange. Both paths therefore hit the same mint contract.
- **DirectLine cache correctness.** `directLineCacheKey()` gains an identity
  component, and `performLogout()` calls `clearDirectLineCache()`. Today
  neither is true, so a token minted for one user is served from
  `localStorage` to the next user of the same browser after logout. This is a
  pre-existing defect that identity-bound tokens would otherwise make worse.
- **`/token` fetch branch** attaches `Authorization: Bearer <access_token>`
  whenever an SSO session is active.
- **Login overlay i18n.** The overlay's strings are currently hardcoded English
  while `i18n/en.json` already ships `login.title`, `login.subtitle`,
  `login.loginWith` and `login.noProviders`, used only by the React bundle's
  fallback login page. The overlay is routed through `uiT()` against those keys.

The two existing login surfaces (runtime overlay when `/auth/config` returns
`enabled: true`; React SPA page when it 404s) both remain. The `greentic`
provider is wired into the runtime overlay only; the React fallback keeps its
current behaviour.

### Layer C — configuration and defaults

- `compose_oauth_providers()` gains a `greentic` branch pushing
  `{id: "greentic", label: "Greentic SSO", type: "greentic", issuer, client_id}`
  as the **first** element, which makes it the first button in the overlay.
- New QA questions: `oauth_enable_greentic` (default `true`),
  `oauth_greentic_issuer`, `oauth_greentic_client_id` (default `webchat-gui`).
  The existing `oauth_bool_question()` helper hardcodes `default: false`, so a
  default-true variant is needed.
- The new toggle must be added to the `has(...)` chain in `apply_answers`, or
  the answers are silently dropped.
- New config fields are added to `ProviderConfig`, `ProviderConfigOut` and both
  allowed-key lists in `config.rs`, in the GUI component and its non-GUI mirror.
- `config_schema()`, `I18N_KEYS` and `I18N_PAIRS` in the shared `describe.rs`
  gain the matching entries. `i18n_keys_cover_qa_specs()` enforces the pairing.
- Both `config.schema.json` and `public.config.schema.json` are hand-written,
  carry `additionalProperties: false`, and are already missing every `oauth_*`
  field that `config_schema()` declares. They are updated by hand to add the
  new fields; closing the pre-existing gap is explicitly **out of scope**.
- `config/tenants/default.json` and `greentic.json` list the `greentic`
  provider first. Note `import_webchat_gui_assets.sh` fully rewrites
  `default.json`, so its heredoc is the place to change it.

### Layer D — verified chat-token mint

In `handle_tokens()` (`components/messaging-provider-webchat/src/directline/http.rs`):

```
Authorization: Bearer present?
├─ no  → anonymous DirectLine token (today's behaviour, unchanged)
└─ yes → GET <oidc_issuer>/jwks.json via the existing http_client import
         verify ES256 signature
         pin iss == oidc_issuer
         pin aud == oidc_audience
         check exp / nbf with the Utc::now() pattern verify_token already uses
         require scope to contain oidc_required_scope
         ├─ any check fails → 401   (SDK contract: 401 on bad or unpinned bearer)
         └─ all pass        → DirectLine token with sub = OIDC sub, verified: true
```

- New dependency: `p256` (RustCrypto), `default-features = false`, feature
  `ecdsa`. Pure Rust and safe on `wasm32-wasip2`. Deliberately not
  `jsonwebtoken`, which pulls `ring`.
- New config fields: `oidc_issuer`, `oidc_audience` (default `webchat-gui`),
  `oidc_required_scope` (default `greentic.webchat`).
- JWKS responses are cached in the existing `HostStateStore`.
- The DirectLine claim struct in `jwt.rs` gains `verified: bool`. `sub` carries
  the OIDC subject on the verified path and the guest UUID / IP hash /
  `"anonymous"` otherwise, exactly as today.
- Wall-clock access already exists — `jwt.rs` calls `chrono::Utc::now()` — so no
  new capability is required. The `wasi.clocks: false` declaration in
  `pack.yaml` is already inaccurate; correcting it is out of scope.

## Testing

**Rust.** Unit tests for the verifier: valid token, expired, wrong `iss`,
wrong `aud`, missing scope, corrupted signature, malformed JWKS. Existing
`schema_hash_is_stable()` and `i18n_keys_cover_qa_specs()` cover the describe
changes.

**Fixtures.** `tools/regenerate_registry_fixtures.sh` — the shared `describe.rs`
change drifts `tests/fixtures/registry/webchat/*.cbor`. Also
`packs/messaging-webchat-gui/fixtures/setup.input.json` and
`setup.expected.plan.json`.

**Playwright.** Extend the `/auth/config` mock in
`tests/webchat-gui/fixtures/server.mjs` with a `greentic` provider; add specs
to `fullscreen.spec.ts` and `embedded.spec.ts` covering the SSO button, the
popup-blocked fallback, and logout clearing the DirectLine cache. The visual
baseline `login-page-chromium-linux.png` must be refreshed because the overlay
gains a button and switches to translated strings.

**Manual.** `scripts/test_webchat_gui.sh --login` injects its auth providers
from an inline Python block; a `greentic` entry is added there.

## Out of scope

- Reconciling the pre-existing drift between the two JSON schemas and
  `config_schema()` beyond the new fields.
- Removing the dead `oauth.js`, `locale-picker.js` and `js/tenant-resolver.js`,
  which still ship in the `.gtpack` but are never executed.
- Correcting `wasi.clocks: false` in `pack.yaml`.
- Adding a redirect mode to the SDK itself. D2 works around its absence; a
  proper fix belongs in `greenticai/greentic-webchat-sdk`.
- Wiring the `greentic` provider into the React SPA fallback login page.

## Sequencing

**PR 1 — SDK login (layers A, B, C).** Greentic SSO appears as the first login
option and completes a real login. The chat token stays anonymous, so nothing
regresses if the mint is not yet verified.

**PR 2 — verified mint (layer D).** `/token` honours the bearer, and the chat
token becomes identity-bound.

## Prerequisites outside this repo

Per tenant, via the Greentic admin surface: an OIDC public client (`webchat-gui`
by platform convention, no secret), the exact `sso-callback.html` URL registered
as a redirect URI, and `oidc_issuer` set on the tenant's webchat provider so the
mint trusts that issuer.
