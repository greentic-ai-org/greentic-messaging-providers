# Gmail Inbound Parity (EPIC-E1-b) Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** `messaging-provider-email` gains a Gmail inbound backend (Cloud Pub/Sub push → `users.history.list` + `users.messages.get` → `ChannelMessageEnvelope`), selected by a `kind` config discriminator, off by default (existing MS-Graph tenants unchanged). Offline-testable; live Google OAuth + Pub/Sub verification is a pre-enablement checklist.

**Architecture:** Additive. A `kind: EmailKind { Graph (default) | Gmail }` on the config branches `ingest_http`; the Graph path is untouched. A new `src/gmail/` module holds the Pub/Sub-push parse, the Google-OAuth token acquire, the Gmail API fetch, and the Gmail-message→envelope mapping, reusing the existing `channel_message_envelope` shape + the `http-client` WIT import.

**Tech Stack:** Rust (edition 2024), `wasm32-wasip2`, the component's existing `http-client`/`secrets-store` WIT imports (no reqwest), `serde_json`, base64/base64url.

## Global Constraints

- **Reference:** `components/messaging-provider-email/src/{ingress.rs, config.rs, auth.rs, graph.rs}` — mirror the Graph inbound shape (validation/notification/fetch/envelope) for the Gmail equivalents; reuse `channel_message_envelope`.
- **`kind` discriminator:** `#[serde(default)]` → `Graph`, so every existing `graph_*`-only config deserializes as Graph and behaves byte-identically. No behavior change for Graph tenants.
- **Cross-envelope target:** the produced value is `greentic_types::ChannelMessageEnvelope` (channel `"email"`), same as the Graph path — so the host routes it identically.
- **Fail-closed inbound:** the Pub/Sub push is gated by a shared `gmail_pubsub_verification_token` (compare `?token=` / bearer, constant-time); mismatch/missing → `403`, no events. (Full OIDC-JWT verification of the Google-signed push token is a hardening follow-up.)
- **Best-effort/robust:** malformed body → `400`; a Gmail API/token error → log + `HttpOutV1{status:200, events:[]}` (a non-2xx makes Pub/Sub redeliver forever — so on a transient fetch error we ACK with no events rather than loop); never panic.
- **Secrets tenant-scoped:** `gmail_client_secret`, `gmail_refresh_token`, `gmail_pubsub_verification_token` declared in `describe()` `secret_requirements` (`scope: "tenant"`); `component.manifest.json` auto-generated (Task 5).
- **WIT world:** shared `component-v0-v6-v0` (unchanged); use the existing `http-client` import for Gmail REST (NO new dep).
- **Text-only v1:** extract the `text/plain` part (base64url); HTML-only → strip-to-text or store raw in metadata; attachments dropped with a metadata note.
- **Conventional commits, NO Claude co-author.** Target `research`.
- **Build discipline (SHARED CONTENDED MACHINE — ~8 concurrent cargo builds; naive builds OOM):** all cargo with `-j2` + `CARGO_BUILD_JOBS=2`; FOREGROUND; block+wait. NEVER pkill/kill or delete another worktree's `target/`. Prefer host-target `cargo test -p messaging-provider-email` for unit tests (Task 1's config test established host tests work for this crate); the `wasm32-wasip2` build is the deferred Task-5 gate.
- **Live verification is OUT OF SCOPE here** (no live Google) — the OAuth token, `history.list`, `messages.get`, and real Pub/Sub push are covered by the spec §6 pre-enablement checklist, not by CI. Unit tests cover the pure parse/map logic only.

---

### Task 1: `kind` discriminator + `gmail_*` config

**Files:**
- Modify: `components/messaging-provider-email/src/config.rs` (add `EmailKind` + `gmail_*` fields + allowed-keys)
- Modify: `components/messaging-provider-email/src/describe.rs` (schema fields + `secret_requirements`)
- Test: inline `#[cfg(test)]`

**Interfaces:**
- Produces: `enum EmailKind { Graph, Gmail }` (`#[derive(Default)]` with `#[default] Graph`, serde `rename_all="lowercase"`); `ProviderConfig` gains `#[serde(default)] kind: EmailKind` + `gmail_client_id/gmail_client_secret/gmail_refresh_token/gmail_token_endpoint/gmail_scope/gmail_user/gmail_pubsub_verification_token: Option<String>`.

