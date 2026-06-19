# Answer-Owned Providers

Answer-owned providers are new provider packages where `build-answer.json` is
the source of truth for generated pack inputs.

This workflow currently applies only to:

- `messaging-http`
- `messaging-websocket`

Existing providers remain on their current packaging flow until a separate
migration is designed and tested. Do not refactor Slack, Teams, Telegram,
WebChat, WebChat GUI, WebEx, Email, Microsoft Email, or WhatsApp as part of
the HTTP/WebSocket work.

## Why

New providers should follow the same generated-pack direction used by
`greentic-demo` and existing answer-owned flows such as Teams:

- keep source, assets, tests, and answers in one provider-owned directory
- generate pack YAML, JSON, CBOR, and GTPACK internals from answers
- avoid hand-editing generated pack artifacts
- keep existing providers stable while new providers use the newer workflow

## Layout

```text
generated-providers/
├── messaging-http/
│   ├── build-answer.json
│   ├── src/
│   ├── assets/
│   └── tests/
└── messaging-websocket/
    ├── build-answer.json
    ├── src/
    ├── assets/
    └── tests/
```

Generated pack inputs are materialized under:

```text
target/generated/<provider>.pack
```

Final package artifacts are written to:

```text
dist/packs/<provider>.gtpack
```

## Commands

```bash
scripts/generate_answer_provider.sh messaging-http
scripts/package_answer_provider.sh messaging-http
scripts/check_answer_provider.sh messaging-http

scripts/generate_answer_provider.sh messaging-websocket
scripts/package_answer_provider.sh messaging-websocket
scripts/check_answer_provider.sh messaging-websocket
```

The answer-owned scripts intentionally reject every other provider ID.

## Manual Edit Rule

For `messaging-http` and `messaging-websocket`, generated pack internals should
be reproducible from `build-answer.json`, source, and assets.

Do not manually maintain generated:

- `pack.yaml`
- `pack.manifest.json`
- `pack.lock.cbor`
- component manifests
- GTPACK archive internals

This rule does not apply retroactively to existing providers.
