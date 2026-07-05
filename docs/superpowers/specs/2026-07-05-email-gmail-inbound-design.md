# EPIC-E1-b — Gmail Inbound Parity for `messaging-provider-email` — Design Spec

**Status:** Draft for review (NO build) — 2026-07-05
**Initiative:** Agentic platform coverage PRD, EPIC-E1 "Channels". E1 v1 shipped conversational SMS; conversational email already works via MS Graph. This slice adds **Gmail inbound** so Google-Workspace tenants reach the same conversational email path.

## 1. Scope reality (read before estimating)

Reconnaissance of `components/messaging-provider-email` on research:
- `ingest_http` is **100% MS-Graph-specific**: `GET` → `handle_validation` (Graph subscription validation token echo); `POST` → `handle_graph_notifications` → parse Graph change notifications → `fetch_graph_message` via Graph API → `channel_message_envelope`.
- `config.rs` is entirely `graph_*` fields (`graph_client_id/secret/refresh_token/token_endpoint/scope/base_url/...`).
- **No Gmail anything** today.

Therefore Gmail inbound is a **full second email backend**, comparable in size to the SMS slice — NOT a small parity tweak. It is **unit-testable offline** (envelope/message parse) but **cannot be verified end-to-end without live Google OAuth + a Cloud Pub/Sub push subscription** (deferred to a pre-enablement checklist, exactly like the SMS signature/URL item). Its value is lower than SMS's (email already works via Graph). **This spec exists so the scope is explicit before committing build effort.**

## 2. How Gmail inbound differs from Graph

| | MS Graph (existing) | Gmail (new) |
|---|---|---|
| Inbound transport | Graph change-notification webhook (direct POST) | **Google Cloud Pub/Sub push** — Gmail `users.watch` publishes to a Pub/Sub topic; a push subscription POSTs to our ingress |
| Validation | GET echoes `validationToken` | Pub/Sub push carries an OIDC bearer / a shared verification token in the URL query (no GET handshake) |
| Notification body | `value[].resourceData.id` (message id) | `message.data` = base64(JSON `{emailAddress, historyId}`) — a **historyId**, not a message id |
| Fetch new mail | `GET /messages/{id}` | `users.history.list?startHistoryId=<h>` → new message ids → `users.messages.get?id=<id>&format=full` |
| Auth | Graph OAuth (`graph_*`) | Google OAuth2 (`gmail_*`: client_id/secret/refresh_token, scope `gmail.readonly`) |
| Parse | Graph message JSON → envelope | Gmail message (`payload.headers[]` From/Subject, `payload.parts[]`/`body.data` base64url) → same envelope |

## 3. Architecture (additive, off by default)

A `kind` discriminator on the email config selects the backend; the existing Graph path is untouched and remains the default.

