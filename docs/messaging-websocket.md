# Messaging WebSocket

`messaging-websocket` is an answer-owned provider for text JSON WebSocket frame
mapping and connection lifecycle events.

This answer-owned workflow applies to the new HTTP and WebSocket providers.
Existing providers remain on their current packaging flow until a separate
migration is designed and tested.

## First-Version Scope

Implemented first:

- server-side text JSON frame mapping
- outbound text JSON frame mapping
- connection/session ID metadata
- tenant/team metadata
- open, close, and error lifecycle event normalization
- optional bearer token or query token validation

Deferred:

- binary frames
- multiplexing
- advanced reconnect orchestration
- protocol-specific subprotocols

## Inbound Frame Example

```python
from messaging_websocket_provider import WebSocketFrame, inbound_frame_to_envelope

frame = WebSocketFrame(
    session_id="s-1",
    data='{"event":"created","case_id":"C123"}',
)

envelope = inbound_frame_to_envelope(
    frame,
    {"tenant_id": "demo", "team_id": "default"},
)
```

The resulting envelope contains:

```json
{
  "provider": "messaging-websocket",
  "direction": "inbound",
  "kind": "websocket.frame",
  "session_id": "s-1"
}
```

## Outbound Frame Example

```python
from messaging_websocket_provider import outbound_message_to_frame

frame = outbound_message_to_frame(
    {"session_id": "s-1", "payload": {"text": "hello"}}
)
```

The frame payload is emitted as compact JSON text.

## Lifecycle Example

```python
from messaging_websocket_provider import lifecycle_event

event = lifecycle_event("s-1", "open", {"ip": "127.0.0.1"})
```

Supported lifecycle events:

- `open`
- `close`
- `error`

## Auth Examples

Bearer token:

```json
{
  "auth": {
    "type": "bearer",
    "token": "secret"
  }
}
```

Query token:

```json
{
  "auth": {
    "type": "query_token",
    "param": "token",
    "value": "secret"
  }
}
```

## Commands

```bash
scripts/generate_answer_provider.sh messaging-websocket
scripts/package_answer_provider.sh messaging-websocket
scripts/check_answer_provider.sh messaging-websocket
```
