# PR-06 — Document the answer-owned HTTP and WebSocket providers

## Goal

Document only the two new answer-owned providers and the limited new-provider workflow.

The docs must make clear that existing providers are intentionally not being refactored or converted.

## Documentation Scope

Add or update docs for:

```text
docs/answer-owned-providers.md
docs/messaging-http.md
docs/messaging-websocket.md
```

Use different paths if the repo already has a docs convention, but keep the scope focused on HTTP/WebSocket.

## Required Content

### `docs/answer-owned-providers.md`

Explain:

- new providers should use `build-answer.json` as source of truth
- generated YAML/JSON/CBOR/pack internals should not be manually edited
- `messaging-http` and `messaging-websocket` are the first providers using this forward-looking model
- existing providers keep their current build/package flows
- the pattern is inspired by `greentic-demo` and by existing answer-owned flows such as Teams

Include commands:

```bash
scripts/generate_answer_provider.sh messaging-http
scripts/package_answer_provider.sh messaging-http
scripts/generate_answer_provider.sh messaging-websocket
scripts/package_answer_provider.sh messaging-websocket
scripts/check_answer_provider.sh messaging-http
scripts/check_answer_provider.sh messaging-websocket
```

### `docs/messaging-http.md`

Include:

- provider purpose and supported first-version scope
- inbound webhook example
- outbound request example
- auth configuration examples
- answer-owned generation notes
- testing command

### `docs/messaging-websocket.md`

Include:

- provider purpose and supported first-version scope
- server/client mode status
- text JSON frame example
- lifecycle event examples
- auth configuration examples
- answer-owned generation notes
- testing command

## Explicit Existing Provider Note

Each doc should include a short note:

```text
This answer-owned workflow applies to the new HTTP and WebSocket providers. Existing providers remain on their current packaging flow until a separate migration is designed and tested.
```

## Acceptance Criteria

- Docs explain the new-provider-only answer-owned workflow.
- Docs include HTTP and WebSocket examples.
- Docs include generation/package/check commands.
- Docs explicitly say existing providers are not being refactored in this work.
