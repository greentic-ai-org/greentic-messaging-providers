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


if __name__ == "__main__":
    for name, value in sorted(globals().items()):
        if name.startswith("test_") and callable(value):
            value()
