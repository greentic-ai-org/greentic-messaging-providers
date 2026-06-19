from __future__ import annotations

import json
import re
from dataclasses import dataclass
from typing import Any


class HttpProviderError(ValueError):
    def __init__(self, code: str, message: str):
        super().__init__(message)
        self.code = code


@dataclass(frozen=True)
class HttpRequest:
    method: str
    path: str
    headers: dict[str, str]
    query: dict[str, str]
    body: str | bytes | None


def _lower_headers(headers: dict[str, str] | None) -> dict[str, str]:
    return {str(k).lower(): str(v) for k, v in (headers or {}).items()}


def _require_auth(headers: dict[str, str], config: dict[str, Any]) -> None:
    auth = config.get("auth") or {"type": "none"}
    auth_type = auth.get("type", "none")
    if auth_type == "none":
        return
    if auth_type == "bearer":
        expected = auth.get("token")
        actual = headers.get("authorization", "")
        if not expected or actual != f"Bearer {expected}":
            raise HttpProviderError("unauthorized", "missing or invalid bearer token")
        return
    if auth_type == "api_key_header":
        header = str(auth.get("header", "x-api-key")).lower()
        if not auth.get("value") or headers.get(header) != auth.get("value"):
            raise HttpProviderError("unauthorized", "missing or invalid API key")
        return
    raise HttpProviderError("unsupported_auth", f"unsupported auth type: {auth_type}")


def inbound_http_request_to_envelope(request: HttpRequest, config: dict[str, Any] | None = None) -> dict[str, Any]:
    config = config or {}
    method = request.method.upper()
    if method != "POST":
        raise HttpProviderError("unsupported_method", "messaging-http initially supports POST inbound requests")

    headers = _lower_headers(request.headers)
    _require_auth(headers, config)

    try:
        raw_body = request.body.decode("utf-8") if isinstance(request.body, bytes) else (request.body or "{}")
        payload = json.loads(raw_body)
    except json.JSONDecodeError as exc:
        raise HttpProviderError("invalid_json", "request body must be valid JSON") from exc
    if not isinstance(payload, dict):
        raise HttpProviderError("invalid_json", "request JSON body must be an object")

    header_allowlist = {str(item).lower() for item in config.get("capture_headers", [])}
    captured_headers = {key: value for key, value in headers.items() if key in header_allowlist}
    idempotency_key = (
        headers.get("idempotency-key")
        or headers.get("x-request-id")
        or str(payload.get("idempotency_key") or payload.get("id") or "")
    )

    return {
        "provider": "messaging-http",
        "direction": "inbound",
        "kind": "http.webhook",
        "idempotency_key": idempotency_key or None,
        "source": {
            "method": method,
            "path": request.path,
            "headers": captured_headers,
            "query": request.query,
        },
        "payload": payload,
    }


_TEMPLATE_PATTERN = re.compile(r"\{payload\.([A-Za-z0-9_.-]+)\}")


def _lookup_payload(payload: dict[str, Any], dotted: str) -> Any:
    current: Any = payload
    for part in dotted.split("."):
        if not isinstance(current, dict) or part not in current:
            raise HttpProviderError("missing_template_value", f"missing payload value: {dotted}")
        current = current[part]
    return current


def render_url_template(template: str, payload: dict[str, Any]) -> str:
    def replace(match: re.Match[str]) -> str:
        return str(_lookup_payload(payload, match.group(1)))

    return _TEMPLATE_PATTERN.sub(replace, template)


def outbound_message_to_request(message: dict[str, Any], config: dict[str, Any]) -> dict[str, Any]:
    method = str(config.get("method", "POST")).upper()
    if method not in {"GET", "POST"}:
        raise HttpProviderError("unsupported_method", "messaging-http initially supports GET and POST outbound requests")

    payload = message.get("payload") or {}
    if not isinstance(payload, dict):
        raise HttpProviderError("invalid_payload", "message payload must be an object")

    url_template = config.get("url")
    if not url_template:
        raise HttpProviderError("missing_url", "outbound config requires url")

    headers = {str(k).lower(): str(v) for k, v in (config.get("headers") or {}).items()}
    body = None if method == "GET" else json.dumps(payload, separators=(",", ":"))
    return {
        "method": method,
        "url": render_url_template(str(url_template), payload),
        "headers": headers,
        "body": body,
        "timeout_ms": int(config.get("timeout_ms", 10000)),
    }
