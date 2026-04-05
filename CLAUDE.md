# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project Overview

Rust workspace producing WASM-based messaging provider components (WASI Preview 2) for the Greentic platform. Each provider integrates with an external messaging service (Slack, Teams, Telegram, Webex, WhatsApp, WebChat, Email) through self-contained WebAssembly components.

- **Edition:** Rust 2024 | **Target:** `wasm32-wasip2` (components) + native (crates)
- **Workspace version** is defined once in root `Cargo.toml` `[workspace.package]`

## Build, Test, and Lint Commands

```bash
# Full local CI (all steps sequentially — run before any PR)
./ci/local_check.sh

# Individual CI steps (use to iterate on a single failure)
./ci/steps/01_fmt.sh
./ci/steps/02_clippy.sh
# ... see ci/steps/ for all numbered scripts

# Build all WASM components → target/components/*.wasm
./tools/build_components.sh

# Run all tests (requires components built first)
cargo test --workspace

# Run a single crate's tests
cargo test -p messaging-provider-webex --lib

# Run a single integration test file
cargo test -p provider-tests --test registry_fixtures

# Format and lint
cargo fmt
cargo clippy --workspace --all-targets

# Regenerate registry test fixtures after schema changes
./tools/regenerate_registry_fixtures.sh

# Build .gtpack bundles (dry-run)
./tools/build_packs.sh
```

## Architecture

### Directory Layout

- `components/` — WASM provider components (each compiles to `cdylib` targeting `wasm32-wasip2`)
- `crates/` — Shared Rust libraries (native target)
- `packs/` — Packaged provider bundles (`.gtpack` archives containing WASMs, flows, schemas)
- `wit/` — WebAssembly Interface Type definitions
- `tests/` — Integration tests (host-side WASM provider tests)
- `ci/` — CI scripts; `ci/local_check.sh` orchestrates all steps, `ci/steps/` has individual scripts

### WIT Component Model

All providers export the `component-v0-v6-v0` world (`greentic:component@0.6.1`):
- **imports:** `http-client`, `secrets-store`
- **exports:** `descriptor`, `runtime` (CBOR), `qa`, `component-i18n`, `schema-core-api` (JSON compat)

### Egress Pipeline (3-step WASM invocation)

1. **render_plan** — Determines Adaptive Card tier, extracts text summary for TierD
2. **encode** — Serializes to `ChannelMessageEnvelope` in provider-specific format
3. **send_payload** — Fetches secrets, calls external API, returns delivery confirmation

### Adaptive Card Tiers

| Tier | Rendering | Providers |
|------|-----------|-----------|
| TierA | Native AC | Teams, WebChat |
| TierB | AC as attachment + fallback | Webex |
| TierD | Downsampled to plain text | Slack, Telegram, WhatsApp, Email |

### Provider Component Structure

Each `components/messaging-provider-*` follows this pattern:
- `src/lib.rs` — Component struct, trait impls, config types (`ProviderConfig`, `ProviderConfigOut`)
- `src/describe.rs` — Metadata, QA specs, i18n keys/pairs, config schema (`I18N_KEYS`, `I18N_PAIRS`, `SETUP_QUESTIONS`, `config_schema()`)
- `src/ops.rs` — Operations: send, ingest_http, render_plan, encode, send_payload
- `src/config.rs` — Config parsing, validation, secret loading
- `component.manifest.json` — Auto-generated from WASM `describe()` export during build (gitignored, not hand-maintained)
- `Cargo.toml` — `[lib] crate-type = ["cdylib"]`

### Key Crates

- **provider-common** — `ProviderError`, `ProviderCapabilitiesV1`, `RenderTier`, schema helpers, QA helpers, test macros (`standard_provider_tests!`)
- **greentic-messaging-renderer** — Render planning, card downsampling, `plan_render()`
- **messaging-cardkit** — Offline Adaptive Card rendering per platform
- **provider-tests** — WASM test harness, fixture validation, conformance tests
- **greentic-messaging-tester** — CLI for testing providers (`send`, `ingress`, `requirements`)

## Version Management

The workspace version in root `Cargo.toml` must stay in sync with:
- All `pack.yaml` files (12 files in `packs/`)
- All `pack.manifest.json` files (12 files in `packs/`)

Note: `component.manifest.json` files are now auto-generated from WASM `describe()` during build and gitignored. No manual sync needed.

Run `./tools/sync_packs.sh` to synchronize versions from `Cargo.toml` to all manifests and pack files.

## Adding/Modifying Provider Schemas

When adding a new config field to a provider:
1. Add the field to `ProviderConfig` and `ProviderConfigOut` structs in `lib.rs` or `config.rs`
2. Add the field to `config_schema()` in `describe.rs` (required for schema validation)
3. Add i18n keys to `I18N_KEYS` and i18n pairs to `I18N_PAIRS` in `describe.rs`
4. Add the field to the allowed keys list in `config.rs` `load_config()`
5. If the field is `Option<String>`, use `#[serde(default, skip_serializing_if = "Option::is_none")]` on `ProviderConfigOut` to avoid null serialization issues with schema validation
6. Update the schema hash in `standard_provider_tests!` if present
7. Regenerate fixtures: `./tools/regenerate_registry_fixtures.sh`

## WebChat GUI Assets & Embed

The `messaging-webchat-gui` pack includes:

### Skin System
- Skins live in `packs/messaging-webchat-gui/assets/webchat-gui/skins/`
- `default/` and `demo/` skins are self-contained (point to own files, not `_template`)
- Each skin has: `skin.json`, `fullpage/index.html`, `fullpage/page.css`, `webchat/styleOptions.json`, `webchat/hostconfig.json`, `webchat/hooks.js`, `assets/` (logo, favicon, hero)
- Skin `default/` serves as template for bundle scaffold — copied and renamed to tenant name

### Tenant Config
- `config/tenants/default.json` — OAuth providers (Guest, Microsoft, Google) + i18n branding
- OAuth providers are `enabled: false` by default — user enables via capability or manual config
- Tenant config is scaffolded per-tenant when bundle_assets capability is enabled

### embed.js
- `assets/webchat-gui/embed.js` — chat bubble widget script
- Auto-loads defaults (color, title, logo) from tenant's `skin.json`
- Config via `window.greenticChatConfig` — tenant, baseUrl, bubble, window options
- Public API: `greenticChat.open()`, `.close()`, `.toggle()`, `.isOpen()`
- Mobile responsive — fullscreen on <480px viewport

## Greentic Reuse-First Policy

Before adding new core types or interfaces, check if they exist in shared Greentic crates: `greentic-interfaces`, `greentic-types`, `greentic-secrets`, `greentic-oauth`, `greentic-messaging`, `greentic-events`. Only introduce new shared concepts when no existing crate fits.

## CI/CD

- Rust toolchain: **1.94.0**
- GitHub Actions: `.github/workflows/build-and-publish.yml` (fmt, clippy, schema check, build+test, packs)
- Required env vars for OCI publishing: `GHCR_USERNAME`, `GHCR_TOKEN` (see `.env.example`)

## Git Conventions

Do NOT add Claude co-author attribution to commits or PRs.
