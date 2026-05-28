#!/usr/bin/env bash
# Start a local Webex tester UI with a cloudflared public URL.
#
# The page lets you enter the Webex setup values, registers the Webex webhook,
# sends test messages, and displays incoming webhook calls routed through
# greentic-messaging-tester ingress.
#
# Usage:
#   scripts/test_webex.sh [--port <port>] [--no-build] [--no-open]

set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "${ROOT_DIR}"

PORT="${PORT:-8792}"
BUILD=1
OPEN_BROWSER=1
CLOUDFLARED_BIN="${CLOUDFLARED_BIN:-cloudflared}"

while [ $# -gt 0 ]; do
  case "$1" in
    -h|--help)
      sed -n '2,10p' "$0" >&2
      exit 0
      ;;
    --no-build)
      BUILD=0
      ;;
    --no-open)
      OPEN_BROWSER=0
      ;;
    --port)
      shift
      PORT="${1:-}"
      if [ -z "${PORT}" ]; then
        echo "--port requires a value" >&2
        exit 2
      fi
      ;;
    --port=*)
      PORT="${1#--port=}"
      ;;
    -*)
      echo "unknown argument: $1" >&2
      exit 2
      ;;
    *)
      echo "unexpected argument: $1" >&2
      exit 2
      ;;
  esac
  shift
done

if ! command -v "${CLOUDFLARED_BIN}" >/dev/null 2>&1; then
  echo "cloudflared not found. Install it or set CLOUDFLARED_BIN=/path/to/cloudflared" >&2
  exit 1
fi

if [ "${BUILD}" -eq 1 ]; then
  scripts/build_providers.sh webex
  cargo build -p greentic-messaging-tester
fi

TESTER_BIN="${ROOT_DIR}/target/debug/greentic-messaging-tester"
if [ ! -x "${TESTER_BIN}" ]; then
  echo "${TESTER_BIN} not found; run without --no-build first" >&2
  exit 1
fi

WORK_DIR="${TMPDIR:-/tmp}/greentic-webex-test-${PORT}"
rm -rf "${WORK_DIR}"
mkdir -p "${WORK_DIR}"

cat > "${WORK_DIR}/server.py" <<'PY'
from __future__ import annotations

import json
import os
import subprocess
import threading
import time
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
from pathlib import Path
from urllib.parse import quote, urlparse


ROOT = Path(os.environ["GREENTIC_ROOT"]).resolve()
WORK = Path(os.environ["GREENTIC_WEBEX_WORK"]).resolve()
TESTER = Path(os.environ["GREENTIC_TESTER_BIN"]).resolve()
VALUES = WORK / "webex-values.json"
EVENTS = WORK / "events.jsonl"
PUBLIC_URL_FILE = WORK / "public-url.txt"

EVENT_LOCK = threading.Lock()


def public_url() -> str:
    try:
        return PUBLIC_URL_FILE.read_text(encoding="utf-8").strip()
    except FileNotFoundError:
        return ""


def webhook_path(tenant: str = "default", channel: str = "default") -> str:
    return f"/v1/messaging/webex/{quote(tenant or 'default', safe='')}/{quote(channel or 'default', safe='')}/webhook"


def callback_url(tenant: str = "default", channel: str = "default") -> str:
    base = public_url().rstrip("/")
    if not base:
        return ""
    return base + webhook_path(tenant, channel)


def append_event(kind: str, payload: dict) -> None:
    event = {
        "ts": time.strftime("%Y-%m-%dT%H:%M:%SZ", time.gmtime()),
        "kind": kind,
        **payload,
    }
    with EVENT_LOCK:
        with EVENTS.open("a", encoding="utf-8") as handle:
            handle.write(json.dumps(event, ensure_ascii=False) + "\n")


def read_events() -> list[dict]:
    if not EVENTS.exists():
        return []
    with EVENT_LOCK:
        lines = EVENTS.read_text(encoding="utf-8").splitlines()
    out = []
    for line in lines[-200:]:
        try:
            out.append(json.loads(line))
        except json.JSONDecodeError:
            pass
    return out


