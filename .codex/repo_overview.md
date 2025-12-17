# Repository Overview

## 1. High-Level Purpose
- Rust workspace for WASM messaging provider components plus small shared crates for message types and provider utilities.
- Targets Rust 2024 edition; components bind to host-provided HTTP, secrets, state, and telemetry interfaces via WIT.

## 2. Main Components and Functionality
- **Path:** `crates/messaging-core`
  - **Role:** Shared message model crate.
  - **Key functionality:** Defines `Message` struct with `id` and `content` fields, serde serialize/deserialize derives, and a convenience constructor.
  - **Key dependencies / integration points:** Uses `serde` for data interchange.
- **Path:** `crates/provider-common`
  - **Role:** Common provider utilities.
  - **Key functionality:** Defines `ProviderError` enum (`Validation`, `Transport`, `Other`) with `thiserror` display strings, serde serialization, and helper constructors.
  - **Key dependencies / integration points:** Relies on `serde` and `thiserror`; intended for reuse across provider components.
- **Path:** `components/secrets-probe`
  - **Role:** Minimal WASM component that probes the `greentic:secrets-store@1.0.0` interface.
  - **Key functionality:** Exports `run()` which calls `secrets_store::get("TEST_API_KEY")` and returns JSON `{"ok":true,"key_present":true}` when the secret is present; returns `{"ok":false,"key_present":false}` on missing/failed lookups.
  - **Key dependencies / integration points:** Uses `wit-bindgen` 0.26 with WIT definitions under `components/secrets-probe/wit/`; imports canonical `greentic:secrets-store` package via local WIT files (now resolved by `cargo component`).
- **Path:** `components/teams`
  - **Role:** Microsoft Teams provider component with egress, ingress, refresh stub, and formatting.
  - **Key functionality:** Exports WIT world with `send_message` (POST to Graph channel messages using destination JSON), `handle_webhook` (wraps incoming payload), `refresh` (no-op JSON), and `format_message` (returns Graph message payload JSON). Includes basic unit tests for destination parsing and formatting.
  - **Key dependencies / integration points:** Uses canonical Greentic WIT packages for HTTP, secrets, state, telemetry, and interfaces-types; expects `MS_GRAPH_ACCESS_TOKEN` secret for bearer auth.
- **Path:** `components/webchat`
  - **Role:** WebChat provider component with egress, ingress, refresh stub, and formatting.
  - **Key functionality:** Exports WIT world with `send_message` (posts formatted payload to a configurable endpoint placeholder), `handle_webhook` (wraps incoming payload), `refresh` (no-op JSON), and `format_message` (returns webchat message payload JSON). Includes unit tests for payload formatting and webhook normalization.
  - **Key dependencies / integration points:** Uses canonical Greentic WIT packages for HTTP, secrets, state, telemetry, and interfaces-types; optionally uses `WEBCHAT_BEARER_TOKEN` if provided.
- **Path:** `components/webex`
  - **Role:** Webex provider component with egress, ingress, refresh stub, and formatting.
  - **Key functionality:** Exports WIT world with `send_message` (POST to Webex messages API), `handle_webhook` (wraps incoming payload), `refresh` (no-op JSON), and `format_message` (returns message payload JSON). Includes unit tests for payload formatting and webhook normalization.
  - **Key dependencies / integration points:** Uses canonical Greentic WIT packages for HTTP, secrets, state, telemetry, and interfaces-types; expects `WEBEX_BOT_TOKEN` for bearer auth.
- **Path:** `components/whatsapp`
  - **Role:** WhatsApp provider component with egress, ingress, refresh stub, and formatting.
  - **Key functionality:** Exports WIT world with `send_message` (Graph WhatsApp messages), `handle_webhook` (wraps payload; optional verify-token check), `refresh` (no-op JSON), and `format_message` (WhatsApp text payload JSON). Includes unit tests for destination parsing, formatting, and webhook normalization.
  - **Key dependencies / integration points:** Uses canonical Greentic WIT packages for HTTP, secrets, state, telemetry, and interfaces-types; expects `WHATSAPP_TOKEN`, `WHATSAPP_PHONE_NUMBER_ID`, and optional `WHATSAPP_VERIFY_TOKEN`.
