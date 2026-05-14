# Provider Publish Workflows

`ci/provider-matrix.json` is the single checked-in source of truth for:

- provider name to pack mapping
- provider-owned version and GHCR target metadata
- provider name to buildable component targets
- provider name to Rust manifests used for targeted `fmt` and `clippy`
- provider-scoped paths that can stay on the fast path

`ci/provider_matrix.py` exposes these commands:

- `python3 ci/provider_matrix.py list-providers`
- `python3 ci/provider_matrix.py resolve-provider telegram`
- `python3 ci/provider_matrix.py affected --base <sha> --head <sha>`
- `python3 ci/provider_matrix.py detect-changes --base <sha> --head <sha>`

`affected` is the preferred command for new workflow work. `detect-changes`
is kept as a compatibility alias for existing workflows.

Every provider entry declares:

- `pack`: stable `.gtpack` pack name.
- `version`: provider release version.
- `ghcr_target`: provider-specific GHCR package target.
- `shared_crate_dependency`: shared provider crate version that the provider is expected to consume.
- `components`: buildable component package names for the provider.
- `manifests`: component manifests used for targeted Rust checks.
- `paths`: provider-owned paths that should only affect this provider.

Build-all fallback is intentionally conservative. The main workflow flips `build_all=true` when shared crates, shared WIT/build tooling, state/template/common packs, CI workflow files, or any unmapped path changes.

The matrix output also distinguishes docs-only, tooling-only, shared, and
provider-scoped changes so later workflow PRs can avoid publishing providers
for docs/tooling changes and can fan out intentionally only when shared code
changes.

Local provider-focused checks use the provider metadata:

```bash
python3 ci/provider_matrix.py resolve-provider slack
cargo check -p messaging-provider-slack
PACK_FILTER=messaging-slack ./ci/steps/11_build_packs.sh
```

To manually run the fast path in GitHub Actions:

1. Open `Publish Provider Fast Path`
2. Enter a provider such as `telegram`
3. Optionally enable `dry_run`

The fast path builds and lints only the mapped provider components, pulls templates from OCI during pack sync, validates only the mapped pack, and publishes only that provider's components and pack when `dry_run` is disabled.

`Provider Build, Test, and Publish` is the reusable workflow for new provider
orchestration. It supports both `workflow_call` and `workflow_dispatch` with:

- `provider`: one provider from `ci/provider-matrix.json`.
- `provider_version`: optional override; otherwise uses matrix metadata.
- `shared_crate_version`: optional summary metadata for rebuilds after a shared
  crate release.
- `publish`: when `false`, builds, validates, and uploads one `.gtpack` artifact
  without pushing to GHCR.
- `publish_latest`: when publishing, also updates the `latest` tag. The workflow
  keeps the existing `stable` behavior by setting `PUBLISH_STABLE=1`.
- `trigger_e2e_after_publish`: optional post-publish hook for the nightly
  provider e2e workflow. It defaults to disabled because live external services
  can be flaky or rate-limited.

Dry-run runs upload `gtpack-<pack>` as an artifact. Publish runs push only the
selected provider's components and pack to GHCR.

`Provider Release Orchestrator` is the dependency-aware entrypoint for `main`.
It uses `ci/provider_matrix.py affected` to classify changes:

- docs-only: select no providers and publish nothing.
- tooling/CI-only: select no providers by default.
- provider-only: call the reusable workflow for only the changed provider(s).
- shared crate: call `Publish Shared Provider Crate` first, then fan out to all
  providers with the published shared crate version in the downstream summary.

The legacy `Build, Test, and Publish Packs` workflow is retained for validation
coverage during migration, but its monolithic publish job is disabled so it no
longer publishes every provider for broad shared/tooling changes.
