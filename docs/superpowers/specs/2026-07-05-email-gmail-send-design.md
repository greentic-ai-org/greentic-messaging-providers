# EPIC-E1-c — Gmail Send (outbound) for `messaging-provider-email` — Design Spec

**Status:** Draft — 2026-07-05
**Initiative:** Agentic platform coverage PRD, EPIC-E1 channels. E1-b added Gmail **inbound**; this slice adds Gmail **outbound send** so a `kind: gmail` tenant can actually reply (completes the Gmail roundtrip). Off by default; Graph tenants unchanged.

## 1. Problem & goal

E1-b (`#269`) added a Gmail inbound backend selected by `kind: gmail`, but the **send** path (`ops.rs::send_payload`) is MS-Graph-only (`POST {graph}/users/{user}/sendMail`). So a Gmail-inbound tenant receives mail but their agent's reply still attempts the Graph send path (unconfigured → fails). **Goal:** a Gmail send branch — build an RFC 2822 MIME message, base64url-encode it, and `POST users/{user}/messages/send` with the Google OAuth token — selected by the same `kind` discriminator, reusing E1-b's `acquire_google_token`. Off by default; Graph send byte-identical.

## 2. Recon

- `ops.rs::send_payload` (~:341) is the outbound send; it builds the Graph `sendMail` URL (~:449) and posts the message. `handle_send` (~:22) is the render/prepare step (deterministic payload + ids).
- E1-b already provides `auth::acquire_google_token(cfg)` + the `gmail_*` config + `kind` discriminator (on research).
- The Graph send builds a Graph message JSON; Gmail send needs a different shape: `{"raw": "<base64url(MIME)>"}`.

## 3. Architecture (additive, off by default)

### 3.1 `send_payload` kind-branch
```
send_payload:
  parse config
  match cfg.kind {
    Graph => <existing Graph sendMail path, byte-identical>,
    Gmail => gmail::send::gmail_send(&cfg, &prepared),
  }
```
`handle_send` (render/prepare) is backend-agnostic (it produces To/From/Subject/body) — reused unchanged; only the wire-send differs.

### 3.2 `gmail::send::gmail_send`
1. Build an **RFC 2822 MIME message** from the prepared send (`To`, `From` = `gmail_user`, `Subject`, `Date`, `MIME-Version`, `Content-Type: text/plain; charset=UTF-8`, body). A single-part text/plain message (v1; HTML/multipart deferred). CRLF line endings.
2. **base64url-encode** (URL-safe, no pad) the raw MIME bytes → `raw`.
3. `acquire_google_token(cfg)` (reuse E1-b's) → Bearer token.
4. `POST https://gmail.googleapis.com/gmail/v1/users/<gmail_user>/messages/send` with JSON body `{"raw": "<...>"}`, `Authorization: Bearer <token>`, via the `http-client` WIT import (mirror how the Graph send + E1-b fetch issue authenticated calls). Non-2xx → structured send error; 2xx → parse the returned message `id` → success confirmation (mirror the Graph send's success shape).
5. Never panic; a token/HTTP error maps to the same structured send-failure the Graph path returns.

### 3.3 Scope: `gmail.send`
Gmail send requires an OAuth scope with send capability (`https://www.googleapis.com/auth/gmail.send` or `gmail.modify`). E1-b defaulted `gmail_scope` to `gmail.readonly`. This slice: the `gmail_scope` config is tenant-set; **document that a tenant using Gmail inbound+send must grant a scope covering both** (e.g. `gmail.modify`, or both `gmail.readonly gmail.send`). No code default change needed (scope is on the refresh token, not per-request) — a note in the pack setup + PR.

## 4. Failure semantics
Gmail send failure (token or non-2xx) returns the SAME structured send-error the Graph path returns (so the host egress handles it identically). No panic. Off unless `kind: gmail`.

## 5. Scope boundaries (YAGNI)
**In v1:** single-part text/plain Gmail send (MIME build + base64url + `messages.send`), `kind` branch in `send_payload`, offline tests (MIME shape, base64url, request URL/body), off-by-default, reuse `acquire_google_token`.
**Deferred:** HTML/multipart/attachments send; threading (In-Reply-To/References headers for proper reply-threading — v1 sends a fresh message); the `gmail.send` scope enforcement (documented, tenant-configured); rate-limit/retry.

## 6. Testing (offline)
- **MIME build:** To/From/Subject/body → a well-formed RFC 2822 message (correct headers, CRLF, UTF-8 text/plain).
- **base64url:** the MIME bytes encode URL-safe no-pad; round-trips.
- **Request shape:** `gmail_send` builds the `.../users/<user>/messages/send` URL + `{"raw": ...}` body (pure builder tested; the live HTTP is NOT unit-tested — no live Google, spec §7).
- **Backend selection:** `kind: Graph` → the Graph send path unchanged (existing tests green); `kind: Gmail` → the Gmail branch.

## 7. Pre-enablement checklist (live — not CI)
Before a Gmail tenant replies live: the refresh token must carry a **send-capable scope** (`gmail.send`/`gmail.modify`); verify a real reply is delivered via `messages.send`. (Same posture as E1-b inbound — the pure builder is unit-tested; the live call is a pre-enablement item.)

## 8. Rollout
Additive; off unless `kind: gmail`; reuses E1-b auth; Graph send byte-identical; no admin change. Target `research`. Completes the Gmail roundtrip (E1-b inbound + E1-c send).
