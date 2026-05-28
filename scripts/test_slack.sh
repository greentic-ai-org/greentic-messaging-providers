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

import html
import json
import os
import subprocess
import threading
import time
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
from pathlib import Path
from urllib.error import HTTPError, URLError
from urllib.parse import parse_qs, urlencode, urlparse
from urllib.request import Request, urlopen


ROOT = Path(os.environ["GREENTIC_ROOT"]).resolve()
WORK = Path(os.environ["GREENTIC_SLACK_WORK"]).resolve()
TESTER = Path(os.environ["GREENTIC_TESTER_BIN"]).resolve()
VALUES = WORK / "slack-values.json"
EVENTS = WORK / "events.jsonl"
APP_INFO = WORK / "slack-app-info.json"
APP_NAME = WORK / "slack-app-name.txt"
PUBLIC_URL_FILE = WORK / "public-url.txt"
ADD_TO_SLACK_ACTION = {
    "id": "add_to_slack",
    "title": "Add to Slack",
    "kind": "oauth_authorize",
    "provider_id": "slack",
    "oauth_provider_id": "slack",
    "authorize_url": "https://slack.com/oauth/v2/authorize",
    "redirect_path": "/oauth/callback/slack",
    "scopes": ["chat:write", "channels:read", "channels:history", "channels:join", "im:history", "im:write"],
}

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


def read_json_file(path: Path) -> dict:
    try:
        return json.loads(path.read_text(encoding="utf-8"))
    except (FileNotFoundError, json.JSONDecodeError):
        return {}


def write_json_file(path: Path, data: dict) -> None:
    path.write_text(json.dumps(data, indent=2, ensure_ascii=False) + "\n", encoding="utf-8")


def app_display_name() -> str:
    try:
        name = APP_NAME.read_text(encoding="utf-8").strip()
        if name:
            return name
    except FileNotFoundError:
        pass
    bundle_name = os.environ.get("GREENTIC_BUNDLE_NAME", "").strip()
    if bundle_name:
        name = f"{bundle_name} Slack"
    else:
        name = f"Greentic Slack {time.strftime('%H%M%S', time.gmtime())}"
    APP_NAME.write_text(name + "\n", encoding="utf-8")
    return name


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


def first_nested(data: dict, paths: list[list[str]]) -> str:
    for path in paths:
        cur = data
        for key in path:
            if not isinstance(cur, dict) or key not in cur:
                cur = None
                break
            cur = cur[key]
        if isinstance(cur, str) and cur:
            return cur
    return ""


def slack_api_post(method: str, *, token: str = "", payload: dict | None = None, form: dict | None = None) -> dict:
    if form is not None:
        body = urlencode({k: v for k, v in form.items() if v not in ("", None)}).encode("utf-8")
        headers = {"content-type": "application/x-www-form-urlencoded"}
    else:
        body = json.dumps(payload or {}).encode("utf-8")
        headers = {"content-type": "application/json; charset=utf-8"}
    if token:
        headers["authorization"] = f"Bearer {token}"
    req = Request(f"https://slack.com/api/{method}", data=body, headers=headers, method="POST")
    try:
        with urlopen(req, timeout=45) as res:
            raw = res.read().decode("utf-8", errors="replace")
    except HTTPError as exc:
        raw = exc.read().decode("utf-8", errors="replace")
        raise RuntimeError(f"Slack API {method} HTTP {exc.code}: {raw}") from exc
    except URLError as exc:
        raise RuntimeError(f"Slack API {method} failed: {exc}") from exc
    try:
        data = json.loads(raw)
    except json.JSONDecodeError as exc:
        raise RuntimeError(f"Slack API {method} returned non-JSON: {raw}") from exc
    if data.get("ok") is False:
        raise RuntimeError(f"Slack API {method} failed: {json.dumps(data, indent=2)}")
    return data


def rotate_configuration_token(refresh_token: str) -> dict:
    if not refresh_token:
        return {}
    rotated = slack_api_post(
        "tooling.tokens.rotate",
        form={"refresh_token": refresh_token},
    )
    return {
        "access_token": rotated.get("token") or rotated.get("access_token") or "",
        "refresh_token": rotated.get("refresh_token") or refresh_token,
        "response": rotated,
    }


