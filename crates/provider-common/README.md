# Greentic Messaging Provider Common

Shared provider DTOs, helpers, errors, QA bridges, schema helpers, and compatibility utilities for Greentic messaging provider components.

The crates.io package name is `greentic-messaging-provider-common`. The Rust crate name is `provider_common`, preserving the existing import style:

```rust
use provider_common::ProviderError;
use provider_common::helpers::{json_bytes, schema_core_healthcheck};
```

## What Belongs Here

- Provider-safe error types and constructors.
- Component v0.6 DTOs for describe payloads, schema IR, QA specs, i18n text, and canonical CBOR helpers.
- Provider-core helpers for common healthcheck, validation, JSON response, render/encode/send, and schema construction.
- QA bridge helpers for `qa-spec`, `apply-answers`, and i18n dispatch through provider-core `invoke`.
- HTTP compatibility helpers for operator payload shape differences.
- Lifecycle state/config/provenance key helpers.
- Shared test macros and metadata tests used by provider crates.

Provider-specific request payloads, external API quirks, credentials, config structs, and delivery semantics should stay in the provider crate.

## Example

```rust
use provider_common::ProviderError;
use provider_common::helpers::json_bytes;
use serde_json::json;

fn missing_token_response() -> Vec<u8> {
    let err = ProviderError::missing_secret("SLACK_BOT_TOKEN");
    json_bytes(&json!({
        "ok": false,
        "error": err.to_string(),
    }))
}
```

## Publishing Policy

This crate has its own semver version and release notes. Provider crates should consume a released crates.io version once the provider independence work lands. During the transition, workspace path dependencies may remain, but the public API should be treated as the compatibility boundary for provider code.

Before publishing:

```bash
cargo fmt --check
cargo test -p greentic-messaging-provider-common
cargo publish --dry-run -p greentic-messaging-provider-common
```

Repository releases use `.github/workflows/publish-shared-crate.yml`; see
`docs/shared-crate-release.md` for the full release flow and required
`CARGO_REGISTRY_TOKEN` secret.

## Feature Flags

- `schema`: enables `schemars::JsonSchema` derives for public DTOs.

## Migration Notes

See `MIGRATION.md` for provider code that should be replaced by this shared API in follow-up provider migration PRs.
