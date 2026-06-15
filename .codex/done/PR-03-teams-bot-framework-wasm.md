# PR-03: Teams Bot Framework Rust/WASM Runtime

## Title

Port Teams Bot Framework ingress and Adaptive Card submit handling to Rust/WASM

## Goal

Replace the tester's Python/JavaScript Bot Framework runtime path with
provider-owned Rust/WASM behavior generated from the `messaging-teams` answer
JSON source package.

This PR is about the runtime. It assumes PR-01/PR-02 have established the setup
wizard and pack asset story.

## Context

Teams does not send Adaptive Card submit activity through Direct Line. A Teams
app bot must expose a Bot Framework-compatible endpoint that can receive Teams
activities, validate Bot Framework authentication, capture conversation context,
and send follow-up activities.

The local tester proved this path with a Node `botbuilder` sidecar. This PR
migrates that behavior into the Teams messaging provider.

## Scope

Implement the Teams Bot Framework-compatible ingress in Rust/WASM, aligned with
the existing provider boundaries and the answer-generated `messaging-teams`
pack.

Candidate areas:

```text
messaging-teams/src/
messaging-teams/assets/
messaging-teams/build-answer.json
specs/providers/teams.yaml
specs/teams-egress.yaml
crates/provider-tests/
```

Do not make hand-written generated pack files the source of truth. Component
source and reusable fixtures live under `messaging-teams/src` and
`messaging-teams/assets`; the pack is generated from `build-answer.json`.

Runtime behavior:

- receive Bot Framework `Activity` payloads from Teams
- validate Bot Framework bearer tokens at the provider boundary
- handle tenant-aware channel auth configuration
- parse message activities
- parse invoke activities
- capture `serviceUrl`, `conversation.id`, `from`, `recipient`, and tenant ids
- normalize Teams activities into Greentic messaging envelopes
- extract Adaptive Card submit values:
  - action id
  - input text
  - raw submit payload
- send a follow-up Adaptive Card showing the clicked button and entered text
- return the response shape Teams expects for invoke/submit activities

## Boundaries

Keep secrets and tenant-specific values out of static assets.

Do not make Azure Bot Service the required runtime path. The provider should be
able to expose the Bot Framework-compatible endpoint needed by Teams while using
Greentic-owned setup and runtime infrastructure.

Conversation state should be abstracted behind existing provider state or test
fixtures. Avoid baking tester-only storage paths into the component.

Do not modify `messaging-teams-graph` for this runtime. That pack remains the
Microsoft Graph Teams provider.

## Acceptance Criteria

- Teams messages reach the Rust/WASM ingress component.
- Adaptive Card button clicks no longer depend on the Node sidecar.
- Button click follow-up cards show the selected action and text input.
- Positive and destructive card actions are preserved in the rendered payload.
- The provider exposes enough conversation context to send test cards.
- Existing Teams provider conformance still passes or is updated intentionally.
- The generated `messaging-teams` pack contains the Rust/WASM runtime built from
  `messaging-teams/src`.

## Tests

Add fixtures for:

- Teams message activity
- Adaptive Card `Action.Submit`
- Adaptive Card `Action.Execute`, if supported by the target Teams client
- invoke response generation
- invalid/missing Bot Framework auth token
- tenant/channel auth mismatch

Add provider tests for:

- activity normalization
- submit payload extraction
- follow-up card generation
- conversation context persistence
- component instantiation

Minimum local checks:

```text
cargo fmt
cargo test -p provider-tests teams
cargo component build
jq -e '.pack_create and .pack' messaging-teams/build-answer.json
```

## Out Of Scope

- Web component packaging.
- Admin portal embedding.
- Removing all tester diagnostics.
- Supporting every Bot Framework channel beyond Teams.

## Follow-Up

- PR-04: add end-to-end conformance and simplify the tester after the Rust/WASM
  runtime is the default path.
