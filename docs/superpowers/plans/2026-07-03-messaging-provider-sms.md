# messaging-provider-sms (Twilio, conversational) Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** A new `wasm32-wasip2` messaging-provider component `messaging-provider-sms` (Twilio) that receives inbound SMS as a `ChannelMessageEnvelope`, lets the tenant's flow/agent reply, and sends the reply back over the Twilio REST API — routed through the existing `greentic-start` front door with zero host change.

**Architecture:** Clone the structure of `components/messaging-provider-whatsapp` (closest analog: single sender id, phone `Destination`, POST-ingest + egress `render_plan`/`encode`/`send_payload`). Port the Twilio inbound-parse + outbound-build logic from the native crate `greentic-events-providers/crates/provider-sms` (`handle_inbound_sms`, `build_send_request`, `TwilioSendRequest`) into the component, re-targeted to emit `ChannelMessageEnvelope` and implement `send_payload`. A new `packs/messaging-sms/pack.yaml` declares the capability + provider-extension + http-routes so a tenant can enable it.

**Tech Stack:** Rust (edition 2024), `wasm32-wasip2`, `wit-bindgen`, `greentic_types` (`ChannelMessageEnvelope`, `HttpInV1`/`HttpOutV1`), Twilio REST (form-encoded, HTTP Basic), HMAC-SHA1 signature.

## Global Constraints

- **Reference component (copy its idioms verbatim, rename `whatsapp`→`sms`):** `components/messaging-provider-whatsapp/` — its `Cargo.toml`, `wit/.../world.wit`, `src/lib.rs` (the `dispatch_json_invoke` op table), `src/ops/ingest.rs`, egress ops, and tests. Every "mirror WhatsApp" instruction means: read that file and reproduce its structure with only the SMS deltas this plan spells out.
- **Reuse (do not re-derive):** Twilio inbound parse + `TwilioSendRequest`/`build_send_request` from `greentic-events-providers/crates/provider-sms/src/lib.rs` — copy the logic into the component (the crate is not a WASM dep); its unit tests are the reference for the port.
- **Zero `greentic-start` change** — the component is discovered from its pack; the host already dispatches `ingest_http` + egress ops. Do NOT edit greentic-start/greentic-runner.
- **Inbound envelope type is `ChannelMessageEnvelope`** (messaging family, conversational + auto-reply), NOT `EventEnvelope`.
- **Secrets are tenant-scoped**, injected per-request by the host: `TWILIO_ACCOUNT_SID`, `TWILIO_AUTH_TOKEN`, `TWILIO_FROM_NUMBER`. Declared via `describe()` `secret_requirements` (`scope: "tenant"`); `component.manifest.json` is auto-generated from `describe()` (`greentic-component manifest`) — never hand-edit it.
- **Signature validation is in-scope v1** (fail closed → `403` on mismatch/missing token).
- **Text-only v1** — `attachments = []`; `NumMedia>0` media dropped with a metadata note. No MMS, no Messaging Service SID, no delivery callbacks (deferred).
- **No panics on malformed input** — parse failure → `HttpOutV1 { status: 400, events: [] }`.
- **Build target `wasm32-wasip2`** via `tools/build_components.sh` (+ per-package `tools/build_components/messaging-provider-sms.sh`), `SKIP_WASM_TOOLS_VALIDATION=1`. **The wasm build + `describe()` manifest regen are DEFERRED to a disk-free window** — author + unit-test host-side first; the final wasm build is the last step.
- **Conventional commits, NO Claude co-author.** Target branch `research`.
- **Build discipline (shared disk-constrained machine):** run cargo in the worktree only; FOREGROUND; never `pkill`/`kill` or delete another worktree's `target/`. Prefer host-target `cargo test -p messaging-provider-sms` for unit tests (fast, no wasm) where the crate is structured to allow it; the wasm build is the deferred final gate.

---

### Task 1: Scaffold component + `describe()`