def current_configuration_tokens(data: dict) -> tuple[str, str]:
    access = data.get("slack_configuration_access_token") or ""
    refresh = data.get("slack_configuration_refresh_token") or ""
    return access, refresh


def slack_manifest() -> dict:
    base = public_url().rstrip("/")
    ingress_url = f"{base}/v1/messaging/ingress/messaging-slack/default/default"
    callback_url = f"{base}{ADD_TO_SLACK_ACTION['redirect_path']}"
    name = app_display_name()
    return {
        "display_information": {
            "name": name,
        },
        "features": {
            "bot_user": {
                "display_name": name,
                "always_online": False,
            },
            "app_home": {
                "messages_tab_enabled": True,
                "messages_tab_read_only_enabled": False,
            },
        },
        "oauth_config": {
            "redirect_urls": [callback_url],
            "scopes": {
                "bot": ADD_TO_SLACK_ACTION["scopes"],
            },
        },
        "settings": {
            "event_subscriptions": {
                "request_url": ingress_url,
                "bot_events": ["message.im"],
            },
            "interactivity": {
                "is_enabled": True,
                "request_url": ingress_url,
            },
            "org_deploy_enabled": False,
            "socket_mode_enabled": False,
            "token_rotation_enabled": False,
        },
    }


def normalize_app_info(created: dict, access_token: str, refresh_token: str) -> dict:
    app_id = first_nested(created, [
        ["app_id"],
        ["app", "id"],
        ["app", "app_id"],
        ["manifest", "app_id"],
    ])
    client_id = first_nested(created, [
        ["credentials", "client_id"],
        ["app", "credentials", "client_id"],
        ["app", "oauth_config", "client_id"],
        ["oauth_config", "client_id"],
        ["client_id"],
    ])
    client_secret = first_nested(created, [
        ["credentials", "client_secret"],
        ["app", "credentials", "client_secret"],
        ["app", "oauth_config", "client_secret"],
        ["oauth_config", "client_secret"],
        ["client_secret"],
    ])
    return {
        "app_id": app_id,
        "client_id": client_id,
        "client_secret": client_secret,
        "oauth_authorize_url": created.get("oauth_authorize_url") or "",
        "created_response": created,
        "manifest": slack_manifest(),
    }


def sanitized_app_info() -> dict:
    info = read_json_file(APP_INFO)
    if not info:
        return {}
    safe = dict(info)
    safe["app_display_name"] = app_display_name()
    for key in ("client_secret", "bot_token"):
        if safe.get(key):
            safe[key] = "set"
    if isinstance(safe.get("oauth_response"), dict):
        oauth = dict(safe["oauth_response"])
        for key in ("access_token", "refresh_token"):
            if oauth.get(key):
                oauth[key] = "set"
        safe["oauth_response"] = oauth
    safe.pop("created_response", None)
    return safe


def install_url() -> str:
    info = read_json_file(APP_INFO)
    client_id = info.get("client_id") or ""
    base = public_url().rstrip("/")
    if not client_id or not base:
        return ""
    if info.get("oauth_authorize_url"):
        parsed = urlparse(info["oauth_authorize_url"])
        query_pairs = {k: v[-1] if v else "" for k, v in parse_qs(parsed.query).items()}
        query_pairs["redirect_uri"] = f"{base}{ADD_TO_SLACK_ACTION['redirect_path']}"
        return parsed._replace(query=urlencode(query_pairs)).geturl()
    query = urlencode({
        "client_id": client_id,
        "scope": ",".join(ADD_TO_SLACK_ACTION["scopes"]),
        "redirect_uri": f"{base}{ADD_TO_SLACK_ACTION['redirect_path']}",
    })
    return f"{ADD_TO_SLACK_ACTION['authorize_url']}?{query}"


def slack_app_url() -> str:
    app_id = read_json_file(APP_INFO).get("app_id") or ""
    if not app_id:
        return ""
    return f"https://slack.com/app_redirect?app={app_id}"