def redact_text(text: str) -> str:
    if not VALUES.exists():
        return text
    try:
        values = json.loads(VALUES.read_text(encoding="utf-8"))
    except json.JSONDecodeError:
        return text
    for secret in values.get("secrets", {}).values():
        if isinstance(secret, str) and secret:
            text = text.replace(secret, "[redacted]")
    for key, secret in values.get("config", {}).items():
        if isinstance(secret, str) and secret and "secret" in key.lower():
            text = text.replace(secret, "[redacted]")
    return text


def run_tester(args: list[str], timeout: int = 90) -> dict:
    proc = subprocess.run(
        [str(TESTER), *args],
        cwd=str(ROOT),
        text=True,
        capture_output=True,
        timeout=timeout,
    )
    stdout = redact_text(proc.stdout)
    stderr = redact_text(proc.stderr)
    parsed = None
    if stdout.strip():
        try:
            parsed = json.loads(stdout)
        except json.JSONDecodeError:
            parsed = None
    return {
        "ok": proc.returncode == 0,
        "status": proc.returncode,
        "stdout": stdout,
        "stderr": stderr,
        "json": parsed,
    }


def values_from_form(data: dict) -> dict:
    config = {
        "enabled": True,
        "public_base_url": public_url(),
        "api_base_url": "https://webexapis.com/v1",
        "default_room_id": data.get("default_room_id") or None,
        "default_to_person_email": data.get("default_to_person_email") or None,
        "webhook_secret": data.get("webhook_secret") or None,
    }
    return {
        "config": config,
        "secrets": {
            "WEBEX_BOT_TOKEN": data.get("bot_token") or "",
        },
        "http": "real",
        "state": {},
    }


def clean_values(values: dict) -> dict:
    values = json.loads(json.dumps(values))
    values["config"] = {k: v for k, v in values["config"].items() if v not in ("", None)}
    values["secrets"] = {k: v for k, v in values["secrets"].items() if v not in ("", None)}
    return values


