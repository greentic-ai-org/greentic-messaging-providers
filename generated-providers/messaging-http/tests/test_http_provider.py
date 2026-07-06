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


def test_inbound_api_key_header_auth_accepts_and_rejects():
    cfg = {"auth": {"type": "api_key_header", "header": "X-API-Key", "value": "k-123"}}
    ok = http_provider.inbound_http_request_to_envelope(
        http_provider.HttpRequest("POST", "/x", {"X-API-Key": "k-123"}, {}, "{}"), cfg
    )
    assert ok["provider"] == "messaging-http"

    try:
        http_provider.inbound_http_request_to_envelope(
            http_provider.HttpRequest("POST", "/x", {"X-API-Key": "nope"}, {}, "{}"), cfg
        )
    except http_provider.HttpProviderError as err:
        assert err.code == "unauthorized"
    else:
        raise AssertionError("expected unauthorized")


def test_inbound_rejects_non_post():
    try:
        http_provider.inbound_http_request_to_envelope(
            http_provider.HttpRequest("GET", "/x", {}, {}, None)
        )
    except http_provider.HttpProviderError as err:
        assert err.code == "unsupported_method"
    else:
        raise AssertionError("expected unsupported_method")


def test_inbound_idempotency_falls_back_to_payload_id():
    envelope = http_provider.inbound_http_request_to_envelope(
        http_provider.HttpRequest("POST", "/x", {}, {}, json.dumps({"id": "evt-9"}))
    )
    assert envelope["idempotency_key"] == "evt-9"


def test_outbound_get_omits_body():
    request = http_provider.outbound_message_to_request(
        {"payload": {"q": "1"}}, {"method": "GET", "url": "https://example.test/ping"}
    )
    assert request["method"] == "GET"
    assert request["body"] is None


def test_outbound_requires_url():
    try:
        http_provider.outbound_message_to_request({"payload": {}}, {"method": "POST"})
    except http_provider.HttpProviderError as err:
        assert err.code == "missing_url"
    else:
        raise AssertionError("expected missing_url")


def test_outbound_template_missing_value_errors():
    try:
        http_provider.outbound_message_to_request(
            {"payload": {"a": "1"}},
            {"method": "POST", "url": "https://example.test/{payload.missing}"},
        )
    except http_provider.HttpProviderError as err:
        assert err.code == "missing_template_value"
    else:
        raise AssertionError("expected missing_template_value")


if __name__ == "__main__":
    for name, value in sorted(globals().items()):
        if name.startswith("test_") and callable(value):
            value()
