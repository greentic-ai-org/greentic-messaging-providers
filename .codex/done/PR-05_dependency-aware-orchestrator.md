# PR-05: Dependency-Aware Orchestrator

## Purpose

Replace monolithic all-provider publishing with explicit dependency-aware orchestration.

## Current Audit Notes

- `ci/provider-matrix.json` has a broad `shared_paths` list that includes `.github/workflows/`, many `ci/*` paths, all shared crates, `Cargo.toml`, `Cargo.lock`, `wit/`, and several tools.
- `ci/provider_matrix.py detect-changes` immediately returns `build_all=true` when any `shared_paths` entry changes, when an unmapped path changes, or when a path maps to multiple providers.
- `.github/workflows/build-and-publish.yml` uses `affected_components` and `affected_packs` from that result, then publishes all affected artifacts on `main` or tags.
- This means common code, tooling, or unmapped changes can fan out into all providers.
- Current docs/tooling-only changes are not distinguished from shared provider code changes.

## Scope

- Refactor `.github/workflows/build-and-publish.yml` or introduce a new orchestrator workflow.
- Use `ci/provider_matrix.py` and provider metadata to classify changes:
  - docs-only,
  - tooling/CI-only,
  - provider-only,
  - shared-crate changed,
  - mixed changes.
- Replace broad `shared_paths` fanout with explicit dependency-aware behavior.
- Call the shared crate publish workflow for shared crate changes.
- After shared crate publish succeeds, call the reusable provider workflow for every provider.
- For provider-only changes, call the reusable provider workflow only for changed providers.
- Preserve manual dispatch options:
  - all providers,
  - selected provider(s),
  - shared crate + all providers,
  - dry-run vs publish.

## Out Of Scope

- Do not migrate provider code to crates.io dependencies in this PR.
- Do not change provider/shared version policy beyond consuming the version metadata established earlier.
- Do not delete legacy scripts until PR-08.

## Implementation Tasks

1. Extend `ci/provider_matrix.py affected` to emit structured classification:
   - `classification`
   - `shared_crate_changed`
   - `tooling_changed`
   - `docs_only`
   - `provider_changed`
   - `affected_providers`
   - `affected_components`
   - `affected_packs`
   - selection reasons per provider.
2. Narrow `shared_paths` in `ci/provider-matrix.json`:
   - separate true shared crate paths from tooling/CI paths,
   - keep provider path ownership explicit.
3. Update orchestration:
   - docs-only: no build/publish,
   - tooling/CI-only: run CI validation but do not publish providers unless explicitly requested,
   - provider-only: call reusable provider workflow for changed providers,
   - shared-crate changed: publish shared crate first, then call every provider workflow using the published version,
   - mixed provider + shared: treat as shared fanout and explain why.
4. Preserve PR behavior:
   - no crates.io publish,
   - no GHCR publish,
   - focused tests/builds for changed providers,
   - shared crate tests when shared crate paths changed.
5. Preserve `main` behavior:
   - publish only selected provider packs for provider-only changes,
   - publish shared crate before fanout for shared changes.
6. Add workflow summaries:
   - changed files,
   - classification,
   - provider selection/rejection reasons,
   - publish/dry-run decision.
7. When provider publish workflows are called, pass through an optional post-publish e2e setting that can trigger the provider nightly e2e workflow for the provider that was just published. Default this setting to disabled and non-blocking unless maintainers explicitly enable a blocking release gate.

## Acceptance Criteria

- Changing `components/messaging-provider-slack/**` only triggers the Slack provider workflow.
- Changing `packs/messaging-slack/**` only triggers the Slack provider workflow.
- Changing shared crate code triggers shared crate publish and then all provider workflows.
- Changing one provider never publishes unrelated providers.
- Changing docs only does not publish anything.
- The workflow summary clearly shows why each provider was or was not selected.
- Optional post-publish e2e dispatch is configurable, disabled by default, and does not block normal publish unless explicitly enabled.

## Review Notes

- This PR is the behavior change that removes broad fanout. Keep it reviewable by leaning on metadata and the reusable workflow from PR-04.
- Keep compatibility shims only as long as needed for a safe transition.
