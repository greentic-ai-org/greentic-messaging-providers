# Provider Migration Notes

This crate is the shared boundary for reusable messaging provider code. Provider crates should migrate to it incrementally, keeping provider-specific behavior local.

## Package Name

Use the crates.io package name once published:

```toml
provider-common = { package = "greentic-messaging-provider-common", version = "x.y.z" }
```

Existing Rust imports remain:

```rust
use provider_common::ProviderError;
```

## Migrate To Shared APIs

Replace duplicated provider code with shared APIs in follow-up PRs:

- `ProviderError` and missing-secret responses should use `provider_common::ProviderError`.
- Provider-core `validate_config` and `healthcheck` boilerplate should use `provider_common::helpers::schema_core_validate_config` and `schema_core_healthcheck` where the provider accepts the common behavior.
- JSON response serialization should use `provider_common::helpers::json_bytes`.
- Common `qa-spec`, `apply-answers`, and i18n dispatch through provider-core `invoke` should use `provider_common::qa_invoke_bridge`.
- Component v0.6 describe payloads, schema IR, QA specs, skip expressions, and canonical CBOR helpers should use `provider_common::component_v0_6`.
- Operator HTTP payload compatibility should use `provider_common::http_compat`.
- Config/provenance/state key construction should use `provider_common::lifecycle_keys`.
- Adaptive Card conversion and render plan helpers should use `provider_common::ac_converter` and `provider_common::helpers` when the provider behavior matches the shared renderer contract.
- Generic `apply_answers` success/remove/error result shape should use `provider_common::qa_helpers` when provider config types fit the generic result.

## Keep Provider-Local

Do not move these into the shared crate unless multiple providers genuinely need the same stable API:

- External API request/response structs.
- Provider-specific config fields and validation rules.
- OAuth, webhook, or direct-message details unique to one service.
- Secrets names that are provider-specific.
- Tests that assert provider API behavior.

## Transition Rule

During the workspace transition, path dependencies are acceptable. Once this crate is published, provider crates should depend on the crates.io semver version and only keep workspace-only common code with an explicit justification.