**Files:**
- Create: `components/messaging-provider-sms/Cargo.toml`, `src/lib.rs`, `src/descriptor.rs` (or wherever WhatsApp puts `describe()`), `wit/messaging-provider-sms/world.wit` + `interfaces.wit`
- Modify: workspace `Cargo.toml` members (if the repo lists components there), `tools/build_components.sh` (add `messaging-provider-sms` to the package list)
- Test: `src/descriptor.rs` `#[cfg(test)]`

**Interfaces:**
- Produces: `describe()` returning CBOR metadata — `id = "messaging-provider-sms"`, `version`, `ops = [ingest_http, render_plan, encode, send_payload, setup_webhook, descriptor, qa, ...]` (match WhatsApp's op set minus WhatsApp-only ops), `secret_requirements = [TWILIO_ACCOUNT_SID, TWILIO_AUTH_TOKEN, TWILIO_FROM_NUMBER]` (`scope: tenant`), `provider_type = "messaging.sms.twilio"`.
- Produces: `dispatch_json_invoke(op, input)` skeleton (the op-string match table) — later tasks fill each arm.

- [ ] **Step 1: Copy + rename the WhatsApp skeleton.** Read `components/messaging-provider-whatsapp/{Cargo.toml,wit/**,src/lib.rs}`. Create the `messaging-provider-sms` equivalents: rename the package, the WIT world (`component-v0-v6-whatsapp` → `component-v0-v6-sms`), and the crate name. Keep the same exports (`descriptor`, `runtime`, `qa`, `component-i18n`, `schema-core-api`, `instance-identity-*`). Register the package in `tools/build_components.sh`.

- [ ] **Step 2: Write the failing `describe()` test.**
```rust
#[test]
fn describe_reports_sms_identity_and_secrets() {
    let d = describe_value(); // the crate's testable accessor for describe() metadata (mirror WhatsApp)
    assert_eq!(d.id, "messaging-provider-sms");
    assert_eq!(d.provider_type.as_deref(), Some("messaging.sms.twilio"));
    for op in ["ingest_http", "render_plan", "encode", "send_payload"] {
        assert!(d.ops.iter().any(|o| o == op), "op {op} present");
    }
    let secret_names: Vec<_> = d.secret_requirements.iter().map(|s| s.name.as_str()).collect();
    for s in ["TWILIO_ACCOUNT_SID", "TWILIO_AUTH_TOKEN", "TWILIO_FROM_NUMBER"] {
        assert!(secret_names.contains(&s), "secret {s} declared");
    }
}
```
(Adapt `describe_value()`/field names to WhatsApp's actual `describe()` shape.)

- [ ] **Step 3: Run — expect FAIL** (`cargo test -p messaging-provider-sms describe_reports_sms_identity`).

- [ ] **Step 4: Implement `describe()`** with the SMS identity, op list, `provider_type`, and the three tenant-scoped `secret_requirements`. Fill `dispatch_json_invoke` arms as `todo!()`/`not_implemented` stubs for the ops later tasks own.

- [ ] **Step 5: Run — expect PASS + commit.**
```bash
cargo fmt --all
git add components/messaging-provider-sms tools/build_components.sh Cargo.toml
git commit -m "feat(sms): scaffold messaging-provider-sms component + describe()"
```

---

### Task 2: Inbound `ingest_http` → `ChannelMessageEnvelope`

**Files:**
- Create: `components/messaging-provider-sms/src/ops/ingest.rs`
- Modify: `src/lib.rs` (`"ingest_http" => ingest_http(...)` arm)
- Test: `src/ops/ingest.rs` `#[cfg(test)]`

**Interfaces:**
- Consumes: `greentic_types::messaging::{HttpInV1, HttpOutV1, ChannelMessageEnvelope, Actor, Destination}` (read the exact fields in `greentic-types/src/messaging.rs` + `messaging/universal_dto.rs`).
- Produces: `fn ingest_http(input_json) -> HttpOutV1` — parses the Twilio urlencoded body into a `ChannelMessageEnvelope`.

- [ ] **Step 1: Write the failing inbound test.**
```rust
#[test]
fn parses_twilio_inbound_form_into_channel_message() {
    // Twilio inbound webhook body (application/x-www-form-urlencoded)
    let body = "From=%2B15551230001&To=%2B15559990000&Body=hello+agent&MessageSid=SM123&NumMedia=0&AccountSid=AC1";
    let out = ingest_http(&http_in_with_body(body)); // helper builds HttpInV1{ body_b64, headers, ... } mirroring WhatsApp tests
    assert_eq!(out.status, 200);
    assert_eq!(out.events.len(), 1);
    let env = &out.events[0];
    assert_eq!(env.channel, "sms");
    assert_eq!(env.text.as_deref(), Some("hello agent"));
    assert_eq!(env.correlation_id.as_deref(), Some("SM123"));
    // from = sender phone; to = the Twilio number
    assert!(env.from.as_ref().is_some_and(|a| a_phone(a) == "+15551230001"));
}

#[test]
fn malformed_body_returns_400_no_events() {
    let out = ingest_http(&http_in_with_body("%%%not-a-form"));
    assert_eq!(out.status, 400);
    assert!(out.events.is_empty());
}

#[test]
fn mms_drops_media_keeps_text_with_note() {
    let body = "From=%2B15551230001&To=%2B15559990000&Body=pic&MessageSid=SM9&NumMedia=1&MediaUrl0=https%3A%2F%2Fx";
    let out = ingest_http(&http_in_with_body(body));
    assert_eq!(out.events.len(), 1);
    assert!(out.events[0].attachments.is_empty(), "text-only v1");
    // metadata carries a dropped-media note (assert per the MessageMetadata shape)
}
```
(Use the real `greentic_types` field names — read them; adapt `a_phone`/`http_in_with_body` to the actual `Actor`/`HttpInV1` shapes as WhatsApp's tests do.)

- [ ] **Step 2: Run — expect FAIL.**

- [ ] **Step 3: Implement `ingest_http`.** Port `handle_inbound_sms` from `crates/provider-sms/src/lib.rs`, re-targeted: decode `HttpInV1.body_b64`, urlencoded-parse (`From`/`To`/`Body`/`MessageSid`/`NumMedia`), build `ChannelMessageEnvelope` per §4 of the spec (channel `"sms"`, `from`/`to` phones, `text`, `correlation_id = MessageSid`, `reply_scope` so the reply returns to the sender, `attachments = []`, tenant from injected `TenantCtx` — mirror how WhatsApp reads the injected tenant). Malformed → `400`. Signature check is Task 3 (leave a hook).

- [ ] **Step 4: Run — expect PASS + commit** (`feat(sms): inbound ingest_http parses Twilio SMS into ChannelMessageEnvelope`).

---

### Task 3: Twilio signature validation

**Files:**
- Create: `components/messaging-provider-sms/src/ops/signature.rs`
- Modify: `src/ops/ingest.rs` (call the validator first)
- Test: `src/ops/signature.rs` `#[cfg(test)]`

**Interfaces:**
- Produces: `fn valid_twilio_signature(auth_token: &str, url: &str, params: &BTreeMap<String,String>, header_sig: &str) -> bool` — Twilio scheme: `base64(HMAC_SHA1(auth_token, url + concat(sorted_key + value)))`, constant-time compare.

- [ ] **Step 1: Write the failing signature test** with a known vector.
```rust
#[test]
fn validates_known_twilio_signature() {
    // Twilio's documented algorithm: url + sorted(param k=v concatenated), HMAC-SHA1 with auth token, base64.
    let token = "12345";
    let url = "https://example.com/v1/messaging/ingress/sms/t1";
    let mut params = std::collections::BTreeMap::new();
    params.insert("Body".to_string(), "hi".to_string());
    params.insert("From".to_string(), "+15551230001".to_string());
    let expected = expected_sig(token, url, &params); // compute via a reference impl in the test
    assert!(valid_twilio_signature(token, url, &params, &expected));
    assert!(!valid_twilio_signature(token, url, &params, "tampered=="));
}
```

- [ ] **Step 2: Run — expect FAIL.**

- [ ] **Step 3: Implement `valid_twilio_signature`** (HMAC-SHA1 via the crate the repo already uses for hashing/hmac — check WhatsApp/other components' deps; add `hmac`+`sha1` only if none exists). Wire it into `ingest_http`: reconstruct the request URL from injected config + `To`/host, read `X-Twilio-Signature`, validate against `TWILIO_AUTH_TOKEN`; on mismatch or missing token → `HttpOutV1 { status: 403, events: [] }`.

- [ ] **Step 4: Add the 403 test to `ingest.rs`** (tampered/missing signature → 403, no events) and run both — expect PASS + commit (`feat(sms): validate X-Twilio-Signature, fail closed on mismatch`).

---

### Task 4: Outbound egress — `render_plan` / `encode` / `send_payload`

**Files:**
- Create: `components/messaging-provider-sms/src/ops/egress.rs`
- Modify: `src/lib.rs` (the three op arms)
- Test: `src/ops/egress.rs` `#[cfg(test)]`

**Interfaces:**
- Consumes: the host egress contract shapes (read `greentic-start/src/messaging_egress.rs` for what `render_plan`/`encode`/`send_payload` receive/return, and mirror WhatsApp's egress ops).
- Produces: `send_payload` builds + issues the Twilio REST send; on success `{ ok: true, sid }`, on failure a structured `{ ok: false, error, status }`.

- [ ] **Step 1: Write the failing outbound test** (build-shape, no network).
```rust
#[test]
fn builds_twilio_send_request_from_reply() {
    // Given an agent reply targeted at +15551230001 from TWILIO_FROM_NUMBER +15559990000
    let req = build_twilio_send("+15559990000", "+15551230001", "thanks!"); // port of build_send_request
    assert_eq!(req.to, "+15551230001");
    assert_eq!(req.from, "+15559990000");
    assert_eq!(req.body, "thanks!");
    // form-encoded + Basic-auth header shape asserted per TwilioSendRequest
}

#[test]
fn send_payload_maps_non_2xx_to_structured_error() {
    let out = send_payload_result(TwilioResponse::error(400, "21211", "invalid To"));
    assert!(!out.ok);
    assert_eq!(out.status, Some(400));
}
```

- [ ] **Step 2: Run — expect FAIL.**

- [ ] **Step 3: Implement the three ops.** `render_plan`: agent reply text → single-segment SMS plan (mirror WhatsApp). `encode`: plan → Twilio payload (`To`/`From`/`Body`). `send_payload`: port `build_send_request`/`TwilioSendRequest` — POST `https://api.twilio.com/2010-04-01/Accounts/{AccountSid}/Messages.json`, form body, HTTP Basic `AccountSid:AuthToken` (from injected secrets), `From = TWILIO_FROM_NUMBER`; non-2xx → structured error; success → `{ ok, sid }`. Use the host HTTP capability the WhatsApp component uses for outbound (do NOT add a raw sockets dep — mirror WhatsApp's send transport).

- [ ] **Step 4: Run — expect PASS + commit** (`feat(sms): outbound egress render_plan/encode/send_payload via Twilio REST`).

---

### Task 5: Pack + capability wiring

**Files:**
- Create: `packs/messaging-sms/pack.yaml`, `packs/messaging-sms/` QA setup asset(s) (mirror `packs/messaging-whatsapp/`)
- Test: a pack-load / manifest assertion if the repo has one (mirror any `packs/*` test); else a `pack.yaml` schema check via the repo's pack tooling.

**Interfaces:**
- Consumes: the component from Tasks 1-4 (`component_ref: messaging-provider-sms`).

- [ ] **Step 1: Copy the WhatsApp pack.** Read `packs/messaging-whatsapp/pack.yaml`. Create `packs/messaging-sms/pack.yaml` with the extension trio:
  - `greentic.ext.capabilities.v1` — `cap_id: greentic.cap.messaging.provider.v1`, `offer_id: messaging-sms-v1`, `provider.component_ref: messaging-provider-sms`, `op: messaging.configure`, `requires_setup: true`, `setup.qa_ref`.
  - `greentic.provider-extension.v1` — `provider_type: messaging.sms.twilio`, `ops: [ingest_http, render_plan, encode, send_payload, setup_webhook]`, `config_schema_ref`, `setup_contract` (answers → `config_out` from-number + `secrets_out` `TWILIO_ACCOUNT_SID`/`TWILIO_AUTH_TOKEN`/`TWILIO_FROM_NUMBER`).
  - `greentic.http-routes.v1` — default `ingest_http`, domain `messaging`.
- [ ] **Step 2: Wire the QA setup** (mirror WhatsApp's `setup.qa_ref` asset): prompts for account SID, auth token, from-number → mapped by `setup_contract` to the three secrets + the from-number config.
- [ ] **Step 3: Validate** the pack with the repo's pack tooling (mirror how WhatsApp's pack is validated in CI/tests); commit (`feat(sms): messaging-sms pack + Twilio QA setup contract`).

---

### Task 6 (DEFERRED to disk-free window): wasm build + manifest regen + full gate

**Files:**
- Create: `tools/build_components/messaging-provider-sms.sh` (copy WhatsApp's)
- Modify: `components/messaging-provider-sms/component.manifest.json` (auto-generated, do not hand-edit)

- [ ] **Step 1: Build the component** for `wasm32-wasip2`: `SKIP_WASM_TOOLS_VALIDATION=1 ./tools/build_components/messaging-provider-sms.sh` (or via `tools/build_components.sh`). Fix any wasm-only issues (host-fn imports, wit-bindgen).
- [ ] **Step 2: Regenerate the manifest** from `describe()`: `greentic-component manifest` (per the repo's remove-manifest-json single-source-of-truth rule) so `component.manifest.json` matches `describe()` (incl. `secret_requirements`).
- [ ] **Step 3: Full repo gate** — `cargo fmt --all --check`, `cargo clippy --workspace -- -D warnings`, `cargo test --workspace`, and `./ci/local_check.sh` (per repo CLAUDE.md). Commit (`chore(sms): build messaging-provider-sms wasm + regenerate manifest`).
- [ ] **Step 4:** finishing-a-development-branch → push + PR to `research`.

---

## Self-Review

- **Spec coverage:** §2 messaging-path rationale → Task 2 (envelope type) + Global Constraints; §3 architecture/reuse → Tasks 1-4; §4 inbound → Task 2; §4.1 signature → Task 3; §5 egress → Task 4; §6 pack/config → Task 5; §7 error handling → Tasks 2 (400), 3 (403), 4 (structured error); §9 testing → per-task tests; §10/deferred build → Task 6.
- **Placeholder scan:** the "mirror WhatsApp" / "adapt to real `greentic_types` shape" instructions are deliberate — the exact struct fields + `describe()`/egress idioms must be read from the named reference files (they cannot be invented correctly here). Every task names its exact reference file + the specific SMS delta. No TBD/TODO left as work-defining.
- **Type consistency:** `ingest_http` → `HttpOutV1{status,events}` consistent Task 2 ↔ 3; `ChannelMessageEnvelope` fields (channel/from/to/text/correlation_id/attachments) consistent §4 ↔ Task 2; secret names `TWILIO_ACCOUNT_SID`/`TWILIO_AUTH_TOKEN`/`TWILIO_FROM_NUMBER` consistent Task 1 (describe) ↔ Task 4 (send) ↔ Task 5 (pack); `provider_type = "messaging.sms.twilio"` consistent Task 1 ↔ Task 5; `TwilioSendRequest` fields (to/from/body) consistent Task 4.
- **Scope:** single component + pack, one plan; build isolated as the deferred final task.
