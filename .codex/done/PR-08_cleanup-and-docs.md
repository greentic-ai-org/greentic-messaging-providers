# PR-08: Cleanup And Docs

## Purpose

Remove obsolete monolithic publishing paths and document the new provider release model.

## Current Audit Notes

- `.github/workflows/build-and-publish.yml` currently contains both validation and main/tag publish logic.
- `.github/workflows/publish-provider.yml` is a manual provider fast path, likely to be replaced or reduced after PR-04 and PR-05.
- `tools/publish_oci.sh`, `tools/publish_packs_oci.sh`, `ci/steps/*`, pack validation, component doctor, flow doctor, and provider tests are still important and should either remain supported or be replaced with documented equivalents.
- `.codex/repo_overview.md` will need updates once crate boundaries, workflows, and release policy change.
- Existing packs under `packs/messaging-*` must continue to build.

## Scope

- Remove dead scripts/workflow branches once new provider workflows are live.
- Update README and operator/developer docs.
- Update `.codex/repo_overview.md`.
- Document how to:
  - add a provider,
  - release shared code,
  - release one provider,
  - force rebuild all providers,
  - inspect GHCR output,
  - recover from crates.io publish failure,
  - recover from GHCR publish failure.
- Ensure no obsolete workflow still publishes all providers for provider-only changes.

## Out Of Scope

- Do not redesign version policy in this PR.
- Do not migrate provider source code in this PR.
- Do not remove scripts still referenced by active workflows or local developer docs.

## Implementation Tasks

1. Inventory all workflow and script entrypoints after PR-05:
   - `.github/workflows/build-and-publish.yml`
   - `.github/workflows/publish-provider.yml`
   - reusable provider workflow
   - shared crate publish workflow
   - `tools/publish_oci.sh`
   - `tools/publish_packs_oci.sh`
   - `ci/steps/*`
2. Remove or deprecate obsolete monolithic branches.
3. Update docs:
   - repository README,
   - release policy docs,
   - provider author docs,
   - `.codex/repo_overview.md`.
4. Add troubleshooting docs:
   - crates.io version already published,
   - crates.io publish failed after version bump,
   - GHCR push failed after successful build,
   - provider fanout accidentally selected too much,
   - rerunning failed provider publish safely.
5. Validate the final model:
   - docs-only changes do not publish,
   - provider-only changes do not publish unrelated providers,
   - shared crate changes intentionally fan out after shared crate release,
   - manual all-provider rebuild remains possible.

## Acceptance Criteria

- No obsolete workflow still publishes all providers for provider-only changes.
- Docs match actual commands and workflows.
- Existing packs under `packs/messaging-*` still build.
- CI passes.
- `.codex/repo_overview.md` reflects the final architecture.

## Review Notes

- Be conservative when deleting scripts. Keep local developer workflows working unless the replacement is documented and tested.
- This PR should make the new release model boring to operate.
