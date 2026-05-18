#!/usr/bin/env bash
# Start a local Slack tester UI with a cloudflared public URL.
#
# The page lets you enter the same setup values as gtc setup, registers the
# Slack app manifest with Slack, sends test messages, and displays incoming
# webhook calls routed through greentic-messaging-tester ingress.
#
# Usage:
#   scripts/test_slack.sh [--port <port>] [--no-build] [--no-open]

set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "${ROOT_DIR}"

PORT="${PORT:-8791}"
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
  scripts/build_providers.sh slack
  cargo build -p greentic-messaging-tester
fi

TESTER_BIN="${ROOT_DIR}/target/debug/greentic-messaging-tester"
if [ ! -x "${TESTER_BIN}" ]; then
  echo "${TESTER_BIN} not found; run without --no-build first" >&2
  exit 1
fi

WORK_DIR="${TMPDIR:-/tmp}/greentic-slack-test-${PORT}"
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
from urllib.parse import urlparse


ROOT = Path(os.environ["GREENTIC_ROOT"]).resolve()
WORK = Path(os.environ["GREENTIC_SLACK_WORK"]).resolve()
TESTER = Path(os.environ["GREENTIC_TESTER_BIN"]).resolve()
VALUES = WORK / "slack-values.json"
EVENTS = WORK / "events.jsonl"
PUBLIC_URL_FILE = WORK / "public-url.txt"

EVENT_LOCK = threading.Lock()


def public_url() -> str:
    try:
        return PUBLIC_URL_FILE.read_text(encoding="utf-8").strip()
    except FileNotFoundError:
        return ""


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


def run_tester(args: list[str], timeout: int = 90) -> dict:
    proc = subprocess.run(
        [str(TESTER), *args],
        cwd=str(ROOT),
        text=True,
        capture_output=True,
        timeout=timeout,
    )
    parsed = None
    if proc.stdout.strip():
        try:
            parsed = json.loads(proc.stdout)
        except json.JSONDecodeError:
            parsed = None
    result = {
        "ok": proc.returncode == 0,
        "status": proc.returncode,
        "stdout": proc.stdout,
        "stderr": proc.stderr,
        "json": parsed,
    }
    return result


