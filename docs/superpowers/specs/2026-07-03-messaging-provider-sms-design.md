# EPIC-E1 v1 — `messaging-provider-sms` (Twilio, conversational) — Design Spec

**Status:** Draft for review — 2026-07-03
**Initiative:** Agentic platform coverage (PRD `greentic-designer:docs/superpowers/specs/2026-07-02-agentic-platform-coverage-prd.md`), EPIC-E1 "Channels: SMS + inbound Email".
**Scope of THIS slice:** conversational **SMS** only. Email is deferred (see §8).

## 1. Problem & goal

The PRD calls for "agents receive SMS + inbound email, not just send." A read-only audit of the ingress plumbing found:

- **Conversational email already works** via `greentic-messaging-providers/components/messaging-provider-email` (MS Graph inbound `ingest_http` + full egress send). So email is not the real gap for v1.
- **SMS is the real gap.** `greentic-events-providers/components/events-provider-sms-twilio` emits an `EventEnvelope` on inbound (no conversational reply) and its `send_sms` op is a **stub** (`{"ok":false,"status":"not_enabled"}`); its pack points at `components/stubs/*.wasm`. The real, reusable Twilio logic lives in the native-only crate `greentic-events-providers/crates/provider-sms` (`handle_inbound_sms`, `build_send_request`/`TwilioSendRequest`) but is not wired into a runnable messaging component. **There is no `messaging-provider-sms`.**

**Goal:** a new WASM messaging-provider component `messaging-provider-sms` (Twilio) so a tenant's agent can **receive an inbound SMS, run its flow, and reply by SMS** — routed through the existing HTTP front door with **zero `greentic-start` change**.

## 2. Why the messaging-provider path (not events-provider)

Greentic has one HTTP front door (`greentic-start`) and two provider families that both implement the `ingest_http` op but differ in routing:

| | messaging-provider | events-provider |
|---|---|---|
| Inbound emits | `ChannelMessageEnvelope` (in `HttpOutV1.events`) | `EventEnvelope` (`emitted_events`) |
| Host routing | `route_messaging_envelopes` → **runs the tenant app flow, auto-replies via egress** | `event_router::route_events` → topic routing, **no reply** |
| Fit | a conversation (agent chats back) | a fire-and-forget event/trigger |

The deliverable ("agents *receive* SMS") is conversational, so we use the **messaging-provider path** and get the reply/egress pipeline for free. (`greentic-start/src/http_ingress/mod.rs` routes `domain == Messaging` → `http_ingress/messaging.rs::route_messaging_envelopes`.)

## 3. Architecture

New component `greentic-messaging-providers/components/messaging-provider-sms/`, modeled on `messaging-provider-whatsapp` (closest analog: single sender id, phone `Destination`, GET-verify-optional + POST-ingest). Target `wasm32-wasip2`, exports the `component-v0-v6-*` world (`greentic:component@0.6.1`): `descriptor`, `runtime`, `qa`, `component-i18n`, `schema-core-api`, `instance-identity-*` (copy the WhatsApp world verbatim, renamed).

**Logic reuse.** Port the inbound parse + outbound build from `greentic-events-providers/crates/provider-sms` (`handle_inbound_sms`, `build_send_request`, `TwilioSendRequest`) into the component's `ops/`, re-targeted to (a) emit `ChannelMessageEnvelope` on inbound and (b) implement the egress `send_payload` op on outbound. We copy the logic into the component (the native crate is not a WASM dependency); the crate's existing unit tests are the reference for the port.

**Zero host change.** The component is discovered from its pack; `greentic-start` already dispatches `/v1/messaging/ingress/{provider}/{tenant}` → `ingest_http` and the egress `render_plan`/`encode`/`send_payload` ops when the pack declares them.

### 3.1 Ops (dispatched via `runtime.invoke(op, input-cbor)`)

Mirror `messaging-provider-whatsapp`'s `dispatch_json_invoke` table:

- **`ingest_http`** (inbound) — see §4.
- **`render_plan`** → **`encode`** → **`send_payload`** (outbound egress) — see §5.
- **`setup_webhook`** (optional, best-effort) — register the tenant's inbound webhook URL with Twilio; may be a no-op returning `not_supported` in v1 (operators can set the webhook in the Twilio console). YAGNI: implement as a documented no-op unless the WhatsApp analog makes it trivial.
- Standard: `descriptor`, `qa`, i18n, schema — copy WhatsApp.

## 4. Inbound: `ingest_http`

Twilio posts inbound SMS as `application/x-www-form-urlencoded` with fields `From`, `To`, `Body`, `MessageSid`, `NumMedia`, `AccountSid`.

