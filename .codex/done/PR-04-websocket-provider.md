# PR-04 — Add `messaging-websocket` as an answer-owned provider

## Goal

Add a new WebSocket messaging provider using the answer-owned generation model from PR-01 and PR-02.

This provider must be implemented without refactoring existing providers.

## Provider Identity

Use stable IDs:

```text
provider id: messaging-websocket
pack id: messaging-websocket
component id: messaging-provider-websocket
artifact: dist/packs/messaging-websocket.gtpack
```

If implementation discovers an existing naming convention that requires a small adjustment, document it in the PR and keep names consistent across answer file, component, pack metadata, provider matrix, and tests.

## Source Layout

Place source under the new answer-owned provider area from PR-01, for example:

```text
generated-providers/messaging-websocket/
├── build-answer.json
├── build_pack.sh
├── src/
├── assets/
│   ├── setup.yaml
│   ├── schemas/
│   └── docs/
└── tests/
```

Do not add or modify `packs/messaging-websocket/pack.yaml` manually. If a generated `packs/messaging-websocket` directory is needed by the repo's package flow, it must be generated from `build-answer.json`.

## Functional Scope

Keep the first version focused and testable.

Support one of these modes initially, in this order of preference:

1. server mode: Greentic accepts WebSocket connections
2. client mode: Greentic connects to a configured remote endpoint

Only implement both modes in the first PR if the runtime/generator support is already present and tests remain small.

Required initial behavior:

- text JSON frames
- frame-to-message envelope mapping
- message-to-frame mapping
- connection/session ID metadata
- tenant/team scoping metadata
- open/close/error lifecycle events
- optional bearer token or query token validation

Defer binary frames, multiplexing, advanced reconnect orchestration, and protocol-specific subprotocols unless required by a fixture.

## Pack Metadata

The generated pack should declare capabilities such as:

```text
greentic.messaging.websocket.server.v1
greentic.messaging.websocket.client.v1
```

Only declare a capability that is actually implemented and tested in this first version.

The exact extension names should follow current repo conventions, but they must come from `build-answer.json`.

## Tests

Add focused tests for:

- answer file validation
- generated pack metadata contains expected WebSocket provider IDs/capabilities
- inbound text JSON frame maps to a messaging envelope
- outbound message maps to a text JSON frame
- auth rejection when configured
- open/close/error lifecycle event normalization
- generated pack builds and validates

## Acceptance Criteria

- `scripts/generate_answer_provider.sh messaging-websocket` succeeds.
- `scripts/package_answer_provider.sh messaging-websocket` succeeds.
- `dist/packs/messaging-websocket.gtpack` is produced by the answer-owned flow.
- No existing provider is moved, converted, or regenerated as part of this PR.
