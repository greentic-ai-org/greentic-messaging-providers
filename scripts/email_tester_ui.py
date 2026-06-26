#!/usr/bin/env python3
from __future__ import annotations

import html
import json
import os
import subprocess
import time
import webbrowser
from base64 import b64encode
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
from pathlib import Path
from urllib.parse import parse_qs, urlparse


ROOT = Path(os.environ["GREENTIC_ROOT"]).resolve()
TESTER = Path(os.environ["GREENTIC_TESTER_BIN"]).resolve()
PROVIDER = os.environ.get("GREENTIC_EMAIL_TEST_PROVIDER", "email")
TITLE = os.environ.get("GREENTIC_EMAIL_TEST_TITLE", "Email")
PORT = int(os.environ.get("GREENTIC_EMAIL_TEST_PORT", "8795"))
OPEN_BROWSER = os.environ.get("GREENTIC_EMAIL_TEST_OPEN", "1") != "0"
WORK = Path(os.environ.get("TMPDIR", "/tmp")) / f"greentic-{PROVIDER}-test-{PORT}"
VALUES = WORK / "values.json"
EVENTS = WORK / "events.jsonl"


def now() -> str:
    return time.strftime("%Y-%m-%dT%H:%M:%SZ", time.gmtime())


def append_event(kind: str, payload: dict) -> None:
    EVENTS.parent.mkdir(parents=True, exist_ok=True)
    event = {"ts": now(), "kind": kind, **payload}
    with EVENTS.open("a", encoding="utf-8") as handle:
        handle.write(json.dumps(event, ensure_ascii=False) + "\n")


def read_events() -> list[dict]:
    if not EVENTS.exists():
        return []
    out: list[dict] = []
    for line in EVENTS.read_text(encoding="utf-8").splitlines()[-80:]:
        try:
            out.append(json.loads(line))
        except json.JSONDecodeError:
            pass
    return out


def read_values() -> dict:
    try:
        return json.loads(VALUES.read_text(encoding="utf-8"))
    except (FileNotFoundError, json.JSONDecodeError):
        return default_values()


def default_values() -> dict:
    if PROVIDER == "microsoft-email":
        return {
            "config": {
                "from_address": "",
                "graph_tenant_id": "",
                "ms_graph_client_id": "",
                "graph_scope": "https://graph.microsoft.com/.default offline_access openid",
            },
            "secrets": {
                "FROM_ADDRESS": "",
                "GRAPH_TENANT_ID": "",
                "MS_GRAPH_CLIENT_ID": "",
                "MS_GRAPH_REFRESH_TOKEN": "",
                "MS_GRAPH_CLIENT_SECRET": "",
            },
            "http": "mock",
            "responses": mock_graph_responses(),
            "state": {},
        }
    return {
        "config": {
            "host": "",
            "port": 587,
            "username": "",
            "from_address": "",
            "tls_mode": "starttls",
        },
        "secrets": {
            "EMAIL_PASSWORD": "",
            "FROM_ADDRESS": "",
            "GRAPH_TENANT_ID": "mock-tenant",
            "MS_GRAPH_CLIENT_ID": "mock-client",
            "MS_GRAPH_REFRESH_TOKEN": "mock-refresh-token",
        },
        "http": "mock",
        "responses": mock_graph_responses(),
        "state": {},
    }


def mock_graph_responses() -> list[dict]:
    return [
        {
            "url": "/oauth2/v2.0/token",
            "status": 200,
            "body": {"access_token": "mock-access-token", "token_type": "Bearer", "expires_in": 3600},
        },
        {
            "url": "/messages/",
            "status": 200,
            "body": {
                "id": "msg-1",
                "subject": "Inbound email test",
                "bodyPreview": "Hello from inbound email test.",
                "receivedDateTime": "2026-06-19T15:00:00Z",
                "from": {"emailAddress": {"address": "sender@example.com"}},
                "toRecipients": [{"emailAddress": {"address": "worker@example.com"}}],
                "webLink": "https://example.test/messages/msg-1",
                "internetMessageId": "<msg-1@example.test>",
            },
        },
        {"url": "/sendMail", "status": 202, "body": {}},
    ]