### 3.1 Config
Add `kind: EmailKind` (`#[serde(default)] → Graph`; `Graph | Gmail`) + a parallel `gmail_*` block (`gmail_client_id`, `gmail_client_secret`, `gmail_refresh_token`, `gmail_token_endpoint` default Google's, `gmail_scope` default `https://www.googleapis.com/auth/gmail.readonly`, `gmail_pubsub_verification_token` for the push shared-secret, `gmail_user` = the mailbox address). Add these to the allowed-keys list + `describe.rs` schema + `secret_requirements` (the secrets are tenant-scoped). Existing `graph_*`-only tenants deserialize with `kind: Graph` and behave identically.

### 3.2 `ingest_http` branch
```
ingest_http:
  parse HttpInV1 (unchanged)
  match cfg.kind {
    Graph => existing GET handle_validation / POST handle_graph_notifications   // UNCHANGED
    Gmail => gmail::handle_gmail_push(&http, &cfg)
  }
```
Reading `cfg.kind` requires the config at the ingress point (the Graph path already reads `http.config`), so no new plumbing.

### 3.3 `gmail::handle_gmail_push`
1. **Verify** the Pub/Sub push: compare `?token=` (or an `Authorization` bearer) against `gmail_pubsub_verification_token`; mismatch → `403` (fail closed, mirroring the SMS signature gate). (Full OIDC-JWT verification of the Google-signed push token is a hardening follow-up; v1 uses the shared verification token Google supports on the push endpoint URL.)
2. **Decode** the Pub/Sub envelope: `body.message.data` (base64) → JSON `{emailAddress, historyId}`.
3. **Acquire** a Google access token from `gmail_refresh_token` (mirror `auth::acquire_graph_token`, but Google's token endpoint + a `gmail_*` variant `acquire_google_token`).
4. **Fetch** new messages: `users.history.list?startHistoryId=<historyId>&historyTypes=messageAdded` → collect new message ids → for each, `users.messages.get?id=<id>&format=full`.
5. **Map** each Gmail message → `ChannelMessageEnvelope` (channel `"email"`, from = `From` header, text = the text/plain part decoded from base64url, subject in metadata, correlation = message id, tenant from injected config) via a shared `channel_message_envelope` (reuse/adapt the Graph one).
6. Return `HttpOutV1 { status: 200, events: [...] }` (Pub/Sub needs a 200 to ack; a non-2xx makes Pub/Sub redeliver).

### 3.4 Egress (send) — out of scope for E1-b
`messaging-provider-email` already sends via Graph. Gmail **send** (`users.messages.send`) is a separate follow-up (E1-c); E1-b is inbound parity only. A Gmail-inbound tenant that also wants to *reply* via Gmail needs E1-c — documented as a known limitation (v1 Gmail is receive-only; reply would attempt the Graph send path, which won't be configured).

## 4. Pack / config seam
The existing `packs/messaging-email` pack gains the `gmail_*` config fields + a QA setup path for `kind: gmail` (mirror the Graph setup). A tenant selects Gmail by setting `kind: gmail` + the `gmail_*` secrets. Graph tenants are unaffected.

## 5. Testing (offline)
- **Pub/Sub envelope parse:** base64 `message.data` → `{emailAddress, historyId}`; malformed → 400; missing token / wrong token → 403 (fail closed).
- **Gmail message → envelope:** a sample `users.messages.get` JSON (From/Subject headers + a base64url text/plain part) → `ChannelMessageEnvelope` with the right channel/from/text/subject/correlation; multipart + HTML-only fallback.
- **Backend selection:** `kind: Graph` (or absent) → the Graph path is taken (existing tests unchanged); `kind: Gmail` → the Gmail branch.
- The live path (OAuth token, history.list, messages.get, real Pub/Sub push) is **not** unit-tested (no live Google) — covered by the pre-enablement checklist.

## 6. Pre-enablement checklist (cannot be automated here — needs live Google infra)
Before pointing a real Gmail mailbox at this:
1. Google OAuth2 app + `gmail.readonly` scope + a refresh token for the mailbox.
2. A Cloud Pub/Sub topic + `users.watch` on the mailbox publishing to it.
3. A Pub/Sub **push** subscription targeting `/v1/messaging/ingress/messaging-email/<tenant>` with the verification token.
4. Verify a real inbound email produces a `ChannelMessageEnvelope` end-to-end and the agent replies (reply needs E1-c / Graph-send, see §3.4).

## 7. Scope boundaries (YAGNI)
**In v1 (if built):** Gmail inbound (Pub/Sub push → history/messages → envelope), `kind` discriminator, offline parse tests, off-by-default, tenant-scoped `gmail_*` secrets.
**Deferred:** Gmail **send** (E1-c); full OIDC-JWT verification of the Google push token (v1 = shared verification token); Gmail label/thread mapping; attachments; the `users.watch` renewal cron (Gmail watches expire in 7 days — an operator/host concern).

## 8. Recommendation
E1-b is a legitimate but **large, lower-value, live-unverifiable** slice (a full Gmail backend; MS Graph email already covers conversational email). Recommend building it **only if Google-Workspace tenants are a near-term requirement**; otherwise defer in favour of higher-value work (EPIC-C business-event triggers, or unblocking EPIC-H/F). This spec documents the real scope so that decision is informed.
