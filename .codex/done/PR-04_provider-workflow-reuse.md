# PR-04: Provider Workflow Reuse

## Purpose

Create a reusable provider build/test/publish workflow that operates on exactly one provider at a time.

## Current Audit Notes

- `.github/workflows/publish-provider.yml` is already a manual provider fast path, but it is not reusable via `workflow_call`.
- The fast path can build one selected provider and honors `PACK_FILTER` / `COMPONENT_FILTER`.
- It still derives the publish version from the workspace version and publishes selected components with `PUBLISH_STABLE=1`; pack publishing has optional `publish_latest` and always sets `PUBLISH_STABLE=1`.
- `.github/workflows/build-and-publish.yml` duplicates setup steps and publishes many affected components/packs from one monolithic workflow.
- `ci/steps/*`, `tools/publish_oci.sh`, and `tools/publish_packs_oci.sh` already support filters that should be reused rather than replaced.

## Scope

- Add a reusable workflow such as `.github/workflows/provider-build-publish.yml` callable with:
  - provider name,
  - provider version,
  - shared crate version,
  - publish boolean,
  - publish tags/options.
- Keep a manual dispatch wrapper for one-provider dry-run/publish use.
- Build only provider components, run provider-relevant tests and pack validation, build one `.gtpack`, and publish only that provider when requested.
- Factor existing setup from `publish-provider.yml`, `build-and-publish.yml`, and `ci/steps/*`.

## Out Of Scope

- Do not implement dependency-aware orchestration in this PR.
- Do not publish all providers from this reusable workflow by default.
- Do not change provider version policy beyond accepting a provider version input.

## Implementation Tasks

1. Convert or add a workflow with `workflow_call` inputs:
   - `provider`
   - `provider_version`
   - `shared_crate_version`
   - `publish`
   - `publish_latest`
   - `publish_stable`
   - optional `dry_run`
2. Resolve provider metadata through `ci/provider_matrix.py`.
3. Install Rust, `cargo-component`, `greentic-pack`, `greentic-flow`, `greentic-component`, and `oras` using existing setup patterns.
4. Run targeted lint:
   - `COMPONENT_MANIFESTS_JSON` into `ci/steps/01_fmt.sh`
   - `COMPONENT_MANIFESTS_JSON` into `ci/steps/02_clippy.sh`
5. Build only selected provider components using existing `tools/build_components/<component>.sh` scripts.
6. Run provider-specific tests:
   - relevant `crates/provider-tests/tests/provider_core_<provider>.rs`,
   - additional provider-specific tests from metadata when present.
7. Build and validate only the selected pack:
   - `PACK_FILTER=<pack>`
   - `PACK_VERSION=<provider_version>`
8. Publish only selected artifacts when `publish=true`:
   - `COMPONENT_FILTER=<provider components>`
   - `PACK_FILTER=<provider pack>`
   - exact provider semver tag
   - `stable` / `latest` behavior per inputs.
9. Upload one `.gtpack` artifact for dry runs.

## Acceptance Criteria

- Manual workflow dispatch can build/publish one provider.
- Dry-run mode uploads one `.gtpack`.
- Publish mode pushes one provider pack to GHCR.
- Existing `latest` and `stable` tagging behavior is preserved or explicitly documented as changed.
- No unselected provider components or packs are published.
- Add an optional post-publish hook to invoke the provider nightly e2e workflow for the provider that was just published. The hook must be disabled by default and non-blocking unless explicitly configured, because real external services can be flaky or rate-limited.

## Review Notes

- Prefer reusing existing scripts with filters over introducing parallel script stacks.
- Make workflow summaries show provider, version, pack, components, publish mode, and tags pushed.