def write_values(values: dict) -> None:
    WORK.mkdir(parents=True, exist_ok=True)
    VALUES.write_text(json.dumps(values, indent=2, sort_keys=True) + "\n", encoding="utf-8")


def merged_values(form: dict[str, str]) -> dict:
    values = read_values()
    http_mode = form.get("http", "mock")
    if PROVIDER == "microsoft-email":
        from_address = form.get("from_address", "")
        tenant = form.get("graph_tenant_id", "")
        client_id = form.get("ms_graph_client_id", "")
        values["config"] = {
            "from_address": from_address,
            "graph_tenant_id": tenant,
            "ms_graph_client_id": client_id,
            "graph_scope": form.get("graph_scope", ""),
        }
        values["secrets"] = {
            "FROM_ADDRESS": from_address,
            "GRAPH_TENANT_ID": tenant,
            "MS_GRAPH_CLIENT_ID": client_id,
            "MS_GRAPH_REFRESH_TOKEN": form.get("ms_graph_refresh_token", ""),
            "MS_GRAPH_CLIENT_SECRET": form.get("ms_graph_client_secret", ""),
        }
    else:
        from_address = form.get("from_address", "")
        values["config"] = {
            "host": form.get("host", ""),
            "port": int(form.get("port", "587") or "587"),
            "username": form.get("username", ""),
            "from_address": from_address,
            "tls_mode": form.get("tls_mode", "starttls"),
            "password": form.get("password", ""),
        }
        values["secrets"] = {
            "EMAIL_PASSWORD": form.get("password", ""),
            "FROM_ADDRESS": from_address,
            "GRAPH_TENANT_ID": "mock-tenant",
            "MS_GRAPH_CLIENT_ID": "mock-client",
            "MS_GRAPH_REFRESH_TOKEN": "mock-refresh-token",
        }
    values["http"] = http_mode
    values["responses"] = mock_graph_responses() if http_mode == "mock" else []
    values["state"] = values.get("state") or {}
    return values


def parse_json_from_stdout(stdout: str):
    text = stdout.strip()
    if not text:
        return None
    try:
        return json.loads(text)
    except json.JSONDecodeError:
        pass
    start = text.find("{")
    while start >= 0:
        try:
            return json.loads(text[start:])
        except json.JSONDecodeError:
            start = text.find("{", start + 1)
    return None


def run_send(to: str, subject: str, body: str) -> dict:
    metadata_values = read_values()
    metadata_values["to"] = {"subject": subject}
    write_values(metadata_values)
    if PROVIDER == "email":
        input_path = WORK / f"email-send-{int(time.time() * 1000)}.json"
        input_path.write_text(
            json.dumps(
                {
                    "config": metadata_values.get("config") or {},
                    "id": f"email-test-{int(time.time() * 1000)}",
                    "tenant": {
                        "attempt": 0,
                        "env": "default",
                        "tenant": "default",
                        "tenant_id": "default",
                    },
                    "channel": "messaging.email.smtp",
                    "session_id": "manual-test",
                    "reply_scope": None,
                    "from": None,
                    "to": [{"id": to, "kind": "email"}],
                    "correlation_id": None,
                    "text": body,
                    "attachments": [],
                    "metadata": {"subject": subject},
                    "extensions": {},
                },
                indent=2,
                sort_keys=True,
            )
            + "\n",
            encoding="utf-8",
        )
        proc = subprocess.run(
            [
                str(TESTER),
                "invoke",
                "--provider",
                "email",
                "--op",
                "send",
                "--input",
                str(input_path),
                "--values",
                str(VALUES),
            ],
            cwd=str(ROOT),
            text=True,
            capture_output=True,
            timeout=120,
        )
        return {
            "ok": proc.returncode == 0,
            "status": proc.returncode,
            "stdout": proc.stdout,
            "stderr": proc.stderr,
            "json": parse_json_from_stdout(proc.stdout),
        }
    env = os.environ.copy()
    proc = subprocess.run(
        [
            str(TESTER),
            "send",
            "--provider",
            PROVIDER,
            "--values",
            str(VALUES),
            "--text",
            body,
            "--to",
            to,
            "--to-kind",
            "email",
        ],
        cwd=str(ROOT),
        env=env,
        text=True,
        capture_output=True,
        timeout=120,
    )
    return {
        "ok": proc.returncode == 0,
        "status": proc.returncode,
        "stdout": proc.stdout,
        "stderr": proc.stderr,
        "json": parse_json_from_stdout(proc.stdout),
    }


