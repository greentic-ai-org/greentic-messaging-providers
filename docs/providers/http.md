# HTTP Provider

## What It Does

`messaging-http` is a generic HTTP messaging provider: it turns inbound JSON
`POST` webhooks into Greentic envelopes and turns outbound messages into HTTP
requests. It is an answer-owned provider (generated from `build-answer.json`).

## Features

- Inbound JSON `POST` webhook → envelope mapping (`kind: http.webhook`).
- Optional auth: `bearer` token or `api_key_header` validation.
- Outbound `GET`/`POST` request mapping with `{payload.field}` URL templates.
- Header capture allowlist and idempotency key extraction
  (`Idempotency-Key` / `X-Request-Id` / `payload.id`).

## Setup Inputs

- Outbound `url` template (e.g. `https://api.example.com/cases/{payload.case_id}`).
- Outbound `method` (`GET` or `POST`), `headers`, and `timeout_ms`.
- Inbound `auth` (`none` | `bearer` | `api_key_header`) and `capture_headers` allowlist.

## Capabilities

| Capability | Direction |
| --- | --- |
| `greentic.messaging.http.inbound.webhook.v1` | inbound |
| `greentic.messaging.http.outbound.request.v1` | outbound |

## Message Features

- Outbound: HTTP request (`GET`/`POST`) with JSON body + URL templating.
- Inbound: JSON `POST` webhook normalized to an envelope.
- Auth: bearer token or API-key header.

## Owned Files

- `generated-providers/messaging-http/build-answer.json` — source of truth.
- `generated-providers/messaging-http/src/messaging_http_provider.py` — implementation.
- `generated-providers/messaging-http/assets/schemas/` — inbound/outbound schemas.
- `generated-providers/messaging-http/tests/test_http_provider.py` — conformance tests.
- `e2e/providers/messaging-http/payload.json` — e2e smoke fixture.

## Testing

```bash
scripts/test_http.sh                 # interactive tester UI
scripts/check_answer_provider.sh messaging-http   # run conformance + rebuild pack
```
