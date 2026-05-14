# PR-01: Shared Crate Boundaries

## Purpose

Define and implement the shared provider crate boundary without changing provider behavior.

## Current Audit Notes

- The workspace currently has several shared/provider-adjacent crates: `crates/provider-common`, `crates/provider-runtime-config`, `crates/messaging-core`, `crates/greentic-messaging-renderer`, `crates/greentic-messaging-planned`, `crates/webchat-directline-core`, and provider test/tooling crates.
- `crates/provider-common` is already the closest shared provider crate. It contains `ProviderError`, universal render/send helper code, schema-core helpers, QA invoke bridge code, Adaptive Card conversion, pack metadata tests, and utility tests.
- Most provider components depend on `provider-common` via workspace/path dependencies, and many providers duplicate config validation, invoke dispatch, provider-core response shaping, HTTP/secrets/state host glue, and send/render/encode patterns.
- `Cargo.toml` currently pins `provider-common = { path = "crates/provider-common" }` under `[workspace.dependencies]`, so it is not yet a crates.io boundary.
- `.codex/repo_overview.md` mentions `crates/messaging-universal-dto`, but the crate is not present as a workspace member in the current checkout. Universal DTOs appear to come from `greentic-types::messaging::universal_dto`.

## Scope

- Audit `crates/provider-common`, `crates/provider-runtime-config`, `crates/messaging-core`, renderer/planned crates, `webchat-directline-core`, and provider-local duplicate logic under `components/messaging-provider-*`.
- Decide the publishable crate name. Prefer an existing-compatible path if possible:
  - Option A: rename/package `provider-common` as `greentic-messaging-provider-common` while preserving crate import compatibility if feasible.
  - Option B: keep crate import name `provider_common` and publish package name `greentic-messaging-provider-common`.
- Consolidate reusable DTO wrappers, provider-core helpers, manifest/schema helpers, error handling, common HTTP/secrets/state abstractions, config helpers, and shared render/send utilities into the shared crate.
- Add crate README, crate-level docs, examples, changelog/release note entry, package metadata, and focused tests.
- Keep workspace path dependencies in this PR if needed. Design the public API so provider crates can later consume the crate from crates.io.

## Out Of Scope

- Do not switch providers to crates.io dependencies in this PR.
- Do not redesign workflow orchestration in this PR.
- Do not bump every provider version in this PR.

## Implementation Tasks

1. Inventory duplicate provider code:
   - `dispatch_json_invoke` and schema-core bridge code.
   - `validate_config_out` patterns.
   - secret lookup and missing-secret error formatting.
   - HTTP response parsing and provider message id extraction.
   - manifest/config schema response helpers.
   - render/encode/send universal op helpers.
2. Produce an API boundary note in the PR description:
   - what belongs in shared code,
   - what remains provider-local,
   - what is intentionally deferred.
3. Update the shared crate package metadata:
   - `description`
   - `repository`
   - `license`
   - `readme`
   - `keywords` / `categories` if useful
   - publishable package name
4. Add or update docs:
   - crate-level `//!` docs,
   - README with examples,
   - changelog entry,
   - migration note listing provider code to replace in later PRs.
5. Add focused tests for the public API and keep existing tests passing.

## Acceptance Criteria

- `cargo test -p <shared-crate>` passes.
- Public API is documented and has stable examples for provider authors.
- Crate metadata is crates.io-ready and `cargo publish --dry-run -p <shared-crate>` is expected to pass once publish credentials exist.
- Provider behavior does not change.
- Migration note identifies provider-local code that should be replaced by the shared API in PR-07.

## Review Notes

- Reviewers should look for a small, stable API surface rather than a broad move of every utility into common code.
- Any large or risky consolidation should be explicitly deferred to provider-by-provider migration PRs.