def run_inbound(from_address: str) -> dict:
    values = read_values()
    config = dict(values.get("config") or {})
    if not config.get("from_address"):
        config["from_address"] = from_address or "worker@example.com"
    if not config.get("host"):
        config["host"] = "smtp.example.test"
    if not config.get("port"):
        config["port"] = 587
    if not config.get("username"):
        config["username"] = config["from_address"]
    if not config.get("graph_tenant_id"):
        config["graph_tenant_id"] = "mock-tenant"
    values["config"] = config
    values.setdefault("secrets", {})
    values["secrets"].setdefault("MS_GRAPH_CLIENT_ID", "mock-client")
    values["secrets"].setdefault("MS_GRAPH_REFRESH_TOKEN", "mock-refresh-token")
    values["responses"] = mock_graph_responses()
    values["http"] = "mock"
    write_values(values)
    notification = {
        "value": [
            {
                "resource": "me/messages/msg-1",
                "resourceData": {"id": "msg-1"},
            }
        ]
    }
    input_path = WORK / f"email-inbound-{int(time.time() * 1000)}.json"
    input_path.write_text(
        json.dumps(
            {
                "method": "POST",
                "path": "/webhook/email",
                "query": None,
                "headers": [],
                "body_b64": b64encode(json.dumps(notification).encode("utf-8")).decode("ascii"),
                "config": config,
                "binding_id": "manual-user|MS_GRAPH_REFRESH_TOKEN",
                "route_hint": None,
            },
            indent=2,
            sort_keys=True,
        )
        + "\n",
        encoding="utf-8",
    )
    env = os.environ.copy()
    proc = subprocess.run(
        [
            str(TESTER),
            "invoke",
            "--provider",
            "email" if PROVIDER == "email" else PROVIDER,
            "--op",
            "ingest_http",
            "--input",
            str(input_path),
            "--values",
            str(VALUES),
        ],
        cwd=str(ROOT),
        env=env,
        text=True,
        capture_output=True,
        timeout=120,
    )
    return {
        "ok": proc.returncode == 0,
        "status": proc.returncode,
        "stdout": proc.stdout,
        "stderr": proc.stderr,
        "json": parse_json_from_stdout(proc.stdout),
    }


def esc(value) -> str:
    return html.escape(str(value or ""), quote=True)


def form_value(values: dict, key: str) -> str:
    return values.get("config", {}).get(key) or values.get("secrets", {}).get(key.upper()) or ""


def latest_test_event(events: list[dict]) -> dict | None:
    for event in reversed(events):
        if event.get("kind") in {"send", "inbound"}:
            return event
    return None


def result_banner(events: list[dict]) -> str:
    latest = latest_test_event(events)
    if latest is None:
        return '<section class="status neutral"><strong>No test run yet</strong><span>Click Outbound Test or Inbound Test to run a provider check.</span></section>'
    if latest.get("ok") is True:
        if latest.get("kind") == "inbound":
            detail = "Inbound email provider test passed."
        else:
            parsed = latest.get("json") or {}
            provider_type = parsed.get("provider_type")
            if PROVIDER == "microsoft-email":
                detail = "Microsoft Graph outbound email provider test passed."
            elif provider_type == "messaging.email.smtp":
                detail = "SMTP outbound email provider test passed."
            else:
                detail = "Outbound email provider test passed."
        return f'<section class="status pass"><strong>Test passed</strong><span>{esc(detail)}</span></section>'
    error = latest.get("error") or latest.get("stderr") or "Unknown error"
    return f'<section class="status fail"><strong>Test failed</strong><span>{esc(error)}</span></section>'


