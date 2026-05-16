# Provider Release Operations

This repository now separates shared crate releases from provider pack releases.

## Add A Provider

1. Add component crate(s) under `components/`.
2. Add the pack under `packs/messaging-<name>/`.
3. Add provider tests under `crates/provider-tests/tests/`.
4. Add the provider to `ci/provider-matrix.json` with:
   - `pack`
   - `version`
   - `ghcr_target`
   - `shared_crate_dependency`
   - `components`
   - `manifests`
   - provider-owned `paths`
5. Run:

```bash
python3 ci/provider_matrix.py list-providers
python3 tools/provider_versions.py validate --provider <name>
cargo check -p messaging-provider-<name>
scripts/build_providers.sh <name>
```

## Release Shared Code

Shared provider code lives in `crates/provider-common` and is published as
`greentic-messaging-provider-common`.

```bash
python3 tools/provider_versions.py set-shared 0.5.0
cargo test -p greentic-messaging-provider-common
cargo publish --dry-run -p greentic-messaging-provider-common
```

After merge to `main`, run or allow `Publish Shared Provider Crate`. The
dependency-aware orchestrator waits for that workflow before rebuilding
providers for shared-code changes.

## Release One Provider

```bash
scripts/change_provider_version.sh slack 0.5.0
```

`scripts/change_provider_version.sh` validates the selected provider metadata
and builds that provider locally. Pass `--no-build` for metadata-only changes.

For `webchat-gui`, the focused local build preserves the browser assets already
checked into `packs/messaging-webchat-gui`. Set
`GREENTIC_WEBCHAT_SITE_DIR=/path/to/site/app` when the release should import a
new WebChat SPA build before packaging.

After committing and pushing the change, publish the selected provider with:

```bash
scripts/publish_provider.sh slack 0.5.0
```

The publish helper runs the same version update, validation, focused local
build, and targeted local checks before dispatching `provider-build-publish.yml`.
Pass `--dry-run` to upload the workflow artifact without pushing to GHCR, or
`--publish-latest` when the release should also move `latest`.

Equivalent manual workflow inputs are:

- `provider=slack`
- `publish=true`
- `publish_latest=false` unless the release should also move `latest`

The workflow always uploads the built `.gtpack` artifact. Publish mode pushes
only the selected provider components and pack to GHCR.

## Force Rebuild All Providers

Use `Provider Release Orchestrator` with:

- `providers=all`
- `publish=false` for validation/dry-run artifacts
- `publish=true` to push all provider artifacts

Use `providers=shared+all` when the run should also execute the shared crate
publish workflow before provider fanout.

Pushes to `main` do not start the orchestrator. After the merge, decide whether
to run a focused provider release or an all-provider fanout, then start the
manual workflow dispatch with the matching `providers` and `publish` inputs.

## Inspect GHCR Output

Provider components are published under:

```text
ghcr.io/<owner>/greentic-messaging-providers/<component>:<provider-version>
```

Provider packs are published under:

```text
ghcr.io/<owner>/greentic-messaging-providers/packs/messaging/<pack>:<provider-version>
```

Prefer immutable digests for production pinning:

```bash
oras manifest fetch ghcr.io/<owner>/greentic-messaging-providers/packs/messaging/messaging-slack:<version>
```

## Recovery

### crates.io Version Already Published

`Publish Shared Provider Crate` treats an already-published exact version as a
successful idempotent run. If the source changed after that version was
published, bump `crates/provider-common/Cargo.toml` and rerun.

### crates.io Publish Failed After Version Bump

Fix the publish failure, keep the same version if crates.io did not accept it,
and rerun. If crates.io accepted the version but a later workflow step failed,
do not republish the same version; rerun downstream provider orchestration.

### GHCR Push Failed

Provider publish workflows are scoped to one provider. Rerun the failed provider
with the same version. Exact semver tags should be treated as the same release;
`stable` and optional `latest` may be refreshed by the rerun.

### Too Many Providers Were Selected

Check the `Provider Release Plan` summary. If a path was classified as shared or
unmapped, update `ci/provider-matrix.json` ownership so future changes route to
the intended provider.

### E2E Status

Provider release notes should record nightly/manual e2e status for the released
`.gtpack` version or digest. Emergency releases are allowed unless maintainers
explicitly enable a blocking e2e gate.
