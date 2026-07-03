from pathlib import Path
import importlib.util
import sys


MODULE_PATH = Path(__file__).resolve().parents[1] / "src" / "messaging_websocket_provider.py"
SPEC = importlib.util.spec_from_file_location("messaging_websocket_provider", MODULE_PATH)
websocket_provider = importlib.util.module_from_spec(SPEC)
assert SPEC and SPEC.loader
sys.modules[SPEC.name] = websocket_provider
SPEC.loader.exec_module(websocket_provider)


def test_inbound_text_json_frame_maps_to_envelope():
    frame = websocket_provider.WebSocketFrame(
        session_id="s-1",
        data='{"event":"created","case_id":"C123"}',
    )

    envelope = websocket_provider.inbound_frame_to_envelope(
        frame,
        {"tenant_id": "demo", "team_id": "default"},
    )

    assert envelope["provider"] == "messaging-websocket"
    assert envelope["kind"] == "websocket.frame"
    assert envelope["session_id"] == "s-1"
    assert envelope["payload"]["case_id"] == "C123"
    assert envelope["metadata"] == {"tenant_id": "demo", "team_id": "default"}


def test_outbound_message_maps_to_text_json_frame():
    frame = websocket_provider.outbound_message_to_frame(
        {"session_id": "s-1", "payload": {"text": "hello"}}
    )

    assert frame == {"session_id": "s-1", "opcode": "text", "data": '{"text":"hello"}'}


def test_query_token_auth_rejects_invalid_connection():
    frame = websocket_provider.WebSocketFrame(
        session_id="s-1",
        data="{}",
        query={"token": "wrong"},
    )

    try:
        websocket_provider.inbound_frame_to_envelope(
            frame,
            {"auth": {"type": "query_token", "value": "expected"}},
        )
    except websocket_provider.WebSocketProviderError as err:
        assert err.code == "unauthorized"
    else:
        raise AssertionError("expected unauthorized")


def test_lifecycle_event_normalization():
    event = websocket_provider.lifecycle_event("s-1", "open", {"ip": "127.0.0.1"})

    assert event["provider"] == "messaging-websocket"
    assert event["kind"] == "websocket.open"
    assert event["session_id"] == "s-1"
    assert event["payload"] == {"ip": "127.0.0.1"}


def test_inbound_bearer_auth_accepts_and_rejects():
    cfg = {"auth": {"type": "bearer", "token": "secret"}}
    ok = websocket_provider.inbound_frame_to_envelope(
        websocket_provider.WebSocketFrame("s-1", "{}", headers={"Authorization": "Bearer secret"}),
        cfg,
    )
    assert ok["provider"] == "messaging-websocket"

    try:
        websocket_provider.inbound_frame_to_envelope(
            websocket_provider.WebSocketFrame("s-1", "{}", headers={"Authorization": "Bearer nope"}),
            cfg,
        )
    except websocket_provider.WebSocketProviderError as err:
        assert err.code == "unauthorized"
    else:
        raise AssertionError("expected unauthorized")


def test_inbound_rejects_binary_frame():
    try:
        websocket_provider.inbound_frame_to_envelope(
            websocket_provider.WebSocketFrame("s-1", b"\x00\x01")
        )
    except websocket_provider.WebSocketProviderError as err:
        assert err.code == "unsupported_frame"
    else:
        raise AssertionError("expected unsupported_frame")


def test_inbound_rejects_malformed_json():
    try:
        websocket_provider.inbound_frame_to_envelope(
            websocket_provider.WebSocketFrame("s-1", "{")
        )
    except websocket_provider.WebSocketProviderError as err:
        assert err.code == "invalid_json"
    else:
        raise AssertionError("expected invalid_json")


def test_outbound_requires_session_id():
    try:
        websocket_provider.outbound_message_to_frame({"payload": {"x": 1}})
    except websocket_provider.WebSocketProviderError as err:
        assert err.code == "missing_session"
    else:
        raise AssertionError("expected missing_session")


def test_outbound_rejects_non_object_payload():
    try:
        websocket_provider.outbound_message_to_frame({"session_id": "s-1", "payload": "nope"})
    except websocket_provider.WebSocketProviderError as err:
        assert err.code == "invalid_payload"
    else:
        raise AssertionError("expected invalid_payload")


def test_lifecycle_rejects_unsupported_event():
    try:
        websocket_provider.lifecycle_event("s-1", "explode")
    except websocket_provider.WebSocketProviderError as err:
        assert err.code == "unsupported_lifecycle"
    else:
        raise AssertionError("expected unsupported_lifecycle")


if __name__ == "__main__":
    for name, value in sorted(globals().items()):
        if name.startswith("test_") and callable(value):
            value()
