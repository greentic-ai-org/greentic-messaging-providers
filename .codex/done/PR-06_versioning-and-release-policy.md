# PR-06: Versioning And Release Policy

## Purpose

Define how shared crate versions, provider versions, pack versions, and GHCR tags are managed.

## Current Audit Notes

- The workspace currently uses `[workspace.package] version = "0.4.99"`.
- Most crates and provider components inherit `version.workspace = true`.
- `publish-provider.yml`, `build-and-publish.yml`, `ci/steps/07_sync_packs.sh`, `ci/steps/11_build_packs.sh`, and `tools/publish_packs_oci.sh` derive pack publish versions from the workspace version unless `PACK_VERSION` is supplied.
- `tools/publish_oci.sh` publishes component tags from `VERSION`.
- Current main publish uses `PUBLISH_LATEST=1` and `PUBLISH_STABLE=1` for components and packs.
- The manual provider fast path publishes packs with `PUBLISH_STABLE=1` and optional `publish_latest`.

## Scope

- Add release policy docs for:
  - shared crate semver,
  - provider semver,
  - pack version,
  - GHCR exact semver tags,
  - `latest`,
  - `stable`,
  - OCI digest references.
- Decide and implement the version source of truth:
  - manual `Cargo.toml` bump,
  - changeset files,
  - conventional commits,
  - or a simple repo-local release manifest.
- Implement tooling to read/update provider versions and shared crate version.
- Ensure pack version and GHCR tags derive from provider version, not the workspace version.
- Ensure shared crate version is independent from provider versions.
- Provider releases should record whether the nightly/manual e2e test passed for the released `.gtpack` version or OCI digest. Do not block emergency provider releases solely on nightly e2e unless repository maintainers intentionally enable that policy.

## Out Of Scope

- Do not migrate provider source code to crates.io dependencies in this PR.
- Do not remove legacy workflows in this PR.
- Do not change pack/component names.

## Implementation Tasks

1. Choose a release policy and document the tradeoff.
2. Add docs under an appropriate location, for example `docs/release-policy.md`.
3. Add or extend tooling:
   - read shared crate version,
   - read provider version,
   - validate pack version equals provider version,
   - optionally bump versions.
4. Update workflow/script version plumbing:
   - reusable provider workflow passes `PACK_VERSION=<provider_version>`,
   - component publish uses provider version for provider-owned components,
   - pack publish uses provider version,
   - shared crate workflow uses shared crate version.
5. Define shared-crate fanout version behavior:
   - if shared crate changes require provider rebuilds, decide whether provider versions must be bumped,
   - if automatic rebuild tags are allowed, define deterministic pre-release/build metadata or require explicit provider version bumps.
6. Document current `latest` / `stable` migration:
   - preserve current behavior if still desired,
   - or intentionally change it with operator migration notes.
7. Define how release notes record e2e status:
   - nightly/manual e2e workflow run URL,
   - provider,
   - `.gtpack` version/tag/digest,
   - result: passed / failed / skipped / not run,
   - whether release blocking was enabled.

## Acceptance Criteria

- Provider pack version equals provider crate/release version.
- Shared crate version can advance without manually editing every provider version unless the policy says provider releases must be intentionally bumped.
- Provider rebuilds caused by shared-crate changes produce deterministic version/tag behavior.
- Release docs include examples for:
  - shared crate release,
  - one provider release,
  - all provider rebuild,
  - `latest`,
  - `stable`,
  - exact semver tags,
  - OCI digest references.
- Provider release records include nightly/manual e2e status for the released `.gtpack` version or digest, while emergency releases remain possible unless maintainers intentionally enable e2e gating.

## Review Notes

- Avoid hidden version magic. Reviewers should be able to predict tags from files in the repo.
- If preserving both `latest` and `stable`, clearly define who can move each tag and when.
