# PR-01 — Add a new-provider-only answer-owned workspace lane

## Goal

Add the minimal repository structure needed for two new providers, HTTP and WebSocket, to be generated from checked-in answer files.

This PR must **not** reorganise or convert existing providers. Existing providers such as Slack, Teams, Telegram, WebChat, WebChat GUI, WebEx, Email, Microsoft Email, and WhatsApp keep their current directories, scripts, manifests, lock files, and packaging behavior.

## Non-goals

- Do not move existing `components/*` crates.
- Do not move existing `packs/*` directories.
- Do not convert existing provider `pack.yaml`, `pack.manifest.json`, `pack.lock.cbor`, or component manifests into answer-owned generation.
- Do not remove checked-in generated artifacts for existing providers.
- Do not change the root workspace layout beyond what is needed for the two new providers.
- Do not introduce a repo-wide provider workspace migration.

## Background

The repo already has an answer-owned pattern for generated packs, for example `messaging-teams/build-answer.json` plus `messaging-teams/build_pack.sh`, and generated-answer examples under `packs/messaging-email` and `packs/messaging-microsoft-email`.

Going forward, new providers should follow that style: one answer file is the source of truth, and generated YAML/JSON/CBOR/package artifacts are produced by scripts instead of being edited manually.

The first two providers to use this rule are:

- `messaging-http`
- `messaging-websocket`

## Proposed structure

Add a new isolated area for answer-owned providers:

```text
generated-providers/
├── messaging-http/
│   ├── build-answer.json
│   ├── build_pack.sh
│   ├── src/
│   ├── assets/
│   └── tests/
└── messaging-websocket/
    ├── build-answer.json
    ├── build_pack.sh
    ├── src/
    ├── assets/
    └── tests/
```

Use a different directory name if implementation finds an existing repo convention that is more appropriate, but keep the scope limited to the two new providers.

## Answer-Owned Provider Contract

Each new provider owns:

- `build-answer.json` as the source of truth for pack creation and packaging
- provider source under `src/`
- static assets, setup specs, schemas, docs, and fixtures under `assets/`
- provider-local tests under `tests/`
- a `build_pack.sh` wrapper that applies the answer file and packages the provider

Generated pack inputs may be materialized under `target/generated/<provider>.pack`.

Generated final package artifacts should continue to land in the repo's normal output location:

```text
dist/packs/<provider>.gtpack
```

## Existing Provider Safety Rule

The implementation should add guardrails in docs/tests/scripts that make the boundary explicit:

- new answer-owned scripts may target only `messaging-http` and `messaging-websocket`
- existing provider build scripts continue to work as they do today
- existing provider manifests are not rewritten by the new generator
- existing provider versions are not changed by this PR unless a separate existing-provider code change requires it

## Acceptance Criteria

- A clear new-provider-only directory and contract exists for `messaging-http` and `messaging-websocket`.
- Existing provider paths are untouched except for shared scripts/tests needed to discover the two new providers.
- The PR documentation states that existing providers are intentionally not being refactored.
- Generated files for the new providers are created by answer-owned scripts, not by manual edits to YAML/JSON/CBOR files.