- [ ] **Step 1: Read `config.rs`** — the `ProviderConfig`/`ProviderConfigOut` structs, the allowed-keys list (`load_config`), and how `graph_*` secrets are surfaced. Read `describe.rs` `config_schema()` + `secret_requirements`.
- [ ] **Step 2: Failing tests** — a config with `kind: "gmail"` + `gmail_*` fields deserializes with `kind == EmailKind::Gmail`; a config with only `graph_*` fields deserializes with `kind == EmailKind::Graph` (default); an unknown key is still rejected as before.
- [ ] **Step 3: Run — expect FAIL** (`CARGO_BUILD_JOBS=2 cargo test -p messaging-provider-email -j2 config`).
- [ ] **Step 4: Implement** `EmailKind` + fields + allowed-keys + `describe.rs` schema + 3 `gmail_*` `secret_requirements` (`scope: tenant`).
- [ ] **Step 5: Run — PASS + commit** (`feat(email): kind discriminator + gmail config fields`).

---

### Task 2: Pub/Sub push parse + verification gate

**Files:**
- Create: `components/messaging-provider-email/src/gmail/mod.rs` (`pub mod push; pub mod fetch; pub mod envelope;`) + `src/gmail/push.rs`
- Modify: `src/lib.rs` or the module root (`mod gmail;`)
- Test: inline

**Interfaces:**
- Produces: `struct PushNotification { email_address: String, history_id: String }`; `fn parse_pubsub_push(body: &[u8]) -> Result<PushNotification, String>` (decode `{message:{data:<base64>}}` → base64-decode `data` → JSON `{emailAddress, historyId}`); `fn verify_push(http: &HttpInV1, expected_token: &str) -> bool` (compare `?token=` query / bearer, constant-time; missing → false).