- **Path:** `components/telegram`
  - **Role:** Telegram provider component with egress, ingress, refresh stub, and formatting.
  - **Key functionality:** Exports WIT world with `send_message` (Telegram `sendMessage` via imported HTTP client), `handle_webhook` (wraps incoming update payload), `refresh` (no-op JSON), and `format_message` (returns Telegram send payload JSON). Includes unit tests for payload formatting and webhook normalization.
  - **Key dependencies / integration points:** Uses canonical Greentic WIT packages for HTTP, secrets, state, telemetry, and interfaces-types; expects `TELEGRAM_BOT_TOKEN` for bearer calls.
- **Path:** `components/slack`
  - **Role:** Slack provider component with egress, ingress, refresh stub, and formatting.
  - **Key functionality:** Exports WIT world with `send_message` (POST to Slack `chat.postMessage` via imported HTTP client), `handle_webhook` (optional signature verification using HMAC-SHA256 and signing secret), `refresh` (no-op JSON), and `format_message` (returns chat.postMessage payload JSON). Includes unit tests for formatting and signature verification.
  - **Key dependencies / integration points:** Uses canonical Greentic WIT packages for HTTP, secrets, state, telemetry, and interfaces-types (all co-located under `components/slack/wit/slack/deps`); secrets fetched via `greentic:secrets-store`, HTTP via `greentic:http/http-client`.
- **Path:** `tools/build_components.sh`
  - **Role:** Builds components to `target/components/*.wasm`.
  - **Key functionality:** Runs `cargo component build --target wasm32-wasip2` for each component using an explicit target-dir, falling back to `cargo build` only if necessary; currently `cargo component` succeeds for both components and copies WASM artifacts into `target/components/`, then cleans nested target directories.
- **Path:** `ci/local_check.sh`
  - **Role:** CI convenience wrapper.
  - **Key functionality:** Runs `cargo fmt --check`, `cargo test --workspace`, and `tools/build_components.sh`.
- **Path:** `.github/workflows/build.yml`
  - **Role:** CI workflow for pushes/PRs.
  - **Key functionality:** Checks out the repo, installs Rust with `wasm32-wasip2`, installs `cargo-component`, runs fmt/test, builds components, and uploads `target/components/*.wasm` as artifacts.
- **Path:** `.github/workflows/publish.yml`
  - **Role:** Release publishing workflow.
  - **Key functionality:** On tags (`v*`), installs toolchain + `cargo-component` + `oras`, runs fmt/test/build, logs into GHCR, publishes component WASM artifacts via `tools/publish_oci.sh`, and uploads `components.lock.json`.
- **Path:** `tools/publish_oci.sh`
  - **Role:** Publish built WASM components to OCI and emit a lockfile.
  - **Key functionality:** Requires `OCI_REGISTRY`, `OCI_NAMESPACE`, and `VERSION`; pushes `target/components/*.wasm` with `oras` and writes `components.lock.json` recording references and digests.
- **Path:** `tests/provider_conformance.rs`
  - **Role:** Conformance checks across components.
  - **Key functionality:** Verifies each component has a manifest with `secret_requirements`, exports expected WIT functions, and does not use environment variables (ensures secrets come from the secrets-store).
- **Path:** `.github/workflows/build.yml`
  - **Role:** CI workflow for pushes/PRs.
  - **Key functionality:** Checks out the repo, installs Rust with `wasm32-wasip2`, installs `cargo-component`, runs fmt/test, builds components, and uploads `target/components/*.wasm` as artifacts.

## 3. Work In Progress, TODOs, and Stubs
- Six components exist; remaining planned providers (if any) not yet added.
- `wit_bindgen` macros carry `unsafe_op_in_unsafe_fn` allowances to silence Rust 2024 compatibility warnings; revisit once upstream generates safe wrappers.
- Build artifacts target `wasm32-wasip2`; the build script now removes nested component target directories after copying artifacts.

## 4. Broken, Failing, or Conflicting Areas
- No failing tests or build errors; `ci/local_check.sh` passes and produces WASM artifacts for all components (cargo-component emits nested `wasm32-wasip1` dirs but artifacts are copied).

## 5. Notes for Future Work
- Extend provider implementations (Teams, Telegram, Webchat, Webex, WhatsApp) per planned PRs using shared Greentic interfaces where available.
- Consider revisiting the `wit_bindgen` unsafe lint allowances if lint levels are tightened.
- Align on a single WASM target (wasip1 vs wasip2) if host expectations require consistency.
