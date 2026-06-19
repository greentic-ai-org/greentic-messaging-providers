from __future__ import annotations

import json
from dataclasses import dataclass
from typing import Any


class WebSocketProviderError(ValueError):
    def __init__(self, code: str, message: str):
        super().__init__(message)
        self.code = code


@dataclass(frozen=True)
class WebSocketFrame:
    session_id: str
    data: str | bytes
    headers: dict[str, str] | None = None
    query: dict[str, str] | None = None


def _lower_headers(headers: dict[str, str] | None) -> dict[str, str]:
    return {str(k).lower(): str(v) for k, v in (headers or {}).items()}


def validate_connection(headers: dict[str, str] | None, query: dict[str, str] | None, config: dict[str, Any] | None = None) -> None:
    config = config or {}
    auth = config.get("auth") or {"type": "none"}
    auth_type = auth.get("type", "none")
    lower_headers = _lower_headers(headers)
    query = query or {}

    if auth_type == "none":
        return
    if auth_type == "bearer":
        expected = auth.get("token")
        actual = lower_headers.get("authorization", "")
        if not expected or actual != f"Bearer {expected}":
            raise WebSocketProviderError("unauthorized", "missing or invalid bearer token")
        return
    if auth_type == "query_token":
        param = str(auth.get("param", "token"))
        if not auth.get("value") or query.get(param) != auth.get("value"):
            raise WebSocketProviderError("unauthorized", "missing or invalid query token")
        return
    raise WebSocketProviderError("unsupported_auth", f"unsupported auth type: {auth_type}")


def inbound_frame_to_envelope(frame: WebSocketFrame, config: dict[str, Any] | None = None) -> dict[str, Any]:
    config = config or {}
    validate_connection(frame.headers, frame.query, config)

    if isinstance(frame.data, bytes):
        raise WebSocketProviderError("unsupported_frame", "binary frames are deferred")

    try:
        payload = json.loads(frame.data)
    except json.JSONDecodeError as exc:
        raise WebSocketProviderError("invalid_json", "text frame must contain JSON") from exc
    if not isinstance(payload, dict):
        raise WebSocketProviderError("invalid_json", "text frame JSON must be an object")

    return {
        "provider": "messaging-websocket",
        "direction": "inbound",
        "kind": "websocket.frame",
        "session_id": frame.session_id,
        "payload": payload,
        "metadata": {
            "tenant_id": config.get("tenant_id"),
            "team_id": config.get("team_id"),
        },
    }


def outbound_message_to_frame(message: dict[str, Any]) -> dict[str, Any]:
    session_id = message.get("session_id")
    if not session_id:
        raise WebSocketProviderError("missing_session", "outbound WebSocket messages require session_id")
    payload = message.get("payload")
    if not isinstance(payload, dict):
        raise WebSocketProviderError("invalid_payload", "message payload must be an object")
    return {
        "session_id": session_id,
        "opcode": "text",
        "data": json.dumps(payload, separators=(",", ":")),
    }


def lifecycle_event(session_id: str, event: str, detail: dict[str, Any] | None = None) -> dict[str, Any]:
    if event not in {"open", "close", "error"}:
        raise WebSocketProviderError("unsupported_lifecycle", f"unsupported lifecycle event: {event}")
    return {
        "provider": "messaging-websocket",
        "direction": "inbound",
        "kind": f"websocket.{event}",
        "session_id": session_id,
        "payload": detail or {},
    }
