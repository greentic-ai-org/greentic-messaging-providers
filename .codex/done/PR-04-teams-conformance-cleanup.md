# PR-04: Teams Setup And Runtime Conformance

## Title

Add Teams setup/runtime conformance and retire tester-only happy paths

## Goal

Lock down the Teams setup and runtime behavior with conformance tests, then
simplify `scripts/test_teams_bot.sh` into a diagnostic wrapper around the real
provider and pack functionality.

This PR should land after the setup assets and Rust/WASM Bot Framework runtime
are in place.

## Context

The tester grew into an effective discovery tool, but it now contains behavior
that should live in the Teams provider, pack assets, or shared conformance
tests. Once PR-02 and PR-03 move those responsibilities, the tester should stop
being the only place where the full happy path works.

`messaging-teams` must continue following the generated-pack pattern:

- source files in `messaging-teams/src`
- static/setup assets in `messaging-teams/assets`
- pack generation driven by `messaging-teams/build-answer.json`
- generated pack/flow artifacts treated as outputs, not hand-edited source
- no changes to `messaging-teams-graph` for Bot Framework behavior

## Scope

Add conformance coverage for setup state transitions:

- fresh setup
- Graph device login start
- Graph device login pending
- Graph device login expired
- Graph device login refresh code
- Azure management token expired
- role assignment required
- Teams app publish
- Teams app install
- open bot chat
- first activity received
- send test card
- Adaptive Card action received

Add runtime conformance for:

- Bot Framework Teams activity ingestion
- authentication failures
- message normalization
- Adaptive Card submit normalization
- follow-up card response
- conversation context availability

Simplify the tester:

- keep one-button step execution
- keep raw diagnostic controls below the wizard
- remove duplicated setup logic once provider endpoints own it
- remove Node sidecar as the default success path after Rust/WASM is default
- keep optional sidecar diagnostics only when explicitly requested

## Acceptance Criteria

- The default Teams setup path uses provider/pack-owned code.
- The web component can drive setup without tester-only endpoints.
- The Bot Framework-compatible runtime is Rust/WASM by default.
- The `messaging-teams` pack is reproducible from answer JSON.
- The tester reports provider diagnostics instead of owning setup behavior.
- Device-code login refresh and timeout behavior has automated coverage.
- Adaptive Card submit behavior has automated coverage.
- Azure Bot Service is not required as the default runtime.

## Tests

Minimum local checks:

```text
cargo fmt
cargo test -p provider-tests teams
jq -e '.pack_create and .pack' messaging-teams/build-answer.json
greentic-pack wizard apply --answers messaging-teams/build-answer.json
bash -n scripts/test_teams_bot.sh
node --check messaging-teams/assets/setup/greentic-teams-setup.js
```

CI should include:

- provider instantiation checks
- Teams fixture conformance
- setup-plan fixture conformance
- pack asset validation
- web component syntax validation

## Out Of Scope

- New Teams product features beyond the setup/runtime path.
- Rewriting unrelated messaging providers.
- Changing the public setup UX established by PR-01 unless tests reveal a
  concrete issue.

## Completion Notes

After this PR, future Teams setup work should happen through provider APIs,
pack assets, and conformance fixtures first. `scripts/test_teams_bot.sh` should
remain useful for local debugging, but not be the source of truth for setup or
runtime behavior.