- [ ] **Step 1: Failing tests** — a sample Pub/Sub push body (`{"message":{"data":"<base64 of {\"emailAddress\":\"a@b.com\",\"historyId\":\"123\"}>"},"subscription":"..."}`) → `PushNotification{email_address:"a@b.com", history_id:"123"}`; malformed → `Err`; `verify_push` true for the right token, false for wrong/missing (constant-time compare — reuse the crate's compare helper if one exists, else a simple fixed-length XOR fold).
- [ ] **Step 2: Run — expect FAIL.**
- [ ] **Step 3: Implement** `parse_pubsub_push` + `verify_push`.
- [ ] **Step 4: Run — PASS + commit** (`feat(email): gmail pub/sub push parse + verification gate`).

---

### Task 3: Google OAuth token + Gmail API fetch

**Files:**
- Create: `components/messaging-provider-email/src/gmail/fetch.rs`
- Modify: `src/auth.rs` (add `acquire_google_token`, mirroring `acquire_graph_token`)
- Test: inline for the pure request-builders; the live HTTP is NOT unit-tested (no live Google)

**Interfaces:**
- Produces: `fn acquire_google_token(cfg: &ProviderConfig) -> Result<String, String>` (POST Google token endpoint with `grant_type=refresh_token` + `gmail_client_id/secret/refresh_token`, via the `http-client` WIT import; parse `access_token`); `fn list_history(token, cfg, start_history_id) -> Result<Vec<String>, String>` (`GET .../users/<user>/history?startHistoryId=<h>&historyTypes=messageAdded` → collect `history[].messagesAdded[].message.id`); `fn get_message(token, cfg, id) -> Result<serde_json::Value, String>` (`GET .../users/<user>/messages/<id>?format=full`).

- [ ] **Step 1: Read `auth.rs`/`graph.rs`** — how `acquire_graph_token` + `graph_get` use the `http-client` WIT import (headers, method, body, response parse). Mirror exactly for Google/Gmail.
- [ ] **Step 2: Failing tests** — pure request-shape tests: `list_history`/`get_message` build the expected URLs (path + query) for a given user/id; `acquire_google_token` builds the expected form body. (Extract URL/body construction into pure helpers so they're testable without HTTP.)
- [ ] **Step 3: Run — expect FAIL.**
- [ ] **Step 4: Implement** the three fns (thin `http-client` calls; the URL/body builders are the tested part). Non-2xx/parse error → `Err(msg)`. Never panic.
- [ ] **Step 5: Run — PASS + commit** (`feat(email): google oauth token + gmail history/messages fetch`).

---

### Task 4: Gmail message → envelope + `ingest_http` kind-branch

**Files:**
- Create: `components/messaging-provider-email/src/gmail/envelope.rs`
- Modify: `components/messaging-provider-email/src/ingress.rs` (branch on `cfg.kind`)
- Test: inline

**Interfaces:**
- Consumes: Tasks 1-3.
- Produces: `fn gmail_message_to_envelope(msg: &serde_json::Value, cfg: &ProviderConfig, tenant: &TenantCtx) -> Option<ChannelMessageEnvelope>` (headers `From`/`Subject` from `payload.headers[]`; text from the `text/plain` part `body.data` base64url-decoded, walking `payload.parts[]`; channel `"email"`, correlation = message `id`, subject in metadata); `fn handle_gmail_push(http: &HttpInV1, cfg: &ProviderConfig) -> Vec<u8>` (verify → parse push → acquire token → list_history → get_message per id → map → `HttpOutV1{200, events}`).

- [ ] **Step 1: Failing tests** — `gmail_message_to_envelope` on a sample `messages.get` JSON (From/Subject headers + a base64url text/plain part) → envelope with the right channel/from/text/subject/correlation; a multipart message picks the text/plain part; an HTML-only message falls back (raw or stripped) with a metadata note. `handle_gmail_push` with a bad token → `403`; with a malformed body → `400`. (The token/fetch calls are stubbed or the test drives only the verify+parse+map portion — factor `handle_gmail_push` so the fetch step is injectable, mirroring how you'd test without live HTTP.)
- [ ] **Step 2: Run — expect FAIL.**
- [ ] **Step 3: Implement** `gmail_message_to_envelope` + `handle_gmail_push`; wire `ingest_http` to `match cfg.kind { Graph => <existing>, Gmail => handle_gmail_push(...) }`. The Graph arm must be the EXISTING code, byte-identical.
- [ ] **Step 4: Run — PASS + commit** (`feat(email): gmail message→envelope + ingest_http kind branch`).

---

### Task 5: pack config + wasm build + manifest + gate (DEFERRED-heavy)

**Files:**
- Modify: `packs/messaging-email/pack.yaml` (+ QA setup for `kind: gmail`), the pack's config schema
- Modify: `components/messaging-provider-email/component.manifest.json` (auto-generated — do NOT hand-edit)

- [ ] **Step 1: Pack config** — add the `gmail_*` fields + `kind` to the pack's `provider-extension.v1` config schema + `setup_contract` (answers → `gmail_*` secrets); mirror the Graph setup. A tenant selects Gmail via `kind: gmail`.
- [ ] **Step 2: Build the wasm** — `SKIP_WASM_TOOLS_VALIDATION=1 CARGO_BUILD_JOBS=2 ./tools/build_components/messaging-provider-email.sh` (or via `tools/build_components.sh`, `-j2`). Fix any wasm-only issues.
- [ ] **Step 3: Regenerate the manifest** from `describe()` (`greentic-component manifest`) so `component.manifest.json` (incl. the new `gmail_*` `secret_requirements`) matches; validate with `bash ci/steps/09_component_doctor.sh` + `05_check_op_schemas.sh`; `bash ci/steps/07_sync_packs.sh`.
- [ ] **Step 4: Gate** — `cargo fmt --all`; `CARGO_BUILD_JOBS=2 cargo clippy --workspace -j2 -- -D warnings`; `CARGO_BUILD_JOBS=2 cargo test --workspace -j2`; `bash ci/steps/11_build_packs.sh`. Commit (`chore(email): gmail pack config + wasm build + manifest`).
- [ ] **Step 5:** finishing-a-development-branch → push + PR to `research`, with the spec §6 pre-enablement checklist in the PR body (live Google OAuth + Pub/Sub verification is NOT covered by CI).

---

## Self-Review

- **Spec coverage:** §3.1 config → Task 1; §3.3 step 1 verify + step 2 parse → Task 2; §3.3 step 3 token + step 4 fetch → Task 3; §3.3 step 5 map + §3.2 branch → Task 4; §4 pack + §5 build → Task 5; §5 offline tests → per-task; §6 pre-enablement (live) → Task 5 Step 5 PR note. §3.4 Gmail send explicitly out of scope (E1-c).
- **Placeholder scan:** "read config.rs/auth.rs/graph.rs" + "mirror acquire_graph_token" are deliberate — the exact `http-client` WIT idioms + config/secret surfacing must be read from the repo. Every task names its reference. The fetch layer's HTTP is intentionally not unit-tested (no live Google — spec §6); the URL/body builders ARE tested. No TBD as work-defining.
- **Type consistency:** `EmailKind`/`gmail_*` (Task 1) consumed by Tasks 2-4; `PushNotification`/`parse_pubsub_push`/`verify_push` (Task 2) ↔ Task 4 `handle_gmail_push`; `acquire_google_token`/`list_history`/`get_message` (Task 3) ↔ Task 4; `gmail_message_to_envelope` → `ChannelMessageEnvelope` (channel `"email"`) matches the Graph path.
- **Scope:** additive Gmail backend (config + 3 gmail/ files + ingress branch + pack); Graph path byte-identical; one plan; live verification deferred to the pre-enablement checklist.
