# Gmail Send (EPIC-E1-c) Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** `messaging-provider-email` gains a Gmail **send** branch (RFC 2822 MIME → base64url → `users/<user>/messages/send` via the Google OAuth token), selected by the existing `kind` discriminator. Off by default; the MS-Graph send path is byte-identical. Completes the Gmail roundtrip (E1-b inbound + E1-c send).

**Architecture:** A new `src/gmail/send.rs` builds the MIME message + issues `messages.send` (reusing E1-b's `auth::acquire_google_token` + the `http-client` WIT import). `ops.rs::send_payload` branches on `cfg.kind` (Graph arm unchanged).

**Tech Stack:** Rust (edition 2024), `wasm32-wasip2`, the component's `http-client`/`secrets-store` WIT imports (no reqwest), base64url, `serde_json`.

## Global Constraints

- **Reference:** `components/messaging-provider-email/src/ops.rs` (`handle_send` ~:22, `send_payload` ~:341, Graph `sendMail` ~:449) + `src/gmail/fetch.rs` + `src/auth.rs::acquire_google_token` (E1-b) — mirror the E1-b fetch's `http-client` + auth idioms for the Gmail send.
- **`kind` branch:** `send_payload` matches `cfg.kind` (from E1-b); the `Graph` arm is the EXISTING code verbatim (byte-identical). `handle_send` (render/prepare) is backend-agnostic — reused unchanged.
- **Gmail send wire:** `POST https://gmail.googleapis.com/gmail/v1/users/<gmail_user>/messages/send`, JSON `{"raw": "<base64url(MIME), URL-safe no-pad>"}`, `Authorization: Bearer <token>`. Non-2xx → the SAME structured send-error the Graph path returns; 2xx → parse the returned `id`. Never panic.
- **MIME:** single-part `text/plain; charset=UTF-8`, CRLF line endings, headers `To`/`From` (= `gmail_user`)/`Subject`/`Date`/`MIME-Version: 1.0`/`Content-Type`. HTML/multipart/attachments/threading deferred.
- **Off by default:** only `kind: gmail` takes the Gmail branch; Graph/absent → unchanged.
- **No new deps** (base64 already a dep from E1-b). **Conventional commits, NO Claude co-author.** Target `research`.
- **Live send is OUT OF SCOPE here** (no live Google) — the MIME/base64url/request builders are unit-tested; the live `messages.send` + the `gmail.send` scope requirement are the spec §7 pre-enablement checklist.
- **Build discipline (SHARED CONTENDED MACHINE — ~8 concurrent builds, OOM risk):** all cargo with `-j2` + `CARGO_BUILD_JOBS=2`; FOREGROUND, block+wait; NEVER pkill/kill or delete another worktree's `target/`. Host-target tests for Rust logic; the `wasm32-wasip2` build is Task 2's gate.

---

### Task 1: MIME builder + `gmail_send`

**Files:**
- Create: `components/messaging-provider-email/src/gmail/send.rs` (+ `pub mod send;` in `src/gmail/mod.rs`)
- Test: inline `#[cfg(test)]`

**Interfaces:**
- Consumes: `ProviderConfig` (E1-b `gmail_*`), `auth::acquire_google_token`, the `http-client` WIT import.
- Produces:
  - `fn build_mime(to: &str, from: &str, subject: &str, body: &str) -> String` — RFC 2822 single-part text/plain, CRLF, UTF-8.
  - `fn gmail_send_url(user: &str) -> String` → `https://gmail.googleapis.com/gmail/v1/users/<user>/messages/send`.
  - `fn gmail_send(cfg: &ProviderConfig, to: &str, subject: &str, body: &str) -> Result<String, String>` — build MIME → base64url(no-pad) → acquire token → POST `{raw}` → parse `id` on 2xx, `Err(msg)` otherwise.

- [ ] **Step 1: Read `ops.rs`'s `send_payload` + the Graph send** (how it reads `cfg`, the prepared To/From/Subject/body, how it issues the authenticated POST + parses success/error) and `gmail/fetch.rs` (the `http-client` idiom + `acquire_google_token` usage).
- [ ] **Step 2: Failing tests** — `build_mime` produces the expected headers + CRLF + a text/plain body for given To/From/Subject/body; `gmail_send_url("me@x.com")` == the expected URL; base64url of the MIME is URL-safe no-pad and round-trips. (The live POST is NOT unit-tested — factor so the MIME/url/base64 builders are pure + tested; `gmail_send`'s HTTP is the live path.)
- [ ] **Step 3: Run — expect FAIL** (`CARGO_BUILD_JOBS=2 cargo test -p messaging-provider-email -j2 gmail::send`).
- [ ] **Step 4: Implement** `build_mime`, `gmail_send_url`, `gmail_send` (mirror the Graph send's `http-client` POST + E1-b's `acquire_google_token`). Non-2xx → structured error matching the Graph send's error shape.
- [ ] **Step 5: Run — PASS + commit** (`feat(email): gmail MIME builder + messages.send`).

---

### Task 2: `send_payload` kind-branch + wasm build + manifest + gate + merge

**Files:**
- Modify: `components/messaging-provider-email/src/ops.rs` (`send_payload` branch on `cfg.kind`)
- Modify: `packs/messaging-email/` (pack setup note for the `gmail.send`/`gmail.modify` scope), `component.manifest.json` (auto-regenerated)
- Test: inline (`kind: Graph` → Graph send unchanged; `kind: Gmail` → the Gmail branch)

**Interfaces:**
- Consumes: Task 1's `gmail_send`.

- [ ] **Step 1: Failing test** — `send_payload` with `kind: Gmail` routes to the Gmail branch (assert via the request-builder seam / a provider-mismatch-style check, mirroring the existing `send_payload_rejects_provider_mismatch...` test at ops.rs:622); with `kind: Graph` (or absent) → the existing Graph path (existing send tests green).
- [ ] **Step 2: Run — expect FAIL.**
- [ ] **Step 3: Implement** the `match cfg.kind { Graph => <existing>, Gmail => gmail::send::gmail_send(...) }` branch in `send_payload`. The Graph arm MUST be the existing code byte-identical. Wire the prepared To/Subject/body from `handle_send`'s output into `gmail_send`.
- [ ] **Step 4: Pack note** — add a setup note / config hint in `packs/messaging-email` that a Gmail tenant's `gmail_scope` must cover send (`gmail.send` or `gmail.modify`) for outbound. `sync_packs`.
- [ ] **Step 5: Build wasm + manifest** — `SKIP_WASM_TOOLS_VALIDATION=1 CARGO_BUILD_JOBS=2 ./tools/build_components/messaging-provider-email.sh`; regenerate `component.manifest.json` from describe (component_doctor validates — no new secrets, so the manifest may be unchanged; confirm `09_component_doctor.sh` passes).
- [ ] **Step 6: Gate + commit** — `cargo fmt --all`; `bash ci/steps/09_component_doctor.sh` + `05_check_op_schemas.sh` + `07_sync_packs.sh` + `11_build_packs.sh`; `CARGO_BUILD_JOBS=2 cargo clippy --workspace -j2 -- -D warnings`; `CARGO_BUILD_JOBS=2 cargo test -p messaging-provider-email -j2` (+ workspace if feasible; ignore the pre-existing telegram-webhook fixture failures). Commit (`feat(email): gmail send branch in send_payload + pack scope note`). Then finishing-a-development-branch → PR to `research` with the spec §7 pre-enablement checklist (live `messages.send` + `gmail.send` scope) in the body.

---

## Self-Review

- **Spec coverage:** §3.2 MIME+send → Task 1; §3.1 branch → Task 2; §3.3 scope note → Task 2 Step 4; §6 offline tests → per-task; §7 pre-enablement → Task 2 Step 6 PR note. §5 deferred (HTML/threading/scope-enforcement) → out of plan.
- **Placeholder scan:** "read ops.rs's send_payload / the Graph send / gmail/fetch.rs" are deliberate — the exact `http-client` POST idiom + success/error shape must be read from the repo (Gmail send must return the SAME structured shape). No TBD as work-defining. Live HTTP intentionally not unit-tested (spec §7).
- **Type consistency:** `build_mime`/`gmail_send_url`/`gmail_send` (Task 1) consumed by Task 2's `send_payload` branch; reuses E1-b's `acquire_google_token` + `kind`/`gmail_*` config.
- **Scope:** 1 new small file + `ops.rs` branch + pack note; Graph send byte-identical; reuses E1-b auth; one plan; live send deferred.