def values_from_app_info(data: dict | None = None) -> dict:
    data = data or {}
    info = read_json_file(APP_INFO)
    base = public_url()
    return {
        "config": {
            "api_base": "https://slack.com/api",
            "public_base_url": base,
            "provider_id": "messaging-slack",
            "tenant": data.get("tenant") or first_nested(info, [["team", "id"], ["oauth_response", "team", "id"]]) or "default",
            "team": data.get("team") or first_nested(info, [["team", "id"], ["oauth_response", "team", "id"]]) or "default",
            "default_channel": data.get("default_channel") or None,
        },
        "secrets": {
            "SLACK_BOT_TOKEN": info.get("bot_token") or "",
            "SLACK_APP_ID": info.get("app_id") or "",
        },
        "http": "real",
        "state": {},
    }


def clean_values(values: dict) -> dict:
    values = json.loads(json.dumps(values))
    values["config"] = {k: v for k, v in values["config"].items() if v not in ("", None)}
    values["secrets"] = {k: v for k, v in values["secrets"].items() if v not in ("", None)}
    return values


def persist_values(data: dict | None = None) -> dict:
    values = clean_values(values_from_app_info(data))
    write_json_file(VALUES, values)
    return values


def create_slack_app(data: dict) -> dict:
    access_token, refresh_token = current_configuration_tokens(data)
    if not access_token and refresh_token:
        rotated = rotate_configuration_token(refresh_token)
        access_token = rotated.get("access_token") or access_token
        refresh_token = rotated.get("refresh_token") or refresh_token
        append_event("configuration-token-rotated", {"ok": bool(access_token)})
    if not access_token:
        raise RuntimeError("Configuration access token is required.")
    if not public_url():
        raise RuntimeError("Public URL is not ready yet.")
    payload = {"manifest": json.dumps(slack_manifest())}
    try:
        created = slack_api_post("apps.manifest.create", token=access_token, payload=payload)
    except RuntimeError as exc:
        if refresh_token and "invalid_auth" in str(exc):
            rotated = rotate_configuration_token(refresh_token)
            access_token = rotated.get("access_token") or access_token
            refresh_token = rotated.get("refresh_token") or refresh_token
            append_event("configuration-token-rotated", {"ok": bool(access_token)})
            created = slack_api_post("apps.manifest.create", token=access_token, payload=payload)
        else:
            raise
    info = normalize_app_info(created, access_token, refresh_token)
    write_json_file(APP_INFO, info)
    persist_values(data)
    return info


def exchange_oauth_code(code: str) -> dict:
    info = read_json_file(APP_INFO)
    client_id = info.get("client_id") or ""
    client_secret = info.get("client_secret") or ""
    if not client_id or not client_secret:
        raise RuntimeError("Slack app client_id/client_secret are not available. Create the Slack app first.")
    response = slack_api_post(
        "oauth.v2.access",
        form={
            "client_id": client_id,
            "client_secret": client_secret,
            "code": code,
            "redirect_uri": f"{public_url().rstrip('/')}{ADD_TO_SLACK_ACTION['redirect_path']}",
        },
    )
    info["oauth_response"] = response
    info["bot_token"] = response.get("access_token") or ""
    if response.get("app_id"):
        info["app_id"] = response["app_id"]
    if isinstance(response.get("team"), dict):
        info["team"] = response["team"]
    write_json_file(APP_INFO, info)
    persist_values({})
    return response


def join_public_channel_if_needed(values: dict, target: str, target_kind: str) -> None:
    bot_token = values.get("secrets", {}).get("SLACK_BOT_TOKEN") or ""
    if not bot_token or target_kind != "channel" or not target.startswith("C"):
        return
    try:
        result = slack_api_post("conversations.join", token=bot_token, form={"channel": target})
        append_event("channel-join", {"target": target, "result": result})
    except RuntimeError as exc:
        text = str(exc)
        if "already_in_channel" in text:
            append_event("channel-join", {"target": target, "result": "already_in_channel"})
            return
        append_event("channel-join", {"target": target, "error": text})


