# Shared Provider Crate Release

The shared provider crate is published as `greentic-messaging-provider-common`
from `crates/provider-common`. Its Rust crate name remains `provider_common`.

## Required Secret

Configure this repository secret:

- `CARGO_REGISTRY_TOKEN`: crates.io API token with permission to publish
  `greentic-messaging-provider-common`.

Do not store the token in answer files, workflow inputs, or committed config.

## Release Flow

1. Update `crates/provider-common/Cargo.toml` with the intended independent
   semver version.
2. Update `crates/provider-common/CHANGELOG.md`.
3. Open a PR and let normal validation run.
4. After merge to `main`, `.github/workflows/publish-shared-crate.yml` validates:
   - `cargo fmt --check -p greentic-messaging-provider-common`
   - `cargo clippy -p greentic-messaging-provider-common --all-targets -- -D warnings`
   - `cargo test -p greentic-messaging-provider-common`
   - `cargo publish --dry-run -p greentic-messaging-provider-common`
5. If the version is not already on crates.io, the workflow publishes it.

Manual releases are available from `Publish Shared Provider Crate`. Set
`publish=true` to publish after validation. Leave `publish=false` for a dry-run
validation that still uploads release metadata.

## Idempotency

The workflow checks crates.io for the exact crate/version before publishing.
If the version already exists, the run exits successfully with publish status
`already-published` and does not call `cargo publish`.

## Downstream Handoff

The workflow exposes these `workflow_call` outputs and uploads the same data as
the `shared-crate-release-metadata` artifact:

- `crate_name`
- `version`
- `git_sha`
- `publish_status`

Provider rebuild orchestration should consume this metadata before rebuilding
providers against a newly released shared crate.