def page_html() -> bytes:
    return f"""<!doctype html>
<html lang="en">
<head>
  <meta charset="utf-8">
  <meta name="viewport" content="width=device-width, initial-scale=1">
  <title>Greentic Webex Tester</title>
  <style>
    body {{ font-family: Inter, system-ui, sans-serif; margin: 0; background: #f6f7f9; color: #1f2937; }}
    main {{ max-width: 1120px; margin: 0 auto; padding: 24px; }}
    h1 {{ font-size: 24px; margin: 0 0 16px; }}
    section {{ background: #fff; border: 1px solid #d8dee8; border-radius: 8px; padding: 16px; margin: 16px 0; }}
    .grid {{ display: grid; grid-template-columns: repeat(auto-fit, minmax(260px, 1fr)); gap: 12px; }}
    label {{ display: block; font-weight: 600; font-size: 13px; }}
    input, textarea, select {{ width: 100%; box-sizing: border-box; margin-top: 4px; padding: 9px 10px; border: 1px solid #cbd5e1; border-radius: 6px; font: inherit; background: #fff; }}
    button {{ border: 1px solid #047857; background: #ecfdf5; color: #065f46; border-radius: 6px; padding: 9px 13px; font-weight: 700; cursor: pointer; }}
    button:disabled {{ opacity: .65; cursor: wait; }}
    code {{ background: #eef2f7; padding: 2px 5px; border-radius: 4px; }}
    pre {{ white-space: pre-wrap; overflow: auto; background: #111827; color: #e5e7eb; padding: 12px; border-radius: 6px; max-height: 420px; }}
    .row {{ display: flex; flex-wrap: wrap; gap: 8px; align-items: center; }}
    .muted {{ color: #64748b; font-size: 13px; }}
  </style>
</head>
<body>
<main>
  <h1>Greentic Webex Tester</h1>
  <section>
    <div class="row">
      <strong>Public URL:</strong>
      <code id="publicUrl">{public_url() or "waiting for cloudflared..."}</code>
    </div>
    <div class="row">
      <strong>Webhook URL:</strong>
      <code id="webhookUrl">{callback_url() or "waiting for cloudflared..."}</code>
    </div>
  </section>
  <section>
    <h2>Setup</h2>
    <div class="grid">
      <label>Webex bot token*<input id="bot_token" type="password" autocomplete="off" placeholder="Bearer token"></label>
      <label>Webhook secret<input id="webhook_secret" type="password" autocomplete="off" placeholder="Optional shared secret"></label>
      <label>Default room ID<input id="default_room_id" autocomplete="off" placeholder="Y2lz..."></label>
      <label>Default person email<input id="default_to_person_email" autocomplete="off" placeholder="user@example.com"></label>
      <label>Tenant<input id="tenant" value="default"></label>
      <label>Channel<input id="channel" value="default"></label>
    </div>
    <p class="muted">API base URL is not requested here; Webex defaults to https://webexapis.com/v1.</p>
    <div class="row">
      <button id="registerBtn">Register webhook with Webex</button>
      <button id="saveBtn" type="button">Save values only</button>
    </div>
  </section>
  <section>
    <h2>Send</h2>
    <div class="grid">
      <label>Destination ID<input id="send_to" placeholder="Room ID, person ID, or email"></label>
      <label>Destination kind<select id="send_kind">
        <option value="room">Room</option>
        <option value="person">Person ID</option>
        <option value="email">Person email</option>
      </select></label>
      <label>Message<textarea id="send_text" rows="3">Hello from Greentic Webex tester</textarea></label>
    </div>
    <button id="sendBtn">Send Webex message</button>
  </section>
  <section>
    <h2>Incoming Webhooks</h2>
    <div class="row">
      <button id="refreshBtn" type="button">Refresh</button>
      <span id="status" class="muted"></span>
    </div>
    <pre id="events">[]</pre>
  </section>
  <section>
    <h2>Last Result</h2>
    <pre id="result">{{}}</pre>
  </section>
</main>
<script>
const ids = ["bot_token","webhook_secret","default_room_id","default_to_person_email","tenant","channel","send_to","send_kind","send_text"];
for (const id of ids) {{
  const saved = localStorage.getItem("webexTester." + id);
  if (saved !== null) document.getElementById(id).value = saved;
  document.getElementById(id).addEventListener("input", e => localStorage.setItem("webexTester." + id, e.target.value));
}}
function formValues() {{
  return Object.fromEntries(ids.map(id => [id, document.getElementById(id).value.trim()]));
}}
async function post(path, body) {{
  const res = await fetch(path, {{ method: "POST", headers: {{ "content-type": "application/json" }}, body: JSON.stringify(body) }});
  const data = await res.json();
  document.getElementById("result").textContent = JSON.stringify(data, null, 2);
  return data;
}}
async function refresh() {{
  const res = await fetch("/api/events");
  const data = await res.json();
  document.getElementById("events").textContent = JSON.stringify(data.events, null, 2);
  document.getElementById("publicUrl").textContent = data.public_url || "waiting for cloudflared...";
  document.getElementById("webhookUrl").textContent = data.webhook_url || "waiting for cloudflared...";
}}
document.getElementById("saveBtn").onclick = () => post("/api/save", formValues());
document.getElementById("registerBtn").onclick = async e => {{ e.target.disabled = true; try {{ await post("/api/register", formValues()); await refresh(); }} finally {{ e.target.disabled = false; }} }};
document.getElementById("sendBtn").onclick = async e => {{ e.target.disabled = true; try {{ await post("/api/send", formValues()); }} finally {{ e.target.disabled = false; }} }};
document.getElementById("refreshBtn").onclick = refresh;
setInterval(refresh, 3000);
refresh();
</script>
</body>
</html>""".encode("utf-8")