def values_from_form(data: dict) -> dict:
    base = public_url()
    return {
        "config": {
            "api_base": "https://slack.com/api",
            "public_base_url": base,
            "provider_id": "messaging-slack",
            "tenant": data.get("tenant") or "default",
            "team": data.get("team") or "default",
            "default_channel": data.get("default_channel") or None,
        },
        "secrets": {
            "SLACK_BOT_TOKEN": data.get("bot_token") or "",
            "SLACK_APP_ID": data.get("slack_app_id") or "",
            "SLACK_CONFIGURATION_ACCESS_TOKEN": data.get("slack_configuration_access_token") or "",
            "SLACK_CONFIGURATION_REFRESH_TOKEN": data.get("slack_configuration_refresh_token") or "",
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
  <title>Greentic Slack Tester</title>
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
    .ok {{ color: #047857; }}
    .bad {{ color: #b91c1c; }}
  </style>
</head>
<body>
<main>
  <h1>Greentic Slack Tester</h1>
  <section>
    <div class="row">
      <strong>Public URL:</strong>
      <code id="publicUrl">{public_url() or "waiting for cloudflared..."}</code>
    </div>
    <p class="muted">The Slack app manifest will be updated to use <code>/v1/messaging/ingress/messaging-slack/default/default</code>.</p>
  </section>
  <section>
    <h2>Setup</h2>
    <div class="grid">
      <label>Slack App ID*<input id="slack_app_id" autocomplete="off" placeholder="A..."></label>
      <label>Slack bot token*<input id="bot_token" type="password" autocomplete="off" placeholder="xoxb-..."></label>
      <label>Configuration access token*<input id="slack_configuration_access_token" type="password" autocomplete="off" placeholder="xoxe-..."></label>
      <label>Configuration refresh token*<input id="slack_configuration_refresh_token" type="password" autocomplete="off" placeholder="xoxe-..."></label>
      <label>Default channel<input id="default_channel" autocomplete="off" placeholder="C..."></label>
      <label>Tenant<input id="tenant" value="default"></label>
      <label>Team<input id="team" value="default"></label>
    </div>
    <p class="muted">API base URL is not requested here; Slack defaults to https://slack.com/api.</p>
    <div class="row">
      <button id="registerBtn">Register webhook with Slack</button>
      <button id="saveBtn" type="button">Save values only</button>
    </div>
  </section>
  <section>
    <h2>Send</h2>
    <div class="grid">
      <label>Destination ID<input id="send_to" placeholder="C..., G..., or U..."></label>
      <label>Destination kind<select id="send_kind">
        <option value="channel">Channel or conversation</option>
        <option value="user">User DM</option>
      </select></label>
      <label>Message<textarea id="send_text" rows="3">Hello from Greentic Slack tester</textarea></label>
    </div>
    <p class="muted">Use a channel/conversation ID for channels and existing conversations. Use a Slack user ID (<code>U...</code>) with User DM.</p>
    <button id="sendBtn">Send Slack message</button>
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
const ids = ["slack_app_id","bot_token","slack_configuration_access_token","slack_configuration_refresh_token","default_channel","tenant","team","send_to","send_kind","send_text"];
for (const id of ids) {{
  const saved = localStorage.getItem("slackTester." + id);
  if (saved !== null) document.getElementById(id).value = saved;
  document.getElementById(id).addEventListener("input", e => localStorage.setItem("slackTester." + id, e.target.value));
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
    server_version = "GreenticSlackTester/1.0"

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
            self.send_json({"public_url": public_url(), "events": read_events()})
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
            values = clean_values(values_from_form(self.read_json()))
            VALUES.write_text(json.dumps(values, indent=2) + "\n", encoding="utf-8")
            result = run_tester([
                "webhook",
                "--provider", "slack",
                "--values", str(VALUES),
                "--public-base-url", public_url(),
            ])
            append_event("register", {"result": result})
            self.send_json(result, 200 if result["ok"] else 500)
            return
        if path == "/api/send":
            data = self.read_json()
            values = clean_values(values_from_form(data))
            VALUES.write_text(json.dumps(values, indent=2) + "\n", encoding="utf-8")
            target = data.get("send_to") or values.get("config", {}).get("default_channel") or ""
            target_kind = data.get("send_kind") or "channel"
            if not target:
                result = {
                    "ok": False,
                    "status": 2,
                    "stdout": "",
                    "stderr": "Destination ID is required. Use a channel/conversation ID, or a Slack user ID with User DM.\n",
                    "json": None,
                }
                append_event("send", {"target": target, "target_kind": target_kind, "result": result})
                self.send_json(result, 400)
                return
            result = run_tester([
                "send",
                "--provider", "slack",
                "--values", str(VALUES),
                "--to", target,
                "--to-kind", target_kind,
                "--text", data.get("send_text") or "Hello from Greentic Slack tester",
            ])
            append_event("send", {"target": target, "target_kind": target_kind, "result": result})
            self.send_json(result, 200 if result["ok"] else 500)
            return
        if path.startswith("/v1/messaging/ingress/"):
            length = int(self.headers.get("content-length") or "0")
            raw = self.rfile.read(length) if length else b""
            body_text = raw.decode("utf-8", errors="replace")
            headers = {k: v for k, v in self.headers.items()}
            append_event("webhook-received", {"path": path, "headers": headers, "body": body_text})
            try:
                body_json = json.loads(body_text) if body_text else {}
            except json.JSONDecodeError:
                body_json = {}
            if body_json.get("type") == "url_verification":
                challenge = body_json.get("challenge", "")
                append_event("url-verification", {"challenge": challenge})
                response = challenge.encode("utf-8")
                self.send_response(200)
                self.send_header("content-type", "text/plain; charset=utf-8")
                self.send_header("content-length", str(len(response)))
                self.end_headers()
                self.wfile.write(response)
                return
            http_in = {
                "method": "POST",
                "path": path,
                "headers": headers,
                "body": body_text,
            }
            http_path = WORK / f"slack-ingress-{int(time.time() * 1000)}.json"
            http_path.write_text(json.dumps(http_in, indent=2) + "\n", encoding="utf-8")
            result = run_tester([
                "ingress",
                "--provider", "slack",
                "--values", str(VALUES),
                "--http-in", str(http_path),
                "--public-base-url", public_url(),
            ])
            append_event("ingress", {"http_in": str(http_path), "result": result})
            self.send_json({"ok": result["ok"]})
            return
        self.send_error(404)


if __name__ == "__main__":
    server = ThreadingHTTPServer(("127.0.0.1", int(os.environ["GREENTIC_SLACK_PORT"])), Handler)
    append_event("server-started", {"port": int(os.environ["GREENTIC_SLACK_PORT"])})
    server.serve_forever()
PY

SERVER_LOG="${WORK_DIR}/server.log"
CLOUDFLARED_LOG="${WORK_DIR}/cloudflared.log"
PUBLIC_URL_FILE="${WORK_DIR}/public-url.txt"

GREENTIC_ROOT="${ROOT_DIR}" \
GREENTIC_SLACK_WORK="${WORK_DIR}" \
GREENTIC_TESTER_BIN="${TESTER_BIN}" \
GREENTIC_SLACK_PORT="${PORT}" \
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
echo "Slack tester UI: ${LOCAL_URL}"
echo "Public URL: ${PUBLIC_URL}"
echo "Slack ingress URL: ${PUBLIC_URL}/v1/messaging/ingress/messaging-slack/default/default"
echo "Logs: ${WORK_DIR}"

if [ "${OPEN_BROWSER}" -eq 1 ]; then
  if command -v xdg-open >/dev/null 2>&1; then
    xdg-open "${LOCAL_URL}" >/dev/null 2>&1 || true
  elif command -v open >/dev/null 2>&1; then
    open "${LOCAL_URL}" >/dev/null 2>&1 || true
  fi
fi

wait "${SERVER_PID}"
