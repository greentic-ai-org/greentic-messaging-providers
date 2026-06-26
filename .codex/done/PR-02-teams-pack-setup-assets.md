# PR-02: Teams Pack Setup Assets

## Title

Generate the messaging-teams pack from answers and ship setup assets

## Goal

Move the setup wizard from the local tester prototype into a new
`messaging-teams` source package that generates the Teams provider pack from
answer JSON.

This PR keeps the runtime behavior unchanged. It does not port Bot Framework
ingress to Rust/WASM.

## Context

PR-01 proves the setup UX inside `scripts/test_teams_bot.sh`.

The `messaging-teams` provider must not be hand-maintained as a checked-in
`packs/messaging-teams` tree. Follow the `../greentic-demo` pattern:

- source-owned files live under a compact source directory
- reusable static files live under `./assets`
- Rust/component implementation lives under `./src`
- pack, flow, manifest, lock, and dist files are generated from answer JSON
- any pack overlay is declared through answer JSON, not manual pack edits

## Scope

Create the source package:

```text
messaging-teams/
  build-answer.json
  assets/
    setup/
      greentic-teams-setup.js
      greentic-teams-setup.d.ts
      README.md
      examples/basic.html
    teams-app/
      manifest.template.json
  src/
    README.md
```

`build-answer.json` must be the source of truth for creating/updating the
generated `messaging-teams` pack. It should follow the same shape used in
`../greentic-demo/crates/*/build-answer.json`, with at least:

- `pack_create` answers for `greentic-pack wizard apply`
- `pack` update answers for generated metadata/build validation
- `pack_overlay.files` entries for metadata that the wizard cannot yet express
- no generated `pack.yaml`, `pack.manifest.json`, `flows/`, or `dist/` edits as
  the primary implementation

Define provider setup endpoints for the wizard to call, for example:

```text
GET  /v1/messaging/setup/messaging-teams/{tenant}
POST /v1/messaging/setup/messaging-teams/{tenant}/next
POST /v1/messaging/setup/messaging-teams/{tenant}/oauth/{graph,management}/start
POST /v1/messaging/setup/messaging-teams/{tenant}/oauth/{graph,management}/complete
POST /v1/messaging/setup/messaging-teams/{tenant}/teams-app/publish
POST /v1/messaging/setup/messaging-teams/{tenant}/teams-app/install-me
GET  /v1/messaging/setup/messaging-teams/{tenant}/teams-app/package.zip
```

Move reusable setup artifacts into generated provider/pack ownership:

- Teams app manifest generation
- Teams app package generation
- Add to Teams link generation
- Open bot chat link generation
- admin action labels and next-step copy
- setup diagnostics returned as structured data

## Web Component Contract

The component must:

- accept an `api-base` attribute
- support configurable endpoint path attributes so it can call provider setup
  routes instead of tester-only `/api/*` routes
- avoid embedding secrets in static JavaScript
- support host-provided theme variables
- support partial i18n dictionaries
- continue emitting the events introduced in PR-01
- remain framework-independent

## Acceptance Criteria

- The `messaging-teams` pack source is generated from answer JSON.
- `messaging-teams/assets/setup` contains the setup component assets.
- Generated pack metadata exposes the setup asset location.
- The wizard can run against configured provider setup endpoints.
- The local tester can either use the source component or the pack asset.
- Admin portal embedding is documented with a minimal HTML example.
- All user-facing setup text can be overridden through i18n.
- `messaging-teams-graph` remains unchanged; it is the Graph API provider and
  must not receive the Bot Framework setup wizard changes.

## Tests

Minimum checks:

```text
node --check messaging-teams/assets/setup/greentic-teams-setup.js
jq -e '.pack_create and .pack' messaging-teams/build-answer.json
greentic-pack wizard apply --answers messaging-teams/build-answer.json
```

Add focused tests for:

- answer JSON includes setup assets
- generated pack metadata includes setup assets
- component route serves JavaScript with the expected content type
- component can call a non-tester `api-base`
- setup package generation includes the current bot app id

## Out Of Scope

- Replacing the Node Bot Framework sidecar.
- Porting Teams activity parsing into Rust/WASM.
- Removing tester-only diagnostic controls.
- Broad redesign of pack asset hosting.
- Hand-editing `packs/messaging-teams` as the source of truth.
- Changing `messaging-teams-graph`.

## Follow-Up

- PR-03: implement the Bot Framework-compatible Teams ingress in Rust/WASM.
- PR-04: add conformance coverage and retire tester-only happy-path dependencies.