def resolve_send_target(values: dict, target: str, target_kind: str) -> tuple[str, str]:
    bot_token = values.get("secrets", {}).get("SLACK_BOT_TOKEN") or ""
    if target_kind == "user" or target.startswith("U"):
        if not target.startswith("U"):
            raise RuntimeError("User DM requires a Slack user ID that starts with U.")
        result = slack_api_post("conversations.open", token=bot_token, form={"users": target})
        channel_id = first_nested(result, [["channel", "id"]])
        if not channel_id:
            raise RuntimeError(f"Slack conversations.open response missing channel id: {json.dumps(result)}")
        append_event("dm-opened", {"user": target, "channel": channel_id})
        return channel_id, "channel"
    if target.startswith("D"):
        raise RuntimeError("D... conversation IDs are app-specific. Use the U... Slack user ID with Destination kind = User DM.")
    return target, target_kind


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
    .actionBox {{ border: 1px solid #cbd5e1; background: #f8fafc; border-radius: 8px; padding: 12px; margin-bottom: 14px; }}
    .primary {{ border-color: #4a154b; background: #4a154b; color: white; }}
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
    <p class="muted">The generated Slack app will use <code>/v1/messaging/ingress/messaging-slack/default/default</code>.</p>
  </section>
  <section>
    <h2>Setup</h2>
    <div class="actionBox">
      <div class="row">
        <button id="createAppBtn" type="button">Create Slack app</button>
        <button id="addToSlackBtn" class="primary" type="button">Add to Slack</button>
        <button id="openSlackAppBtn" type="button">Open in Slack</button>
        <button id="copyAddToSlackBtn" type="button">Copy install URL</button>
        <span class="muted">Create the app first, then install it to retrieve the bot token for sends.</span>
      </div>
      <pre id="addToSlackUrl"></pre>
      <pre id="appInfo">{{}}</pre>
    </div>
    <div class="grid">
      <label>Configuration access token*<input id="slack_configuration_access_token" type="password" autocomplete="off" placeholder="xoxe-..."></label>
      <label>Configuration refresh token*<input id="slack_configuration_refresh_token" type="password" autocomplete="off" placeholder="xoxe-..."></label>
    </div>
    <p class="muted">Other setup values are derived after Slack creates and installs the app.</p>
    <div class="row">
      <button id="saveBtn" type="button">Save values only</button>
    </div>
  </section>
  <section>
    <h2>Send</h2>
    <div class="grid">
      <label>Destination ID<input id="send_to" placeholder="C... for channel, U... for DM"></label>
      <label>Destination kind<select id="send_kind">
        <option value="channel">Channel or conversation</option>
        <option value="user">User DM</option>
      </select></label>
      <label>Message<textarea id="send_text" rows="3">Hello from Greentic Slack tester</textarea></label>
    </div>
    <p class="muted">Use a public channel ID (<code>C...</code>) for channels. Use a Slack user ID (<code>U...</code>) with User DM; the tester opens the bot DM automatically.</p>
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
const setupAction = {json.dumps(ADD_TO_SLACK_ACTION)};
const ids = ["slack_configuration_access_token","slack_configuration_refresh_token","send_to","send_kind","send_text"];
const persistedIds = ["send_to","send_kind","send_text"];
for (const id of ids) {{
  const saved = persistedIds.includes(id) ? localStorage.getItem("slackTester." + id) : null;
  if (saved !== null) document.getElementById(id).value = saved;
  if (persistedIds.includes(id)) {{
    document.getElementById(id).addEventListener("input", e => localStorage.setItem("slackTester." + id, e.target.value));
  }}
}}
function formValues() {{
  return Object.fromEntries(ids.map(id => [id, document.getElementById(id).value.trim()]));
}}
let currentInstallUrl = "";
let currentSlackAppUrl = "";
function refreshAddToSlackUrl(url) {{
  currentInstallUrl = url || "";
  document.getElementById("addToSlackUrl").textContent = currentInstallUrl || "Create a Slack app to generate the install URL.";
  document.getElementById("addToSlackBtn").disabled = !currentInstallUrl;
  document.getElementById("copyAddToSlackBtn").disabled = !currentInstallUrl;
  return url;
}}
function refreshSlackAppUrl(url) {{
  currentSlackAppUrl = url || "";
  document.getElementById("openSlackAppBtn").disabled = !currentSlackAppUrl;
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
  document.getElementById("appInfo").textContent = JSON.stringify(data.app_info || {{}}, null, 2);
  refreshAddToSlackUrl(data.install_url || "");
  refreshSlackAppUrl(data.slack_app_url || "");
}}
document.getElementById("addToSlackBtn").onclick = () => {{
  const url = currentInstallUrl;
  if (!url) return;
  window.open(url, "_blank", "noopener");
}};
document.getElementById("copyAddToSlackBtn").onclick = async () => {{
  const url = currentInstallUrl;
  if (!url) return;
  await navigator.clipboard.writeText(url);
}};
document.getElementById("openSlackAppBtn").onclick = () => {{
  if (!currentSlackAppUrl) return;
  window.open(currentSlackAppUrl, "_blank", "noopener");
}};
document.getElementById("createAppBtn").onclick = async e => {{ e.target.disabled = true; try {{ await post("/api/create-app", formValues()); await refresh(); }} finally {{ e.target.disabled = false; }} }};
document.getElementById("saveBtn").onclick = () => post("/api/save", formValues());
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
        parsed = urlparse(self.path)
        if parsed.path == "/" or parsed.path == "/index.html":
            body = page_html()
            self.send_response(200)
            self.send_header("content-type", "text/html; charset=utf-8")
            self.send_header("content-length", str(len(body)))
            self.end_headers()
            self.wfile.write(body)
            return
        if parsed.path == "/oauth/callback/slack":
            query = {k: v[-1] if v else "" for k, v in parse_qs(parsed.query).items()}
            result = {}
            error = ""
            if query.get("code"):
                try:
                    result = exchange_oauth_code(query["code"])
                except Exception as exc:
                    error = str(exc)
            append_event("oauth-callback", {"query": query, "result": result, "error": error})
            status_text = "Installed Slack app and stored bot token." if result and not error else (error or "Missing OAuth code.")
            body = f"""<!doctype html>
<html lang="en">
<head><meta charset="utf-8"><title>Slack OAuth Callback</title></head>
<body style="font-family: system-ui, sans-serif; margin: 24px;">
  <h1>Slack OAuth Callback</h1>
  <p>{html.escape(status_text)}</p>
  <pre>{html.escape(json.dumps(query, indent=2))}</pre>
  <pre>{html.escape(json.dumps(result, indent=2))}</pre>
  <p><a href="/">Back to tester</a></p>
</body>
</html>""".encode("utf-8")
            self.send_response(200)
            self.send_header("content-type", "text/html; charset=utf-8")
            self.send_header("content-length", str(len(body)))
            self.end_headers()
            self.wfile.write(body)
            return
        if parsed.path == "/api/events":
            self.send_json({
                "public_url": public_url(),
                "events": read_events(),
                "app_info": sanitized_app_info(),
                "install_url": install_url(),
                "slack_app_url": slack_app_url(),
            })
            return
        self.send_error(404)

    def do_POST(self) -> None:
        path = urlparse(self.path).path
        if path == "/api/save":
            data = self.read_json()
            values = persist_values(data)
            append_event("values-saved", {"values_path": str(VALUES)})
            self.send_json({"ok": True, "values_path": str(VALUES), "values": values})
            return
        if path == "/api/create-app":
            try:
                info = create_slack_app(self.read_json())
                result = {
                    "ok": True,
                    "app_info": sanitized_app_info(),
                    "install_url": install_url(),
                    "slack_app_url": slack_app_url(),
                    "raw_keys": sorted(info.get("created_response", {}).keys()),
                }
                append_event("create-app", {"result": result})
                self.send_json(result)
            except Exception as exc:
                result = {"ok": False, "error": str(exc)}
                append_event("create-app", {"result": result})
                self.send_json(result, 500)
            return
        if path == "/api/send":
            data = self.read_json()
            values = persist_values(data)
            target = data.get("send_to") or values.get("config", {}).get("default_channel") or ""
            target_kind = data.get("send_kind") or "channel"
            if not values.get("secrets", {}).get("SLACK_BOT_TOKEN"):
                result = {
                    "ok": False,
                    "status": 2,
                    "stdout": "",
                    "stderr": "Slack bot token is missing. Create the Slack app, then use Add to Slack to install it first.\n",
                    "json": None,
                }
                append_event("send", {"target": target, "target_kind": target_kind, "result": result})
                self.send_json(result, 400)
                return
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
            try:
                target, target_kind = resolve_send_target(values, target, target_kind)
            except Exception as exc:
                result = {
                    "ok": False,
                    "status": 2,
                    "stdout": "",
                    "stderr": f"{exc}\n",
                    "json": None,
                }
                append_event("send", {"target": target, "target_kind": target_kind, "result": result})
                self.send_json(result, 400)
                return
            join_public_channel_if_needed(values, target, target_kind)
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
