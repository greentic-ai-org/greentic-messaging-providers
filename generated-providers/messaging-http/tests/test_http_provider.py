from pathlib import Path
import importlib.util
import json
import sys


MODULE_PATH = Path(__file__).resolve().parents[1] / "src" / "messaging_http_provider.py"
SPEC = importlib.util.spec_from_file_location("messaging_http_provider", MODULE_PATH)
http_provider = importlib.util.module_from_spec(SPEC)
assert SPEC and SPEC.loader
sys.modules[SPEC.name] = http_provider
SPEC.loader.exec_module(http_provider)


def test_inbound_json_post_maps_to_envelope():
    request = http_provider.HttpRequest(
        method="POST",
        path="/webhooks/acme",
        headers={"Content-Type": "application/json", "X-Request-Id": "req-1"},
        query={"source": "test"},
        body=json.dumps({"case_id": "C123", "event": "case.created"}),
    )

    envelope = http_provider.inbound_http_request_to_envelope(
        request,
        {"capture_headers": ["x-request-id"]},
    )

    assert envelope["provider"] == "messaging-http"
    assert envelope["direction"] == "inbound"
    assert envelope["idempotency_key"] == "req-1"
    assert envelope["source"]["headers"] == {"x-request-id": "req-1"}
    assert envelope["payload"]["case_id"] == "C123"


def test_inbound_rejects_malformed_json():
    request = http_provider.HttpRequest("POST", "/bad", {}, {}, "{")

    try:
        http_provider.inbound_http_request_to_envelope(request)
    except http_provider.HttpProviderError as err:
        assert err.code == "invalid_json"
    else:
        raise AssertionError("expected invalid_json")


def test_inbound_rejects_invalid_bearer_token():
    request = http_provider.HttpRequest("POST", "/secure", {}, {}, "{}")

    try:
        http_provider.inbound_http_request_to_envelope(
            request,
            {"auth": {"type": "bearer", "token": "secret"}},
        )
    except http_provider.HttpProviderError as err:
        assert err.code == "unauthorized"
    else:
        raise AssertionError("expected unauthorized")


def test_outbound_message_maps_to_http_request():
    request = http_provider.outbound_message_to_request(
        {"payload": {"case_id": "C123", "status": "open"}},
        {
            "method": "POST",
            "url": "https://example.test/cases/{payload.case_id}",
            "headers": {"Content-Type": "application/json"},
            "timeout_ms": 5000,
        },
    )

    assert request["method"] == "POST"
    assert request["url"] == "https://example.test/cases/C123"
    assert request["headers"] == {"content-type": "application/json"}
    assert request["body"] == '{"case_id":"C123","status":"open"}'
    assert request["timeout_ms"] == 5000


if __name__ == "__main__":
    for name, value in sorted(globals().items()):
        if name.startswith("test_") and callable(value):
            value()