def page() -> str:
    values = read_values()
    events = read_events()
    fields = []
    if PROVIDER == "microsoft-email":
        note = "Mock mode validates the Microsoft Graph provider pipeline without calling Microsoft Graph. Real mode calls the configured Graph token and sendMail endpoints."
    else:
        note = "Outbound tests call the messaging-email SMTP provider operation. Inbound tests call the provider's current webhook ingestion surface with mocked Graph notification data. IMAP and POP3 polling are not implemented by this provider yet."
    if PROVIDER == "microsoft-email":
        fields.extend(
            [
                input_field("from_address", "From address", form_value(values, "from_address")),
                input_field("graph_tenant_id", "Graph tenant ID", form_value(values, "graph_tenant_id")),
                input_field("ms_graph_client_id", "Graph client ID", form_value(values, "ms_graph_client_id")),
                input_field("ms_graph_refresh_token", "Graph refresh token", values.get("secrets", {}).get("MS_GRAPH_REFRESH_TOKEN", ""), "password"),
                input_field("ms_graph_client_secret", "Graph client secret", values.get("secrets", {}).get("MS_GRAPH_CLIENT_SECRET", ""), "password"),
                input_field("graph_scope", "Graph scope", form_value(values, "graph_scope")),
            ]
        )
    else:
        fields.extend(
            [
                input_field("host", "SMTP host", form_value(values, "host")),
                input_field("port", "SMTP port", form_value(values, "port") or "587", "number"),
                input_field("username", "SMTP username", form_value(values, "username")),
                input_field("password", "SMTP password", values.get("secrets", {}).get("EMAIL_PASSWORD", ""), "password"),
                input_field("from_address", "From address", form_value(values, "from_address")),
                input_field("tls_mode", "TLS mode", form_value(values, "tls_mode") or "starttls"),
            ]
        )
    event_html = "\n".join(
        f"<li><time>{esc(event.get('ts'))}</time><pre>{esc(json.dumps(event, indent=2))}</pre></li>"
        for event in reversed(events)
    )
    return f"""<!doctype html>
<html lang="en">
<head>
  <meta charset="utf-8">
  <meta name="viewport" content="width=device-width, initial-scale=1">
  <title>{esc(TITLE)} tester</title>
  <style>
    :root {{ color-scheme: light; font-family: Inter, ui-sans-serif, system-ui, -apple-system, BlinkMacSystemFont, "Segoe UI", sans-serif; }}
    body {{ margin: 0; background: #f6f7f9; color: #18202a; }}
    main {{ max-width: 1120px; margin: 0 auto; padding: 28px; }}
    h1 {{ font-size: 28px; margin: 0 0 6px; }}
    p {{ margin: 0 0 20px; color: #5a6472; }}
    section {{ background: white; border: 1px solid #d8dde5; border-radius: 8px; padding: 18px; margin: 16px 0; }}
    .grid {{ display: grid; grid-template-columns: repeat(2, minmax(0, 1fr)); gap: 14px; }}
    label {{ display: grid; gap: 6px; font-size: 13px; font-weight: 650; color: #334155; }}
    input, textarea, select {{ width: 100%; box-sizing: border-box; border: 1px solid #c9d1dc; border-radius: 6px; padding: 9px 10px; font: inherit; background: #fff; }}
    textarea {{ min-height: 96px; resize: vertical; }}
    button {{ border: 0; border-radius: 6px; background: #1463d9; color: #fff; padding: 10px 14px; font: inherit; font-weight: 700; cursor: pointer; }}
    .actions {{ display: flex; gap: 10px; align-items: center; margin-top: 14px; }}
    .note {{ padding: 10px 12px; background: #fff8e6; border: 1px solid #ead59b; border-radius: 6px; color: #624b12; }}
    .status {{ display: grid; gap: 4px; border-radius: 8px; padding: 14px 16px; margin: 16px 0; }}
    .status strong {{ font-size: 16px; }}
    .status span {{ color: #344054; white-space: pre-wrap; word-break: break-word; }}
    .status.neutral {{ background: #eef4ff; border: 1px solid #b8cdf8; }}
    .status.pass {{ background: #ecfdf3; border: 1px solid #93d4aa; }}
    .status.fail {{ background: #fff1f2; border: 1px solid #f3a8b2; }}
    ul {{ list-style: none; padding: 0; margin: 0; display: grid; gap: 12px; }}
    time {{ display: block; color: #667085; font-size: 12px; margin-bottom: 5px; }}
    pre {{ overflow: auto; white-space: pre-wrap; word-break: break-word; background: #101828; color: #f8fafc; padding: 12px; border-radius: 6px; margin: 0; font-size: 12px; }}
    @media (max-width: 760px) {{ main {{ padding: 18px; }} .grid {{ grid-template-columns: 1fr; }} }}
  </style>
</head>
<body>
  <main>
    <h1>{esc(TITLE)} tester</h1>
    <p>Configure values, then run outbound and inbound provider checks.</p>
    {result_banner(events)}
    <section>
      <h2>Outbound Test</h2>
      <form method="post" action="/send">
        <input type="hidden" name="action" value="send">
        <div class="grid">
          {''.join(fields)}
          {input_field("to", "To address", "")}
          {input_field("subject", "Subject", "Greentic email test")}
          <label>HTTP mode<select name="http"><option value="mock" {"selected" if values.get("http") == "mock" else ""}>mock</option><option value="real" {"selected" if values.get("http") == "real" else ""}>real</option></select></label>
          <label>Message<textarea name="body">Hello from Greentic.</textarea></label>
        </div>
        <div class="actions"><button type="submit">Outbound Test</button><span>{esc(VALUES)}</span></div>
      </form>
    </section>
    <section>
      <h2>Inbound Test</h2>
      <form method="post" action="/send">
        <input type="hidden" name="action" value="inbound">
        <div class="grid">
          {input_field("inbound_from_address", "Inbound account address", form_value(values, "from_address") or "worker@example.com")}
        </div>
        <div class="actions"><button type="submit">Inbound Test</button><span>Uses mocked Graph webhook notification data.</span></div>
      </form>
    </section>
    <section class="note">{esc(note)}</section>
    <section>
      <h2>Events</h2>
      <ul>{event_html or "<li>No events yet.</li>"}</ul>
    </section>
  </main>
</body>
</html>"""


