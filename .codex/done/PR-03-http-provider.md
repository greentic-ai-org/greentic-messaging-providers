# PR-03 — Add `messaging-http` as an answer-owned provider

## Goal

Add a new HTTP messaging provider using the answer-owned generation model from PR-01 and PR-02.

This provider must be implemented without refactoring existing providers.

## Provider Identity

Use stable IDs:

```text
provider id: messaging-http
pack id: messaging-http
component id: messaging-provider-http
artifact: dist/packs/messaging-http.gtpack
```

If implementation discovers an existing naming convention that requires a small adjustment, document it in the PR and keep names consistent across answer file, component, pack metadata, provider matrix, and tests.

## Source Layout

Place source under the new answer-owned provider area from PR-01, for example:

```text
generated-providers/messaging-http/
├── build-answer.json
├── build_pack.sh
├── src/
├── assets/
│   ├── setup.yaml
│   ├── schemas/
│   └── docs/
└── tests/
```

Do not add or modify `packs/messaging-http/pack.yaml` manually. If a generated `packs/messaging-http` directory is needed by the repo's package flow, it must be generated from `build-answer.json`.

## Functional Scope

Keep the first version focused and testable:

### Inbound

Support receiving HTTP webhook requests and converting them into Greentic messaging envelopes.

Required initial behavior:

- `POST` JSON body ingestion
- query/header capture with a conservative allowlist
- route/path metadata
- idempotency key extraction from header or payload
- structured rejection for malformed JSON
- optional bearer token or API key header validation

### Outbound

Support sending HTTP requests from Greentic message data.

Required initial behavior:

- methods: `GET` and `POST`
- URL template from config plus message payload values
- configurable static headers
- JSON body
- timeout configuration
- structured response metadata

Defer advanced retry orchestration, OAuth, multipart, streaming bodies, and broad method support unless required by a fixture.

## Pack Metadata

The generated pack should declare capabilities such as:

```text
greentic.messaging.http.inbound.webhook.v1
greentic.messaging.http.outbound.request.v1
```

The exact extension names should follow current repo conventions, but they must come from `build-answer.json`.

## Tests

Add focused tests for:

- answer file validation
- generated pack metadata contains expected HTTP provider IDs/capabilities
- inbound JSON POST maps to a messaging envelope
- malformed JSON is rejected predictably
- missing/invalid auth is rejected when configured
- outbound request mapping creates the expected method, URL, headers, and JSON body
- generated pack builds and validates

## Acceptance Criteria

- `scripts/generate_answer_provider.sh messaging-http` succeeds.
- `scripts/package_answer_provider.sh messaging-http` succeeds.
- `dist/packs/messaging-http.gtpack` is produced by the answer-owned flow.
- No existing provider is moved, converted, or regenerated as part of this PR.
