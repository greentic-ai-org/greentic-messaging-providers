# PR-03: Shared Crate Publish Workflow

## Purpose

Publish shared provider code to crates.io independently from provider packs.

## Current Audit Notes

- There is no dedicated shared crate release workflow today.
- `.github/workflows/build-and-publish.yml` runs on pushes to all branches and tags, then publishes components/packs on `main` or tags.
- The current workflow derives publish version from `Cargo.toml` workspace package version.
- Current PR validation is intentionally not wired to publish, but the main publish path still couples provider pack publishing to broad affected change detection.
- `provider-common` is currently a workspace/path dependency, so crates.io publication requires package metadata and a dry-run before this PR can publish.

## Scope

- Add a GitHub Actions workflow dedicated to shared crate release.
- Trigger on:
  - push to `main` when shared crate files change,
  - manual dispatch with explicit crate/version inputs where useful.
- The workflow should run fmt, clippy, tests, `cargo publish --dry-run`, and publish to crates.io only from the allowed release path.
- Expose the published crate name/version as an output or artifact for downstream provider rebuild workflows.
- Document `CARGO_REGISTRY_TOKEN`.
- Add idempotency checks so reruns do not fail if the version is already published.

## Out Of Scope

- Do not rebuild/publish provider packs in this PR.
- Do not migrate providers to crates.io dependencies in this PR.
- Do not replace the monolithic provider workflow yet.

## Implementation Tasks

1. Add a workflow such as `.github/workflows/publish-shared-crate.yml`.
2. Use path filters for the selected shared crate and any release metadata files.
3. Verify release readiness:
   - `cargo fmt --check`
   - `cargo clippy -p <shared-crate> --all-targets`
   - `cargo test -p <shared-crate>`
   - `cargo publish --dry-run -p <shared-crate>`
4. Publish only when:
   - event is `push` to `main` with a releaseable version, or
   - event is `workflow_dispatch` with explicit publish intent.
5. Before publishing, check crates.io for the exact version and skip successfully if it already exists.
6. Upload or emit release metadata:
   - crate package name,
   - version,
   - git SHA,
   - whether publish happened or was skipped as already published.
7. Add docs for cutting a shared crate release and required secrets.

## Acceptance Criteria

- PR validation never publishes to crates.io.
- Main branch shared-code changes publish only the shared crate.
- The workflow exposes the published crate name/version for provider rebuild workflows.
- Rerunning the workflow for an already-published version exits successfully with a clear summary.
- Documentation explains how to cut and verify a shared crate release.

## Review Notes

- Keep workflow outputs stable because PR-05 will depend on them.
- If exact GitHub Actions cross-workflow outputs are awkward, use an artifact or repository dispatch payload and document the handoff.