def input_field(name: str, label: str, value: str, typ: str = "text") -> str:
    return f'<label>{esc(label)}<input name="{esc(name)}" type="{esc(typ)}" value="{esc(value)}"></label>'


class Handler(BaseHTTPRequestHandler):
    def do_GET(self) -> None:
        if urlparse(self.path).path != "/":
            self.send_error(404)
            return
        self.respond_html(page())

    def do_POST(self) -> None:
        if urlparse(self.path).path != "/send":
            self.send_error(404)
            return
        length = int(self.headers.get("content-length", "0") or "0")
        raw = self.rfile.read(length).decode("utf-8", errors="replace")
        form = {key: values[-1] for key, values in parse_qs(raw, keep_blank_values=True).items()}
        action = form.get("action", "send")
        try:
            if action == "inbound":
                result = run_inbound(form.get("inbound_from_address", ""))
                append_event("inbound", result)
            else:
                write_values(merged_values(form))
                result = run_send(form.get("to", ""), form.get("subject", ""), form.get("body", ""))
                append_event("send", result)
        except Exception as exc:
            append_event(action, {"ok": False, "error": str(exc)})
        self.send_response(303)
        self.send_header("location", "/")
        self.end_headers()

    def respond_html(self, body: str) -> None:
        data = body.encode("utf-8")
        self.send_response(200)
        self.send_header("content-type", "text/html; charset=utf-8")
        self.send_header("content-length", str(len(data)))
        self.end_headers()
        self.wfile.write(data)

    def log_message(self, fmt: str, *args) -> None:
        append_event("server", {"message": fmt % args})


def main() -> int:
    WORK.mkdir(parents=True, exist_ok=True)
    if not VALUES.exists():
        write_values(default_values())
    url = f"http://127.0.0.1:{PORT}/"
    print(f"{TITLE} tester: {url}", flush=True)
    if OPEN_BROWSER:
        webbrowser.open(url)
    server = ThreadingHTTPServer(("127.0.0.1", PORT), Handler)
    try:
        server.serve_forever()
    except KeyboardInterrupt:
        return 0
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
