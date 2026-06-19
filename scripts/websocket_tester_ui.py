#!/usr/bin/env python3
from __future__ import annotations

import html
import importlib.util
import json
import os
import sys
import time
import traceback
import webbrowser
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
from pathlib import Path
from urllib.parse import parse_qs, urlparse


ROOT = Path(os.environ["GREENTIC_ROOT"]).resolve()
PORT = int(os.environ.get("GREENTIC_WS_TEST_PORT", "8798"))
OPEN_BROWSER = os.environ.get("GREENTIC_WS_TEST_OPEN", "1") != "0"
WORK = Path(os.environ.get("TMPDIR", "/tmp")) / f"greentic-websocket-test-{PORT}"
EVENTS = WORK / "events.jsonl"
MODULE_PATH = ROOT / "generated-providers" / "messaging-websocket" / "src" / "messaging_websocket_provider.py"


def load_provider():
    spec = importlib.util.spec_from_file_location("messaging_websocket_provider", MODULE_PATH)
    if spec is None or spec.loader is None:
        raise RuntimeError(f"failed to load provider module: {MODULE_PATH}")
    module = importlib.util.module_from_spec(spec)
    sys.modules[spec.name] = module
    spec.loader.exec_module(module)
    return module


WS_PROVIDER = load_provider()


def now() -> str:
    return time.strftime("%Y-%m-%dT%H:%M:%SZ", time.gmtime())


def append_event(kind: str, payload: dict) -> None:
    WORK.mkdir(parents=True, exist_ok=True)
    event = {"ts": now(), "kind": kind, **payload}
    with EVENTS.open("a", encoding="utf-8") as handle:
        handle.write(json.dumps(event, ensure_ascii=False) + "\n")


def read_events() -> list[dict]:
    if not EVENTS.exists():
        return []
    out: list[dict] = []
    for line in EVENTS.read_text(encoding="utf-8").splitlines()[-100:]:
        try:
            out.append(json.loads(line))
        except json.JSONDecodeError:
            pass
    return out


def esc(value) -> str:
    return html.escape(str(value or ""), quote=True)


def parse_json_object(raw: str, *, label: str) -> dict:
    value = json.loads(raw or "{}")
    if not isinstance(value, dict):
        raise ValueError(f"{label} must be a JSON object")
    return value


def parse_headers(raw: str) -> dict[str, str]:
    headers: dict[str, str] = {}
    for line in (raw or "").splitlines():
        if not line.strip():
            continue
        if ":" not in line:
            raise ValueError(f"invalid header line: {line}")
        name, value = line.split(":", 1)
        headers[name.strip()] = value.strip()
    return headers


def parse_query(raw: str) -> dict[str, str]:
    query: dict[str, str] = {}
    for line in (raw or "").splitlines():
        if not line.strip():
            continue
        if "=" not in line:
            raise ValueError(f"invalid query line: {line}")
        name, value = line.split("=", 1)
        query[name.strip()] = value.strip()
    return query


def inbound_from_form(form: dict[str, str]) -> dict:
    config = parse_json_object(form.get("inbound_config", "{}"), label="inbound config")
    frame = WS_PROVIDER.WebSocketFrame(
        session_id=form.get("session_id", "s-1"),
        data=form.get("frame_data", "{}"),
        headers=parse_headers(form.get("headers", "")),
        query=parse_query(form.get("query", "")),
    )
    return WS_PROVIDER.inbound_frame_to_envelope(frame, config)


def outbound_from_form(form: dict[str, str]) -> dict:
    message = {
        "session_id": form.get("outbound_session_id", "s-1"),
        "payload": parse_json_object(form.get("payload", "{}"), label="payload"),
    }
    return WS_PROVIDER.outbound_message_to_frame(message)


def lifecycle_from_form(form: dict[str, str]) -> dict:
    detail = parse_json_object(form.get("detail", "{}"), label="detail")
    return WS_PROVIDER.lifecycle_event(
        form.get("lifecycle_session_id", "s-1"),
        form.get("event", "open"),
        detail,
    )


