# Provider Publish Workflows

`ci/provider-matrix.json` is the single checked-in source of truth for:

- provider name to pack mapping
- provider-owned version and component GHCR target metadata
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
- `ghcr_target`: provider-specific component GHCR target.
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

To manually run a focused provider release in GitHub Actions, open
`Provider Build, Test, and Publish`. It is the single reusable/direct workflow
for one-provider builds and supports both `workflow_call` and
`workflow_dispatch` with:

- `provider`: one provider from `ci/provider-matrix.json`.
- `shared_crate_version`: optional summary metadata for rebuilds after a shared
  crate release.
- `publish`: when `false`, builds, validates, and uploads one `.gtpack` artifact
  without pushing to GHCR.
- `publish_latest`: when publishing, also updates the `latest` tag. The workflow
  keeps the existing `stable` behavior by setting `PUBLISH_STABLE=1`.
- `trigger_e2e_after_publish`: optional post-publish hook for the nightly
  provider e2e workflow. It defaults to disabled because live external services
  can be flaky or rate-limited.

Validation-only runs upload `gtpack-<pack>` as an artifact. Publish runs push
only the selected provider's components and pack to GHCR. Component WASM images
use the provider matrix `ghcr_target`; `.gtpack` packages are published under
`ghcr.io/<owner>/packs/messaging/<pack>`.

The `teams` provider input resolves to the current Bot Framework-backed
`messaging-teams` pack. Use `messaging-teams-graph` when you intentionally need
the legacy Graph-backed Teams provider.

For a local one-command fast path, use:

```bash
scripts/publish_provider.sh <provider> [version]
```

The helper optionally bumps the provider version, validates metadata, builds the
selected provider locally, runs targeted checks, and dispatches this workflow on
the current branch. Local-only changes are not visible to GitHub Actions, so
commit and push release fixes before using it for a real publish.

`Provider Release Orchestrator` is the manual dependency-aware provider release
entrypoint. Pushes to `main` do not start it. After merging, a maintainer can
start it with one provider, `providers=all`, or `providers=shared+all`, and can
choose whether that run is validation-only or publishes with `publish=true`.

It uses `ci/provider_matrix.py affected` to classify changes:

- docs-only: select no providers and publish nothing.
- tooling/CI-only: select no providers by default.
- provider-only: call the reusable workflow for only the changed provider(s).
- shared crate: call `Publish Shared Provider Crate` first, then fan out to all
  providers when the maintainer explicitly selects that mode.