1. **Signature validation (security, in-scope v1).** Twilio signs each request with `X-Twilio-Signature` = base64(HMAC-SHA1(auth_token, url + sorted-post-params)). Reconstruct and compare (constant-time). On mismatch → `HttpOutV1 { status: 403, events: [] }`. The `TWILIO_AUTH_TOKEN` secret is injected per-request by the host (`build_injected_config`, tenant-scoped). If the public URL differs from what Twilio signed (proxy), the exact URL used for signing is documented as a config note; v1 uses the `To`/host from the injected config.
2. **Parse** the form body (decode `HttpInV1.body_b64`, urlencoded parse) → extract `From` (E.164 phone), `To`, `Body`, `MessageSid`.
3. **Build `ChannelMessageEnvelope`** (`greentic_types::messaging::ChannelMessageEnvelope`): `channel = "sms"`, `from = Actor` (phone id), `to = [Destination(phone)]`, `text = Some(Body)`, `reply_scope` derived from `From`+`To` (so the reply goes back to the sender), `correlation_id = MessageSid`, `session_id` derived per the WhatsApp convention, `attachments = []` (text-only v1; `NumMedia>0` media dropped with a metadata note), tenant from the injected `TenantCtx`.
4. **Return** `HttpOutV1 { status: 200, body_b64: <empty TwiML or empty 200>, events: vec![envelope] }`. Twilio accepts an empty 200 (no auto-TwiML reply — the agent's reply goes out via the egress send path, not the webhook response).

## 5. Outbound: egress pipeline

The host egress (`greentic-start/src/messaging_egress.rs`) calls `render_plan` → `encode` → `send_payload`. Port the WhatsApp shapes:

- **`render_plan`** — turn the agent's reply (text) into the SMS render plan (single text segment; long messages left to Twilio segmentation).
- **`encode`** — encode the plan into the Twilio send payload (`To`, `From`, `Body`).
- **`send_payload`** — POST `https://api.twilio.com/2010-04-01/Accounts/{AccountSid}/Messages.json` (form-encoded `To`/`From`/`Body`), HTTP Basic auth `AccountSid:AuthToken`. Reuse `TwilioSendRequest`/`build_send_request` from the native crate. On non-2xx → return a structured error (surfaced by the host egress); on success → `{ ok: true, sid }`. `From` is `TWILIO_FROM_NUMBER`.

## 6. Pack & config seam (`packs/messaging-sms/pack.yaml`)

Copy the Telegram/WhatsApp extension trio:

- **`greentic.ext.capabilities.v1`** — `cap_id: greentic.cap.messaging.provider.v1`, `offer_id: messaging-sms-v1`, `provider.component_ref: messaging-provider-sms`, `op: messaging.configure`, `requires_setup: true`, `setup.qa_ref`.
- **`greentic.provider-extension.v1`** — `provider_type: messaging.sms.twilio`, `ops: [ingest_http, render_plan, encode, send_payload, setup_webhook]`, `config_schema_ref`, `setup_contract` mapping QA answers → `config_out` (from number) + `secrets_out` (token/sid).
- **`greentic.http-routes.v1`** — default `ingest_http`, domain `messaging` (so `/v1/messaging/ingress/sms/<tenant>` is served).
- **`secret_requirements`** (in `component.manifest.json`, auto-generated from `describe()`, `scope: "tenant"`): `TWILIO_ACCOUNT_SID`, `TWILIO_AUTH_TOKEN`, `TWILIO_FROM_NUMBER`.

Enabling for a tenant = drop the pack in the bundle + run the capability's QA setup (writes tenant-scoped config + secrets). No host code change.

## 7. Error handling

- Bad/again signature → `403`, no events.
- Missing injected secret (`TWILIO_AUTH_TOKEN`/`ACCOUNT_SID`) → `ingest_http` skips signature check only if configured to (default: fail closed → `403`); `send_payload` returns a structured `error` (host surfaces it).
- Twilio REST non-2xx on send → structured error with status + Twilio code; the host egress logs/surfaces it (no panic).
- `NumMedia > 0` (MMS) → text extracted if present, media dropped with a `metadata` note (text-only v1).
- GET on the ingress route (some setups probe) → `200 ok`.
- The component never panics on malformed input — parse failures return `400` with empty events.

## 8. Scope boundaries (YAGNI)

**In v1:** inbound SMS → envelope → flow → reply SMS (Twilio), signature validation, tenant-scoped secrets, From-number send, unit tests.

**Deferred (follow-up slices):**
- **Email** — conversational email already works via `messaging-provider-email` (MS Graph). Gmail/IMAP inbound to close provider parity is a separate slice (E1-b).
- MMS/attachments (in + out), Messaging Service SID (vs single From number), delivery-status callbacks, `setup_webhook` auto-registration (v1 = console/no-op), outbound segmentation control, short-code/10DLC registration guidance.

## 9. Testing

Component unit tests (mirror `messaging-provider-whatsapp` tests + reuse `crates/provider-sms` test cases):
- Inbound: urlencoded Twilio body → `ChannelMessageEnvelope` (from/to/text/correlation_id correct); `NumMedia>0` drops media with note; malformed body → 400.
- Signature: a known auth-token + url + params → expected `X-Twilio-Signature`; valid passes, tampered fails (403).
- Outbound: reply text → `TwilioSendRequest` (To/From/Body + Basic-auth header shape); non-2xx → structured error.
- `describe()` returns the expected id/version/ops/`secret_requirements`.

Build: `wasm32-wasip2` via the repo's `tools/build_components.sh` (add `messaging-provider-sms` to the package list) + a `tools/build_components/messaging-provider-sms.sh` (copy WhatsApp's). `SKIP_WASM_TOOLS_VALIDATION=1` per the repo convention. **This build is deferred to a disk-free window** (per the active disk-contention constraint); the spec + plan are authored build-free.

## 10. Rollout

- Single component + pack; additive; no host change; other channels untouched.
- Target branch `research` (coverage EPICs land on research).
- Follow-up: designer/admin surfacing of the SMS channel capability (already generic — any `greentic.cap.messaging.provider.v1` pack is selectable), and the email parity slice.