def run_action(action: str, form: dict[str, str]) -> dict:
    try:
        if action == "inbound":
            return {"ok": True, "result": inbound_from_form(form)}
        if action == "outbound":
            return {"ok": True, "result": outbound_from_form(form)}
        if action == "lifecycle":
            return {"ok": True, "result": lifecycle_from_form(form)}
        return {"ok": False, "error": f"unknown action: {action}"}
    except Exception as exc:
        result = {
            "ok": False,
            "error": str(exc),
            "type": exc.__class__.__name__,
        }
        if hasattr(exc, "code"):
            result["code"] = getattr(exc, "code")
        result["trace"] = traceback.format_exc(limit=4)
        return result


def input_field(name: str, label: str, value: str, typ: str = "text") -> str:
    return f'<label>{esc(label)}<input name="{esc(name)}" type="{esc(typ)}" value="{esc(value)}"></label>'


def textarea(name: str, label: str, value: str) -> str:
    return f'<label>{esc(label)}<textarea name="{esc(name)}">{esc(value)}</textarea></label>'


def latest_action_events(events: list[dict]) -> list[dict]:
    return [event for event in events if event.get("kind") in {"inbound", "outbound", "lifecycle"}]


def result_banner(events: list[dict]) -> str:
    action_events = latest_action_events(events)
    if not action_events:
        return '<section class="status neutral"><strong>No test run yet</strong><span>Click one of the test buttons to run a provider check.</span></section>'
    latest = action_events[-1]
    if latest.get("ok") is True:
        labels = {
            "inbound": "Inbound frame mapping passed.",
            "outbound": "Outbound frame generation passed.",
            "lifecycle": "Lifecycle event normalization passed.",
        }
        detail = labels.get(latest.get("kind"), "Provider check passed.")
        return f'<section class="status pass"><strong>Test passed</strong><span>{esc(detail)}</span></section>'
    error = latest.get("error") or latest.get("code") or "Unknown error"
    return f'<section class="status fail"><strong>Test failed</strong><span>{esc(error)}</span></section>'


