# WebSocket Provider

## What It Does

`messaging-websocket` is a generic WebSocket messaging provider: it turns inbound
text JSON frames into Greentic envelopes, turns outbound messages into text
frames, and normalizes connection lifecycle events. It is an answer-owned
provider (generated from `build-answer.json`).

## Features

- Inbound text JSON frame → envelope mapping (`kind: websocket.frame`), keyed by `session_id`.
- Optional auth: `bearer` token or `query_token` validation at connect.
- Outbound message → text frame (`opcode: text`).
- Lifecycle events: `open` / `close` / `error` → `websocket.{event}` envelopes.
- Binary frames are deferred (rejected with `unsupported_frame`).

## Setup Inputs

- Inbound `auth` (`none` | `bearer` | `query_token`).
- `tenant_id` / `team_id` metadata attached to inbound envelopes.

## Capabilities

| Capability | Direction |
| --- | --- |
| `greentic.messaging.websocket.inbound.frame.v1` | inbound |
| `greentic.messaging.websocket.outbound.frame.v1` | outbound |

## Message Features

- Outbound: text JSON frame for a given `session_id`.
- Inbound: text JSON frame normalized to an envelope.
- Lifecycle: connection `open`/`close`/`error` events.
- Auth: bearer token or query-param token at connect.

## Owned Files

- `generated-providers/messaging-websocket/build-answer.json` — source of truth.
- `generated-providers/messaging-websocket/src/messaging_websocket_provider.py` — implementation.
- `generated-providers/messaging-websocket/assets/schemas/` — inbound/outbound frame schemas.
- `generated-providers/messaging-websocket/tests/test_websocket_provider.py` — conformance tests.
- `e2e/providers/messaging-websocket/payload.json` — e2e smoke fixture.

## Testing

```bash
scripts/test_ws.sh                   # interactive tester UI
scripts/check_answer_provider.sh messaging-websocket   # run conformance + rebuild pack
```