class Handler(BaseHTTPRequestHandler):
    server_version = "GreenticWebexTester/1.0"

    def log_message(self, fmt: str, *args) -> None:
        message = fmt % args
        if '"GET /api/events ' in message:
            return
        append_event("server-log", {"message": message})

    def read_json(self) -> dict:
        length = int(self.headers.get("content-length") or "0")
        raw = self.rfile.read(length) if length else b"{}"
        try:
            return json.loads(raw.decode("utf-8"))
        except json.JSONDecodeError:
            return {}

    def send_json(self, data: dict, status: int = 200) -> None:
        body = json.dumps(data, indent=2, ensure_ascii=False).encode("utf-8")
        self.send_response(status)
        self.send_header("content-type", "application/json")
        self.send_header("content-length", str(len(body)))
        self.end_headers()
        self.wfile.write(body)

    def do_GET(self) -> None:
        if self.path == "/" or self.path == "/index.html":
            body = page_html()
            self.send_response(200)
            self.send_header("content-type", "text/html; charset=utf-8")
            self.send_header("content-length", str(len(body)))
            self.end_headers()
            self.wfile.write(body)
            return
        if self.path == "/api/events":
            self.send_json({"public_url": public_url(), "webhook_url": callback_url(), "events": read_events()})
            return
        self.send_error(404)

    def do_POST(self) -> None:
        path = urlparse(self.path).path
        if path == "/api/save":
            values = clean_values(values_from_form(self.read_json()))
            VALUES.write_text(json.dumps(values, indent=2) + "\n", encoding="utf-8")
            append_event("values-saved", {"values_path": str(VALUES)})
            self.send_json({"ok": True, "values_path": str(VALUES)})
            return
        if path == "/api/register":
            data = self.read_json()
            values = clean_values(values_from_form(data))
            VALUES.write_text(json.dumps(values, indent=2) + "\n", encoding="utf-8")
            target = callback_url(data.get("tenant") or "default", data.get("channel") or "default")
            args = [
                "webhook",
                "--provider", "webex",
                "--values", str(VALUES),
                "--public-base-url", target,
            ]
            if data.get("webhook_secret"):
                args.extend(["--secret-token", data["webhook_secret"]])
            result = run_tester(args)
            append_event("register", {"target": target, "result": result})
            self.send_json(result, 200 if result["ok"] else 500)
            return
        if path == "/api/send":
            data = self.read_json()
            values = clean_values(values_from_form(data))
            VALUES.write_text(json.dumps(values, indent=2) + "\n", encoding="utf-8")
            target = data.get("send_to") or values.get("config", {}).get("default_room_id") or values.get("config", {}).get("default_to_person_email") or ""
            target_kind = data.get("send_kind") or ("email" if "@" in target else "room")
            if not target:
                result = {
                    "ok": False,
                    "status": 2,
                    "stdout": "",
                    "stderr": "Destination ID is required. Use a room ID, person ID, or person email.\n",
                    "json": None,
                }
                append_event("send", {"target": target, "target_kind": target_kind, "result": result})
                self.send_json(result, 400)
                return
            result = run_tester([
                "send",
                "--provider", "webex",
                "--values", str(VALUES),
                "--to", target,
                "--to-kind", target_kind,
                "--text", data.get("send_text") or "Hello from Greentic Webex tester",
            ])
            append_event("send", {"target": target, "target_kind": target_kind, "result": result})
            self.send_json(result, 200 if result["ok"] else 500)
            return
        if path.startswith("/v1/messaging/webex/") and path.endswith("/webhook"):
            length = int(self.headers.get("content-length") or "0")
            raw = self.rfile.read(length) if length else b""
            body_text = raw.decode("utf-8", errors="replace")
            headers = {k: v for k, v in self.headers.items()}
            append_event("webhook-received", {"path": path, "headers": headers, "body": body_text})
            http_in = {
                "method": "POST",
                "path": path,
                "headers": headers,
                "body": body_text,
            }
            http_path = WORK / f"webex-ingress-{int(time.time() * 1000)}.json"
            http_path.write_text(json.dumps(http_in, indent=2) + "\n", encoding="utf-8")
            result = run_tester([
                "ingress",
                "--provider", "webex",
                "--values", str(VALUES),
                "--http-in", str(http_path),
                "--public-base-url", public_url(),
            ])
            append_event("ingress", {"http_in": str(http_path), "result": result})
            self.send_json({"ok": result["ok"]}, 200 if result["ok"] else 500)
            return
        self.send_error(404)


