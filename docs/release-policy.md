# Release Policy

Versions belong to the artifact being released.

## Source Of Truth

- Shared provider crate: `crates/provider-common/Cargo.toml`.
- Provider release version: `ci/provider-matrix.json` `providers.<name>.version`.
- Provider component crate versions: every manifest listed in that provider's
  `manifests` entry.
- Provider pack version: `packs/<pack>/pack.yaml` and generated
  `pack.manifest.json`.

Use `tools/provider_versions.py` to inspect or update versions:

```bash
python3 tools/provider_versions.py list
python3 tools/provider_versions.py shared
python3 tools/provider_versions.py provider slack
python3 tools/provider_versions.py validate
scripts/change_provider_version.sh slack 0.5.0
python3 tools/provider_versions.py set-shared 0.5.0
```

`scripts/change_provider_version.sh <provider> <version>` is the preferred
provider bump entrypoint. It wraps `tools/provider_versions.py set-provider`,
validates that the matrix, component manifests, and pack metadata agree, and
builds that provider locally. Pass `--no-build` when only metadata should be
updated.

The current policy uses explicit manual version bumps. We are not deriving
release versions from commit messages, workspace version, or hidden changeset
state.

## Shared Crate

`greentic-messaging-provider-common` has independent semver. A shared crate
release does not automatically require editing every provider version.

Example:

```bash
python3 tools/provider_versions.py set-shared 0.5.0
cargo test -p greentic-messaging-provider-common
cargo publish --dry-run -p greentic-messaging-provider-common
```

After merge to `main`, `Publish Shared Provider Crate` publishes the new crate
version if it is not already on crates.io. The workflow uploads
`shared-crate-release-metadata` with the crate name, version, SHA, and publish
status.

## Provider Releases

A provider release version must match:

- provider matrix `version`
- every component crate manifest listed for that provider
- `packs/<pack>/pack.yaml`
- top-level `packs/<pack>/pack.manifest.json` version

Example one-provider release:

```bash
scripts/change_provider_version.sh slack 0.5.0
```

The reusable provider workflow passes the provider version as both component
`VERSION` and pack `PACK_VERSION`, so exact semver GHCR tags derive from the
provider version, not the workspace version.

## Shared-Crate Fanout

When shared code changes, push the merge to `main` first, then decide whether
to run a focused provider release or an all-provider fanout. `Provider Release
Orchestrator` is manual-only; it does not start provider jobs from a `main`
push. Publishing providers requires an explicit manual dispatch with
`publish=true`, or the one-provider `scripts/publish_provider.sh` helper.
Provider tags remain deterministic:

- If a provider version is bumped, publish the new exact semver tag.
- If maintainers intentionally rebuild without bumping a provider version, the
  workflow may refresh `stable`/`latest` according to the inputs, but the exact
  semver tag must be treated as an idempotent rebuild of the same provider
  release.

For public releases, prefer bumping affected provider versions when the shared
crate change changes provider behavior.

## GHCR Tags

- Exact semver tags, for example `0.5.0`, are release identifiers.
- `stable` is preserved for the latest maintainer-approved provider release.
- `latest` is opt-in for provider workflows through `publish_latest`; use it for
  consumers that intentionally track the freshest published artifact.
- OCI digest references are immutable and preferred for production pinning:
  `ghcr.io/<org>/greentic-messaging-providers/packs/messaging/<pack>@sha256:...`

## E2E Release Records

Provider release notes should record live e2e status for the released `.gtpack`
version or digest:

- provider
- pack version or OCI digest
- workflow run URL
- result: `passed`, `failed`, `skipped`, or `not run`
- whether e2e gating was enabled

Emergency provider releases must remain possible when live e2e is flaky or
rate-limited unless maintainers explicitly enable a blocking release gate.

Operational runbooks for adding providers, forcing rebuilds, inspecting GHCR,
and recovering from publish failures live in `docs/provider-release-operations.md`.