def page() -> str:
    events = read_events()
    event_html = "\n".join(
        f"<li><time>{esc(event.get('ts'))}</time><pre>{esc(json.dumps(event, indent=2))}</pre></li>"
        for event in reversed(events)
    )
    inbound_config = json.dumps({"tenant_id": "demo", "team_id": "default"}, indent=2)
    frame_data = json.dumps({"event": "created", "case_id": "C123"}, indent=2)
    payload = json.dumps({"text": "hello"}, indent=2)
    detail = json.dumps({"ip": "127.0.0.1"}, indent=2)
    return f"""<!doctype html>
<html lang="en">
<head>
  <meta charset="utf-8">
  <meta name="viewport" content="width=device-width, initial-scale=1">
  <title>WebSocket provider tester</title>
  <style>
    :root {{ font-family: Inter, ui-sans-serif, system-ui, -apple-system, BlinkMacSystemFont, "Segoe UI", sans-serif; color-scheme: light; }}
    body {{ margin: 0; background: #f5f7fb; color: #17202c; }}
    main {{ max-width: 1180px; margin: 0 auto; padding: 28px; }}
    h1 {{ font-size: 28px; margin: 0 0 6px; }}
    h2 {{ font-size: 18px; margin: 0 0 14px; }}
    p {{ margin: 0 0 20px; color: #5d6775; }}
    section {{ background: white; border: 1px solid #d9dee8; border-radius: 8px; padding: 18px; margin: 16px 0; }}
    .two {{ display: grid; grid-template-columns: repeat(2, minmax(0, 1fr)); gap: 16px; align-items: start; }}
    .grid {{ display: grid; grid-template-columns: repeat(2, minmax(0, 1fr)); gap: 12px; }}
    label {{ display: grid; gap: 6px; font-size: 13px; font-weight: 650; color: #344054; }}
    input, textarea, select {{ width: 100%; box-sizing: border-box; border: 1px solid #c9d1dc; border-radius: 6px; padding: 9px 10px; font: inherit; background: #fff; }}
    textarea {{ min-height: 112px; resize: vertical; font-family: ui-monospace, SFMono-Regular, Menlo, Consolas, monospace; font-size: 12px; }}
    button {{ border: 0; border-radius: 6px; background: #1463d9; color: white; padding: 10px 14px; font: inherit; font-weight: 700; cursor: pointer; margin-top: 12px; }}
    .status {{ display: grid; gap: 4px; border-radius: 8px; padding: 14px 16px; margin: 16px 0; }}
    .status strong {{ font-size: 16px; }}
    .status span {{ color: #344054; }}
    .status.neutral {{ background: #eef4ff; border: 1px solid #b8cdf8; }}
    .status.pass {{ background: #ecfdf3; border: 1px solid #93d4aa; }}
    .status.fail {{ background: #fff1f2; border: 1px solid #f3a8b2; }}
    ul {{ list-style: none; padding: 0; margin: 0; display: grid; gap: 12px; }}
    time {{ display: block; color: #667085; font-size: 12px; margin-bottom: 5px; }}
    pre {{ overflow: auto; white-space: pre-wrap; word-break: break-word; background: #101828; color: #f8fafc; padding: 12px; border-radius: 6px; margin: 0; font-size: 12px; }}
    @media (max-width: 860px) {{ main {{ padding: 18px; }} .two, .grid {{ grid-template-columns: 1fr; }} }}
  </style>
</head>
<body>
  <main>
    <h1>WebSocket provider tester</h1>
    <p>Test inbound frame mapping, outbound frame generation, and lifecycle event normalization from the answer-owned messaging-websocket provider.</p>
    {result_banner(events)}
    <div class="two">
      <section>
        <h2>Inbound Frame</h2>
        <form method="post" action="/run">
          <input type="hidden" name="action" value="inbound">
          <div class="grid">
            {input_field("session_id", "Session ID", "s-1")}
          </div>
          {textarea("headers", "Handshake headers", "")}
          {textarea("query", "Handshake query", "")}
          {textarea("frame_data", "Text frame JSON", frame_data)}
          {textarea("inbound_config", "Config JSON", inbound_config)}
          <button type="submit">Map Inbound</button>
        </form>
      </section>
      <section>
        <h2>Outbound Frame</h2>
        <form method="post" action="/run">
          <input type="hidden" name="action" value="outbound">
          {input_field("outbound_session_id", "Session ID", "s-1")}
          {textarea("payload", "Payload JSON", payload)}
          <button type="submit">Build Frame</button>
        </form>
      </section>
    </div>
    <section>
      <h2>Lifecycle Event</h2>
      <form method="post" action="/run">
        <input type="hidden" name="action" value="lifecycle">
        <div class="grid">
          {input_field("lifecycle_session_id", "Session ID", "s-1")}
          <label>Event<select name="event"><option value="open">open</option><option value="close">close</option><option value="error">error</option></select></label>
        </div>
        {textarea("detail", "Detail JSON", detail)}
        <button type="submit">Normalize Event</button>
      </form>
    </section>
    <section>
      <h2>Events</h2>
      <ul>{event_html or "<li>No events yet.</li>"}</ul>
    </section>
  </main>
</body>
</html>"""


class Handler(BaseHTTPRequestHandler):
    def do_GET(self) -> None:
        if urlparse(self.path).path != "/":
            self.send_error(404)
            return
        self.respond_html(page())

    def do_POST(self) -> None:
        if urlparse(self.path).path != "/run":
            self.send_error(404)
            return
        length = int(self.headers.get("content-length", "0") or "0")
        raw = self.rfile.read(length).decode("utf-8", errors="replace")
        form = {key: values[-1] for key, values in parse_qs(raw, keep_blank_values=True).items()}
        action = form.get("action", "")
        append_event(action or "run", run_action(action, form))
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
    url = f"http://127.0.0.1:{PORT}/"
    print(f"WebSocket provider tester: {url}", flush=True)
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