if __name__ == "__main__":
    server = ThreadingHTTPServer(("127.0.0.1", int(os.environ["GREENTIC_WEBEX_PORT"])), Handler)
    append_event("server-started", {"port": int(os.environ["GREENTIC_WEBEX_PORT"])})
    server.serve_forever()
PY

SERVER_LOG="${WORK_DIR}/server.log"
CLOUDFLARED_LOG="${WORK_DIR}/cloudflared.log"
PUBLIC_URL_FILE="${WORK_DIR}/public-url.txt"

GREENTIC_ROOT="${ROOT_DIR}" \
GREENTIC_WEBEX_WORK="${WORK_DIR}" \
GREENTIC_TESTER_BIN="${TESTER_BIN}" \
GREENTIC_WEBEX_PORT="${PORT}" \
  python3 "${WORK_DIR}/server.py" >"${SERVER_LOG}" 2>&1 &
SERVER_PID=$!

"${CLOUDFLARED_BIN}" tunnel --url "http://127.0.0.1:${PORT}" --no-autoupdate >"${CLOUDFLARED_LOG}" 2>&1 &
CLOUDFLARED_PID=$!

cleanup() {
  kill "${CLOUDFLARED_PID}" "${SERVER_PID}" >/dev/null 2>&1 || true
}
trap cleanup EXIT

PUBLIC_URL=""
for _ in $(seq 1 60); do
  if ! kill -0 "${SERVER_PID}" >/dev/null 2>&1; then
    echo "server exited. Log:" >&2
    sed -n '1,120p' "${SERVER_LOG}" >&2 || true
    exit 1
  fi
  if ! kill -0 "${CLOUDFLARED_PID}" >/dev/null 2>&1; then
    echo "cloudflared exited. Log:" >&2
    sed -n '1,160p' "${CLOUDFLARED_LOG}" >&2 || true
    exit 1
  fi
  PUBLIC_URL="$(grep -Eo 'https://[-a-zA-Z0-9.]+\.trycloudflare\.com' "${CLOUDFLARED_LOG}" | head -1 || true)"
  if [ -n "${PUBLIC_URL}" ]; then
    printf '%s\n' "${PUBLIC_URL}" > "${PUBLIC_URL_FILE}"
    break
  fi
  sleep 1
done

if [ -z "${PUBLIC_URL}" ]; then
  echo "timed out waiting for cloudflared public URL. Log:" >&2
  sed -n '1,160p' "${CLOUDFLARED_LOG}" >&2 || true
  exit 1
fi

LOCAL_URL="http://127.0.0.1:${PORT}/"
WEBHOOK_URL="${PUBLIC_URL}/v1/messaging/webex/default/default/webhook"
echo "Webex tester UI: ${LOCAL_URL}"
echo "Public URL: ${PUBLIC_URL}"
echo "Default Webex webhook URL: ${WEBHOOK_URL}"
echo "Logs: ${WORK_DIR}"

if [ "${OPEN_BROWSER}" -eq 1 ]; then
  if command -v xdg-open >/dev/null 2>&1; then
    xdg-open "${LOCAL_URL}" >/dev/null 2>&1 || true
  elif command -v open >/dev/null 2>&1; then
    open "${LOCAL_URL}" >/dev/null 2>&1 || true
  fi
fi

wait "${SERVER_PID}"
