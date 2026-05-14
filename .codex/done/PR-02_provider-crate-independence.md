# PR-02: Provider Crate Independence

## Purpose

Make every provider independently versioned and independently buildable/testable.

## Current Audit Notes

- All provider component crates in `components/*/Cargo.toml` currently use `version.workspace = true`; the workspace version is `0.4.99`.
- The current provider matrix lives in `ci/provider-matrix.json` and maps provider names to packs, components, manifests, and path triggers.
- `ci/provider_matrix.py` currently supports `resolve-provider` and `detect-changes`; it does not list provider versions or classify docs/tooling/shared/provider-only changes.
- Provider fast path `.github/workflows/publish-provider.yml` resolves one provider but still derives `PUBLISH_VERSION` from the workspace version.
- Current pack names and component names are stable and must remain stable:
  - `dummy` -> `messaging-dummy`
  - `email` -> `messaging-email`
  - `slack` -> `messaging-slack`
  - `teams` -> `messaging-teams`
  - `telegram` -> `messaging-telegram`
  - `webchat` -> `messaging-webchat`
  - `webchat-gui` -> `messaging-webchat-gui`
  - `webex` -> `messaging-webex`
  - `whatsapp` -> `messaging-whatsapp`

## Scope

- For every provider in `ci/provider-matrix.json`, assign provider-owned version metadata.
- Replace implicit workspace-version coupling for provider components with provider-local versions.
- Extend `ci/provider-matrix.json` or introduce a provider metadata file that includes:
  - provider name
  - component crate(s)
  - pack name
  - provider version
  - GHCR target
  - paths that trigger this provider
  - shared-crate dependency version
- Add provider matrix commands for listing providers, resolving provider metadata, and detecting affected providers.
- Ensure provider-specific tests/builds can be addressed by provider name.

## Out Of Scope

- Do not publish crates or packs from this PR.
- Do not change GHCR tagging policy yet, except to make provider version metadata available.
- Do not switch shared dependency from path to crates.io yet unless PR-01 has already published and the change is trivial.

## Implementation Tasks

1. Decide metadata location:
   - Keep a single `ci/provider-matrix.json`, or
   - add provider-local metadata files and have `ci/provider_matrix.py` read them.
2. Add `ci/provider_matrix.py list-providers`:
   - output provider names, versions, packs, components, and GHCR targets.
3. Add or rename affected detection command:
   - support `ci/provider_matrix.py affected --base ... --head ...`;
   - keep compatibility with the existing `detect-changes` command until workflows are updated.
4. Update provider component `Cargo.toml` files:
   - set provider-owned package versions for component crates that are released as part of a provider;
   - keep non-provider helper components clearly classified.
5. Add provider-focused build/test command documentation:
   - component build command(s),
   - provider tests under `crates/provider-tests/tests/provider_core_*.rs`,
   - pack build command with `PACK_FILTER`.

## Acceptance Criteria

- `ci/provider_matrix.py list-providers` lists all providers and versions.
- `ci/provider_matrix.py affected --base ... --head ...` reports only changed providers for provider-only edits.
- Each provider can be built independently by provider name.
- No unrelated provider is marked affected by provider-local changes.
- Existing pack names and component names remain stable.

## Review Notes

- Treat metadata shape as an API for later workflow PRs.
- Prefer the smallest representation that lets workflows avoid bespoke path logic.
