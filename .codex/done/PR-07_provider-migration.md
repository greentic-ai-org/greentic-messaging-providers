# PR-07: Provider Migration To Published Shared Crate

## Purpose

Migrate providers to consume the published shared provider crate from crates.io.

## Current Audit Notes

- Provider crates currently depend on `provider-common` by workspace or path dependency.
- Several older provider components also use `provider-runtime-config` by path.
- Provider-core components use `greentic-types::messaging::universal_dto` plus `provider_common::helpers`.
- There is duplicated provider-local config validation, invoke dispatch, manifest/config response shaping, secret lookup behavior, and provider-specific send/encode patterns.
- Provider crates are not yet independent crates.io consumers of shared provider common code.

## Scope

- After PR-03 publishes the shared crate, update providers to depend on the crates.io version.
- Remove duplicated shared code from providers where PR-01 exposed stable replacements.
- Keep provider-specific HTTP payloads, provider API quirks, config structs, and credentials local.
- Run provider-specific tests and pack builds for each migrated provider.
- Split into one PR per provider if the combined diff is large.

## Out Of Scope

- Do not publish a new shared crate from this PR unless the migration uncovers missing API that requires a separate shared crate release.
- Do not change provider behavior except where explicitly covered by tests and migration notes.
- Do not remove old workflow branches; PR-08 handles cleanup.

## Implementation Tasks

1. Pin the published shared crate version in provider metadata.
2. Update provider `Cargo.toml` dependencies:
   - replace workspace/path `provider-common` with crates.io package/version where appropriate,
   - keep local path dependencies only with an explicit justification.
3. Migrate providers incrementally:
   - dummy,
   - email,
   - slack,
   - teams,
   - telegram,
   - webchat,
   - webchat-gui,
   - webex,
   - whatsapp.
4. For each provider:
   - remove duplicated shared helper code,
   - use shared crate public API,
   - run provider-specific tests,
   - build selected components,
   - build selected pack.
5. Update migration notes with any provider-specific exceptions.

## Acceptance Criteria

- Provider `Cargo.toml` dependencies use the shared crate version from crates.io.
- No provider depends on unpublished workspace-only common code unless explicitly justified.
- All provider packs build.
- Provider tests pass for each migrated provider.
- Provider behavior remains compatible with existing pack manifests and schemas.

## Review Notes

- Prefer small provider-by-provider PRs if review gets noisy.
- Keep commits grouped by provider so regressions can be bisected cleanly.
