# Messaging HTTP

`messaging-http` is an answer-owned provider for HTTP webhook ingestion and
simple outbound HTTP callbacks.

This answer-owned workflow applies to the new HTTP and WebSocket providers.
Existing providers remain on their current packaging flow until a separate
migration is designed and tested.

## First-Version Scope

Inbound:

- JSON `POST` webhook ingestion
- query and allowlisted header capture
- route/path metadata
- idempotency key extraction from headers or payload
- malformed JSON rejection
- optional bearer token or API key header validation

Outbound:

- `GET` and `POST`
- URL templates using `{payload.field}` placeholders
- static headers
- JSON request body for `POST`
- timeout metadata
- structured request output

Deferred:

- OAuth
- multipart requests
- streaming bodies
- broad HTTP method support
- advanced retry orchestration

## Inbound Example

```python
from messaging_http_provider import HttpRequest, inbound_http_request_to_envelope

request = HttpRequest(
    method="POST",
    path="/webhooks/acme",
    headers={"X-Request-Id": "req-1"},
    query={"source": "acme"},
    body='{"case_id":"C123","event":"case.created"}',
)

envelope = inbound_http_request_to_envelope(
    request,
    {"capture_headers": ["x-request-id"]},
)
```

The resulting envelope contains:

```json
{
  "provider": "messaging-http",
  "direction": "inbound",
  "kind": "http.webhook",
  "idempotency_key": "req-1"
}
```

## Outbound Example

```python
from messaging_http_provider import outbound_message_to_request

request = outbound_message_to_request(
    {"payload": {"case_id": "C123", "status": "open"}},
    {
        "method": "POST",
        "url": "https://example.test/cases/{payload.case_id}",
        "headers": {"Content-Type": "application/json"},
        "timeout_ms": 5000,
    },
)
```

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

API key header:

```json
{
  "auth": {
    "type": "api_key_header",
    "header": "x-api-key",
    "value": "secret"
  }
}
```

## Commands

```bash
scripts/generate_answer_provider.sh messaging-http
scripts/package_answer_provider.sh messaging-http
scripts/check_answer_provider.sh messaging-http
```
