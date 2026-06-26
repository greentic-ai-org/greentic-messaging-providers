#!/usr/bin/env bash
set -euo pipefail

PORT="${PORT:-8793}"
NO_OPEN=0

usage() {
  cat <<'EOF'
Usage: scripts/test_teams_bot.sh [--port <port>] [--no-open]

Starts a local Greentic Teams tester UI with a cloudflared public URL. The
default runtime publishes a native Teams bot app. Teams routes bot messages
through the Bot Framework-compatible service registered for the configured bot
app ID; the Teams app package itself does not carry a messaging endpoint.

  /v1/messaging/ingress/messaging-teams/{tenant}/{team}
  /v1/messaging/webchat/{tenant}/v3/directline/*

It can:
- expose the current Teams Bot messaging endpoint
- publish/install a native Teams bot app
- receive Teams Bot Framework message/invoke activities
- send a Teams Adaptive Card through the Bot Connector API
- optionally receive webchat messages and Action.Submit-style card actions
- optionally send Adaptive Cards through the local Greentic/Webchat conversation
- optionally reconcile the legacy Azure Bot messagingEndpoint from the current public URL
- normalize activities into Greentic-style envelopes

Environment:
  CLOUDFLARED_BIN
  GREENTIC_TEAMS_BOT_APP_ID
  GREENTIC_TEAMS_BOT_APP_PASSWORD
  GREENTIC_TEAMS_BOT_TENANT
  GREENTIC_TEAMS_BOT_TEAM
  GREENTIC_TEAMS_BOT_FRAMEWORK=none|botbuilder-node
  GREENTIC_TEAMS_BOT_FRAMEWORK_PORT

Optional Azure Bot reconciliation:
  AZURE_SUBSCRIPTION_ID
  AZURE_RESOURCE_GROUP
  AZURE_BOT_NAME
  AZURE_MANAGEMENT_TOKEN
  GRAPH_SETUP_CLIENT_ID
  AZURE_SETUP_CLIENT_ID

If AZURE_MANAGEMENT_TOKEN is omitted, the tester can acquire one with:
  AZURE_TENANT_ID
  AZURE_SETUP_CLIENT_ID

For Graph app registration, the tester defaults GRAPH_SETUP_CLIENT_ID to the
Microsoft Graph PowerShell public client. Azure management login defaults
AZURE_SETUP_CLIENT_ID to the Azure CLI public client.
EOF
}

while [[ $# -gt 0 ]]; do
  case "$1" in
    --port)
      PORT="${2:?--port requires a value}"
      shift 2
      ;;
    --port=*)
      PORT="${1#--port=}"
      shift
      ;;
    --no-open)
      NO_OPEN=1
      shift
      ;;
    -h|--help)
      usage
      exit 0
      ;;
    *)
      echo "unknown argument: $1" >&2
      usage >&2
      exit 2
      ;;
  esac
done

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
CLOUDFLARED_BIN="${CLOUDFLARED_BIN:-cloudflared}"
WORK_DIR="${TMPDIR:-/tmp}/greentic-teams-bot-test-${PORT}"
LOCAL_URL="http://localhost:${PORT}"
BOTFRAMEWORK_PORT="${GREENTIC_TEAMS_BOT_FRAMEWORK_PORT:-$((PORT + 1))}"
BOTFRAMEWORK_SDK="${GREENTIC_TEAMS_BOT_FRAMEWORK:-none}"

mkdir -p "${WORK_DIR}"

cat > "${WORK_DIR}/server.py" <<'PY'
import base64
import html
import io
import json
import os
import re
import shutil
import struct
import subprocess
import time
import uuid
import zipfile
import zlib
from datetime import datetime, timezone
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
from pathlib import Path
from threading import Thread
from urllib.error import HTTPError, URLError
from urllib.parse import parse_qs, quote, urlparse
from urllib.request import Request, urlopen

WORK = Path(os.environ["WORK_DIR"])
ROOT_DIR = Path(os.environ.get("ROOT_DIR", "."))
PORT = int(os.environ["PORT"])
LOCAL_URL = os.environ["LOCAL_URL"]
BOTFRAMEWORK_SDK_URL = os.environ.get("BOTFRAMEWORK_SDK_URL", "").strip()
PUBLIC_URL_FILE = WORK / "public-url.txt"
VALUES = WORK / "teams-bot-values.json"
EVENTS = WORK / "teams-bot-events.jsonl"


def now_iso():
    return datetime.now(timezone.utc).replace(microsecond=0).isoformat().replace("+00:00", "Z")


def read_json(path, default):
    try:
        return json.loads(path.read_text(encoding="utf-8"))
    except (FileNotFoundError, json.JSONDecodeError):
        return default


def write_json(path, value):
    path.write_text(json.dumps(value, indent=2) + "\n", encoding="utf-8")


def mark_server_started():
    values = state()
    values["server_started_at"] = now_iso()
    values["last_activity"] = None
    values["last_envelope"] = None
    setup_result = values.get("last_setup_result")
    if isinstance(setup_result, dict) and setup_result.get("step") == "greentic_bot_service_ready":
        values["last_setup_result"] = None
    save_state(values)
    append_event("server-started", {"port": PORT, "server_started_at": values["server_started_at"]})


def public_url():
    try:
        return PUBLIC_URL_FILE.read_text(encoding="utf-8").strip()
    except FileNotFoundError:
        return ""


def ingress_url(config=None):
    config = config or state()["config"]
    base = public_url() or LOCAL_URL
    tenant = (config.get("tenant") or "default").strip("/") or "default"
    team = (config.get("team") or "default").strip("/") or "default"
    return f"{base}/v1/messaging/ingress/messaging-teams/{tenant}/{team}"


def public_ingress_url(config=None):
    base = public_url()
    if not base.startswith("https://"):
        return ""
    config = config or state()["config"]
    tenant = (config.get("tenant") or "default").strip("/") or "default"
    team = (config.get("team") or "default").strip("/") or "default"
    return f"{base}/v1/messaging/ingress/messaging-teams/{tenant}/{team}"


def default_values():
    return {
        "config": {
            "runtime_provider": os.environ.get("GREENTIC_TEAMS_RUNTIME_PROVIDER", "greentic-teams-bot"),
            "tenant": os.environ.get("GREENTIC_TEAMS_BOT_TENANT", "default"),
            "team": os.environ.get("GREENTIC_TEAMS_BOT_TEAM", "default"),
            "teams_app_id": os.environ.get("GREENTIC_TEAMS_APP_ID", ""),
            "teams_app_version": os.environ.get("GREENTIC_TEAMS_APP_VERSION", "1.0.0"),
            "bot_app_id": os.environ.get("GREENTIC_TEAMS_BOT_APP_ID", ""),
            "bot_app_password": os.environ.get("GREENTIC_TEAMS_BOT_APP_PASSWORD", ""),
            "bot_display_name": os.environ.get("GREENTIC_TEAMS_BOT_DISPLAY_NAME", "Greentic Teams Bot"),
            "bot_token_endpoint": "https://login.microsoftonline.com/botframework.com/oauth2/v2.0/token",
            "bot_token_scope": "https://api.botframework.com/.default",
            "azure_auth_tenant": os.environ.get("AZURE_TENANT_ID", "organizations"),
            "graph_setup_client_id": os.environ.get("GRAPH_SETUP_CLIENT_ID", "14d82eec-204b-4c2f-b7e8-296a70dab67e"),
            "azure_setup_client_id": os.environ.get("AZURE_SETUP_CLIENT_ID", "04b07795-8ddb-461a-bbee-02f9e1bf7b46"),
            "azure_subscription_id": os.environ.get("AZURE_SUBSCRIPTION_ID", ""),
            "azure_resource_group": os.environ.get("AZURE_RESOURCE_GROUP", ""),
            "azure_resource_group_location": os.environ.get("AZURE_RESOURCE_GROUP_LOCATION", "westeurope"),
            "azure_bot_name": os.environ.get("AZURE_BOT_NAME", ""),
            "azure_location": os.environ.get("AZURE_LOCATION", "global"),
        },
        "last_activity": None,
        "last_activity_received_at": None,
        "last_envelope": None,
        "server_started_at": None,
        "last_webchat_conversation": None,
        "webchat_conversations": {},
        "last_send": None,
        "last_reconcile": None,
        "last_oauth": None,
        "last_app_registration": None,
        "last_azure_discovery": None,
        "last_teams_app_publish": None,
        "last_teams_app_install": None,
        "last_setup_result": None,
    }


def state():
    values = read_json(VALUES, default_values())
    base = default_values()
    base["config"].update(values.get("config") or {})
    for key in ("last_activity", "last_activity_received_at", "last_envelope", "server_started_at", "last_webchat_conversation", "webchat_conversations", "last_send", "last_reconcile", "last_oauth", "last_app_registration", "last_azure_discovery", "last_teams_app_publish", "last_teams_app_install", "last_setup_result"):
        base[key] = values.get(key)
    return base


def save_state(values):
    current = state()
    incoming = dict(values.get("config") or {})
    current_config = current["config"]
    for key in values.get("remove_config_keys") or ():
        current_config.pop(key, None)
    for key in list(incoming.keys()):
        value = incoming.get(key)
        if value in ("", "set", None) and current_config.get(key):
            incoming.pop(key, None)
    current_config.update(incoming)
    for key in ("last_activity", "last_activity_received_at", "last_envelope", "server_started_at", "last_webchat_conversation", "webchat_conversations", "last_send", "last_reconcile", "last_oauth", "last_app_registration", "last_azure_discovery", "last_teams_app_publish", "last_teams_app_install", "last_setup_result"):
        if key in values:
            current[key] = values[key]
    write_json(VALUES, current)
    return sanitize(current)


def save_client_state(values):
    clean = dict(values or {})
    config = dict(clean.get("config") or {})
    for key in ("oauth_kind", "oauth_device_code", "oauth_user_code", "azure_management_device_code", "azure_management_user_code"):
        config.pop(key, None)
    clean["config"] = config
    return save_state(clean)


def sanitize(value):
    clone = json.loads(json.dumps(value))
    cfg = clone.get("config") or {}
    if cfg.get("bot_app_password"):
        cfg["bot_app_password"] = "set"
    token = cfg.get("bot_access_token")
    if token:
        cfg["bot_access_token"] = "set"
    for key in ("azure_management_access_token", "graph_access_token", "oauth_device_code", "azure_management_device_code"):
        if cfg.get(key):
            cfg[key] = "set"
    return clone


def append_event(kind, payload):
    event = {"ts": now_iso(), "kind": kind, "payload": payload}
    with EVENTS.open("a", encoding="utf-8") as handle:
        handle.write(json.dumps(event) + "\n")
    return event


def recent_events():
    try:
        lines = EVENTS.read_text(encoding="utf-8").splitlines()[-80:]
    except FileNotFoundError:
        return []
    out = []
    for line in lines:
        try:
            out.append(json.loads(line))
        except json.JSONDecodeError:
            pass
    return out


def json_request(method, url, body=None, headers=None, timeout=25):
    payload = None if body is None else json.dumps(body).encode("utf-8")
    req_headers = {"Content-Type": "application/json"}
    req_headers.update(headers or {})
    request = Request(url, data=payload, headers=req_headers, method=method)
    try:
        with urlopen(request, timeout=timeout) as response:
            raw = response.read()
            text = raw.decode("utf-8", errors="replace")
            try:
                parsed = json.loads(text) if text else None
            except json.JSONDecodeError:
                parsed = text
            return {"ok": True, "status": response.status, "body": parsed}
    except HTTPError as error:
        raw = error.read()
        text = raw.decode("utf-8", errors="replace")
        try:
            parsed = json.loads(text) if text else None
        except json.JSONDecodeError:
            parsed = text
        return {"ok": False, "status": error.code, "body": parsed}
    except URLError as error:
        return {"ok": False, "status": 0, "error": str(error)}


def binary_request(method, url, payload, headers=None, timeout=25):
    req_headers = {}
    req_headers.update(headers or {})
    request = Request(url, data=payload, headers=req_headers, method=method)
    try:
        with urlopen(request, timeout=timeout) as response:
            raw = response.read()
            text = raw.decode("utf-8", errors="replace")
            try:
                parsed = json.loads(text) if text else None
            except json.JSONDecodeError:
                parsed = text
            return {"ok": True, "status": response.status, "body": parsed}
    except HTTPError as error:
        raw = error.read()
        text = raw.decode("utf-8", errors="replace")
        try:
            parsed = json.loads(text) if text else None
        except json.JSONDecodeError:
            parsed = text
        return {"ok": False, "status": error.code, "body": parsed}
    except URLError as error:
        return {"ok": False, "status": 0, "error": str(error)}


def form_request(url, form, timeout=25):
    encoded = "&".join(
        f"{url_escape(str(key))}={url_escape(str(value))}" for key, value in form.items()
    ).encode("utf-8")
    request = Request(
        url,
        data=encoded,
        headers={"Content-Type": "application/x-www-form-urlencoded"},
        method="POST",
    )
    try:
        with urlopen(request, timeout=timeout) as response:
            return {"ok": True, "status": response.status, "body": json.loads(response.read())}
    except HTTPError as error:
        raw = error.read().decode("utf-8", errors="replace")
        try:
            body = json.loads(raw)
        except json.JSONDecodeError:
            body = raw
        return {"ok": False, "status": error.code, "body": body}
    except URLError as error:
        return {"ok": False, "status": 0, "error": str(error)}


def url_escape(value):
    from urllib.parse import quote_plus
    return quote_plus(value)


def bearer_result_for_log(result):
    safe = dict(result)
    body = safe.get("body")
    if isinstance(body, dict) and body.get("access_token"):
        body = dict(body)
        body["access_token"] = "set"
        if body.get("refresh_token"):
            body["refresh_token"] = "set"
        safe["body"] = body
    if safe.get("access_token"):
        safe["access_token"] = "set"
    return safe


def oauth_device_urls(config):
    tenant = (config.get("azure_auth_tenant") or "organizations").strip() or "organizations"
    return {
        "device_code_url": f"https://login.microsoftonline.com/{tenant}/oauth2/v2.0/devicecode",
        "token_url": f"https://login.microsoftonline.com/{tenant}/oauth2/v2.0/token",
    }


def oauth_scope(kind):
    if kind == "graph":
        return "Application.ReadWrite.All Directory.ReadWrite.All AppCatalog.ReadWrite.All TeamsAppInstallation.ReadWriteSelfForUser TeamsAppInstallation.ReadForUser"
    if kind == "management":
        return "https://management.azure.com/user_impersonation"
    raise ValueError(f"unknown oauth kind: {kind}")


def oauth_client_id(kind, config):
    if kind == "graph":
        return (config.get("graph_setup_client_id") or "").strip()
    return (config.get("azure_setup_client_id") or "").strip()


def oauth_device_code_key(kind):
    return "oauth_device_code" if kind == "graph" else "azure_management_device_code"


def oauth_user_code_key(kind):
    return "oauth_user_code" if kind == "graph" else "azure_management_user_code"


def start_oauth_device(kind, config):
    urls = oauth_device_urls(config)
    client_id = oauth_client_id(kind, config)
    if not client_id:
        return {"ok": False, "error": f"{'graph' if kind == 'graph' else 'azure'} setup client id is required"}
    result = form_request(urls["device_code_url"], {
        "client_id": client_id,
        "scope": oauth_scope(kind),
    })
    if result.get("ok") and isinstance(result.get("body"), dict):
        values = state()
        values["config"]["oauth_kind"] = kind
        values["config"][oauth_device_code_key(kind)] = result["body"].get("device_code")
        values["config"][oauth_user_code_key(kind)] = result["body"].get("user_code")
        values["last_oauth"] = {"kind": kind, "started": now_iso(), "response": result["body"]}
        save_state(values)
    append_event("oauth-device-start", {"kind": kind, "result": bearer_result_for_log(result)})
    return result


def complete_oauth_device(kind, config):
    urls = oauth_device_urls(config)
    client_id = oauth_client_id(kind, config)
    device_code = (config.get(oauth_device_code_key(kind)) or "").strip()
    if not client_id or not device_code:
        return {"ok": False, "error": "start device login first"}
    result = form_request(urls["token_url"], {
        "grant_type": "urn:ietf:params:oauth:grant-type:device_code",
        "client_id": client_id,
        "device_code": device_code,
    })
    if result.get("ok") and isinstance(result.get("body"), dict):
        token = result["body"].get("access_token")
        values = state()
        if kind == "graph":
            values["config"]["graph_access_token"] = token
        else:
            values["config"]["azure_management_access_token"] = token
        values["last_oauth"] = {"kind": kind, "completed": now_iso(), "result": bearer_result_for_log(result)}
        values["remove_config_keys"] = [oauth_device_code_key(kind), oauth_user_code_key(kind), "oauth_kind"]
        save_state(values)
    append_event("oauth-device-complete", {"kind": kind, "result": bearer_result_for_log(result)})
    return bearer_result_for_log(result)


def oauth_token_key(kind):
    return "graph_access_token" if kind == "graph" else "azure_management_access_token"


def clear_pending_oauth(values=None):
    values = values or {}
    values["remove_config_keys"] = ["oauth_device_code", "oauth_user_code", "azure_management_device_code", "azure_management_user_code", "oauth_kind"]
    save_state(values)
    return state()


def update_config_fields(fields):
    values = state()
    values["config"].update({k: v for k, v in fields.items() if v not in (None, "")})
    save_state(values)
    return state()["config"]


def remember_setup_result(result):
    values = state()
    values["last_setup_result"] = result
    save_state(values)
    return result


def oauth_error_code(result):
    body = result.get("body")
    codes = body.get("error_codes") if isinstance(body, dict) else None
    if isinstance(codes, list) and codes:
        return codes[0]
    return None


def oauth_error_name(result):
    body = result.get("body")
    if isinstance(body, dict):
        return body.get("error")
    return None


def oauth_device_code_invalid(result):
    return oauth_error_name(result) in ("expired_token", "authorization_declined", "invalid_grant") or oauth_error_code(result) in (70020, 7000014)


def pending_oauth_result(kind, result):
    values = state()
    cfg = values.get("config") or {}
    last = values.get("last_oauth") or {}
    response = last.get("response") if isinstance(last, dict) else {}
    body = dict(result.get("body") or {}) if isinstance(result.get("body"), dict) else {}
    if isinstance(response, dict):
        for key in ("verification_uri", "verification_url", "user_code", "message", "expires_in", "interval"):
            if response.get(key) and not body.get(key):
                body[key] = response[key]
    user_code = cfg.get(oauth_user_code_key(kind))
    if user_code and not body.get("user_code"):
        body["user_code"] = user_code
    return {
        "ok": True,
        "step": f"wait_for_{kind}_login",
        "next": "finish the browser authorization, then click this button again",
        "result": {**result, "body": body},
    }


def restart_oauth_device(kind, previous):
    clear_pending_oauth()
    result = start_oauth_device(kind, state()["config"])
    return {
        "ok": result.get("ok", False),
        "step": f"restart_{kind}_login",
        "next": "authorize the new device code in the opened browser, then click this button again",
        "expired_result": previous,
        "result": result,
    }


def start_or_resume_oauth_device(kind, step, next_text, reason, previous):
    cfg = state()["config"]
    if (cfg.get("oauth_kind") or "").strip() == kind and (cfg.get(oauth_device_code_key(kind)) or "").strip():
        result = pending_oauth_result(kind, {"ok": False, "status": 202, "body": {"error": "authorization_pending"}})
        result["reason"] = reason
        result["previous"] = previous
        return result
    result = start_oauth_device(kind, cfg)
    return {
        "ok": result.get("ok", False),
        "step": step,
        "next": next_text,
        "reason": reason,
        "previous": previous,
        "result": result,
    }


def remove_config_keys(*keys):
    values = state()
    values["remove_config_keys"] = list(keys)
    save_state(values)
    return state()


def slugify_resource_name(value, fallback="greentic-teams-bot", max_len=54):
    slug = re.sub(r"[^a-z0-9-]+", "-", (value or "").lower())
    slug = re.sub(r"-+", "-", slug).strip("-")
    if not slug:
        slug = fallback
    if not re.match(r"^[a-z]", slug):
        slug = f"greentic-{slug}"
    return slug[:max_len].strip("-") or fallback


def azure_request(method, path, token, body=None):
    return json_request(
        method,
        f"https://management.azure.com{path}",
        body,
        headers={"Authorization": f"Bearer {token}"},
    )


def azure_error_message(result):
    body = result.get("body") if isinstance(result, dict) else None
    error = body.get("error") if isinstance(body, dict) else None
    if isinstance(error, dict):
        return error.get("message") or error.get("code") or ""
    return ""


def azure_auth_expired(result):
    if not isinstance(result, dict):
        return False
    body = result.get("body") if "body" in result else result.get("response")
    error = body.get("error") if isinstance(body, dict) else None
    code = error.get("code") if isinstance(error, dict) else None
    return result.get("status") == 401 or result.get("http_status") == 401 or code == "ExpiredAuthenticationToken"


def start_management_login_after_expiry(config, previous):
    remove_config_keys("azure_management_access_token")
    return start_or_resume_oauth_device(
        "management",
        "start_azure_management_login",
        "authorize Azure management in the opened browser, then click again",
        "saved Azure management token expired",
        previous,
    )


def graph_auth_expired(result):
    if not isinstance(result, dict):
        return False
    body = result.get("body") if "body" in result else result.get("response")
    error = body.get("error") if isinstance(body, dict) else None
    code = error.get("code") if isinstance(error, dict) else None
    return result.get("status") == 401 or code in ("InvalidAuthenticationToken", "Authentication_ExpiredToken")


def graph_auth_needs_login(result):
    if graph_auth_expired(result):
        return True
    return False


def graph_permission_denied(result):
    if not isinstance(result, dict) or result.get("status") != 403:
        return False
    body = result.get("body") if isinstance(result.get("body"), dict) else {}
    error = body.get("error") if isinstance(body.get("error"), dict) else {}
    return (error.get("code") or "").lower() == "forbidden" or "not authorized" in (error.get("message") or "").lower()


def start_graph_login_after_expiry(config, previous):
    remove_config_keys("graph_access_token")
    return start_or_resume_oauth_device(
        "graph",
        "start_graph_login",
        "authorize Graph in the opened browser, then click again",
        "saved Graph token expired",
        previous,
    )


def azure_resource_group_from_id(resource_id):
    match = re.search(r"/resourceGroups/([^/]+)", resource_id or "", re.IGNORECASE)
    return match.group(1) if match else ""


def discover_existing_azure_bot(config, access_token, subscription_id, force=False):
    cfg = state()["config"]
    current_group = (cfg.get("azure_resource_group") or config.get("azure_resource_group") or "").strip()
    current_bot = (cfg.get("azure_bot_name") or config.get("azure_bot_name") or "").strip()
    if current_group and current_bot and not force:
        return {"ok": True, "skipped": True, "reason": "Azure Bot target is already selected"}

    result = azure_request(
        "GET",
        f"/subscriptions/{subscription_id}/providers/Microsoft.BotService/botServices?api-version=2022-09-15",
        access_token,
    )
    if azure_auth_expired(result):
        return start_management_login_after_expiry(config, {**result, "step": "discover_bot_resources"})
    if not result.get("ok"):
        return {**result, "step": "discover_bot_resources"}

    items = (result.get("body") or {}).get("value") or []
    if not items:
        return {"ok": False, "step": "discover_bot_resources", "error": "no Azure Bot resources found in the selected subscription"}

    app_id = (cfg.get("bot_app_id") or config.get("bot_app_id") or "").strip().lower()
    bot_name = current_bot.lower()

    def score(item):
        name = (item.get("name") or "").strip().lower()
        props = item.get("properties") or {}
        msa_app_id = (props.get("msaAppId") or "").strip().lower()
        if app_id and msa_app_id == app_id:
            return 100
        if bot_name and name == bot_name:
            return 90
        if "greentic" in name:
            return 50
        return 10 if len(items) == 1 else 0

    chosen = max(items, key=score)
    if score(chosen) <= 0:
        names = [item.get("name") for item in items if item.get("name")]
        return {
            "ok": False,
            "step": "discover_bot_resources",
            "error": "multiple Azure Bot resources found; fill Azure Bot name manually",
            "candidates": names,
        }

    resource_group = azure_resource_group_from_id(chosen.get("id"))
    name = chosen.get("name")
    if not (resource_group and name):
        return {"ok": False, "step": "discover_bot_resources", "error": "Azure Bot resource response did not include resource group and name"}

    values = state()
    values["config"]["azure_resource_group"] = resource_group
    values["config"]["azure_bot_name"] = name
    if chosen.get("location"):
        values["config"]["azure_location"] = chosen.get("location")
    props = chosen.get("properties") or {}
    msa_app_id = (props.get("msaAppId") or "").strip()
    previous_app_id = (values["config"].get("bot_app_id") or "").strip()
    if msa_app_id and msa_app_id != previous_app_id:
        values["config"]["bot_app_id"] = msa_app_id
        values["config"].pop("bot_app_password", None)
    action = {
        "action": "selected_existing_bot_resource",
        "resource_group": resource_group,
        "azure_bot_name": name,
        "bot_app_id": msa_app_id or None,
        "replaced_bot_app_id": previous_app_id if msa_app_id and msa_app_id != previous_app_id else None,
        "candidates": len(items),
    }
    values["last_azure_discovery"] = {"ok": True, "actions": [action]}
    save_state(values)
    append_event("azure-bot-resource-discovered", action)
    return {"ok": True, "actions": [action], "config": sanitize(state())["config"]}


def rediscover_azure_bot_after_scope_failure(config, previous):
    token = azure_management_token()
    if not token.get("ok"):
        return token
    subscription_id = (config.get("azure_subscription_id") or os.environ.get("AZURE_SUBSCRIPTION_ID") or "").strip()
    if not subscription_id:
        return previous
    result = discover_existing_azure_bot(config, token["access_token"], subscription_id, force=True)
    if result.get("ok"):
        return {
            "ok": True,
            "step": "discover_azure_bot_resource",
            "next": "click again to update the discovered Azure Bot endpoint",
            "reason": "previous Azure Bot resource scope was invalid or unauthorized",
            "previous": previous,
            "result": result,
        }
    return previous


def azure_rbac_guidance(result, config, attempted_action=None):
    if not isinstance(result, dict) or result.get("status") != 403:
        return None
    message = azure_error_message(result)
    if not message:
        return None
    subscription_id = (config.get("azure_subscription_id") or "").strip()
    resource_group = (config.get("azure_resource_group") or "").strip()
    bot_name = (config.get("azure_bot_name") or "").strip()
    missing_action = attempted_action
    match = re.search(r"perform action '([^']+)'", message)
    if match:
        missing_action = match.group(1)
    scope = (
        f"/subscriptions/{subscription_id}/resourceGroups/{resource_group}"
        if subscription_id and resource_group
        else "the target Azure subscription or resource group"
    )
    return {
        "title": "Azure RBAC permission required",
        "summary": "The Microsoft sign-in succeeded, but this Azure user cannot create or update the Azure Bot resource.",
        "missing_action": missing_action or "Microsoft.BotService/botServices/write",
        "recommended_role": "Contributor on the target resource group, or a custom role with Bot Service write permissions",
        "recommended_scope": scope,
        "bot_resource": bot_name,
        "resource_group": resource_group,
        "subscription_id": subscription_id,
        "next": "Ask an Azure admin to grant the role above, then restart Azure management login to refresh credentials and click Run next setup step again.",
        "raw_message": message,
    }


def checklist_item(label, done, detail=None, blocked=False):
    return {
        "label": label,
        "state": "blocked" if blocked else ("done" if done else "pending"),
        "detail": detail,
    }


def is_webchat_runtime(config=None):
    config = config or state()["config"]
    return (config.get("runtime_provider") or "greentic-teams-bot").strip().lower() in (
        "greentic-webchat",
        "webchat",
    )


def is_legacy_azure_runtime(config=None):
    config = config or state()["config"]
    return (config.get("runtime_provider") or "").strip().lower() in (
        "azure-bot",
        "legacy-azure-bot",
        "botframework-azure",
    )


def is_greentic_teams_runtime(config=None):
    return not is_webchat_runtime(config) and not is_legacy_azure_runtime(config)


def ensure_teams_app_identity(config=None):
    values = state()
    cfg = values["config"]
    if config:
        cfg.update({k: v for k, v in config.items() if v not in (None, "")})
    if not (cfg.get("teams_app_id") or "").strip():
        cfg["teams_app_id"] = str(uuid.uuid4())
        values["config"] = cfg
        save_state(values)
        append_event("teams-app-id-generated", {"teams_app_id": cfg["teams_app_id"]})
    return cfg["teams_app_id"]


def parse_semver(value):
    match = re.fullmatch(r"(\d+)\.(\d+)\.(\d+)", str(value or "").strip())
    if not match:
        return (1, 0, 0)
    return tuple(int(part) for part in match.groups())


def format_semver(parts):
    return ".".join(str(max(0, int(part))) for part in parts)


def bump_semver(value):
    major, minor, patch = parse_semver(value)
    return format_semver((major, minor, patch + 1))


def teams_app_version(config=None):
    config = config or state()["config"]
    version = (config.get("teams_app_version") or "").strip()
    return version if re.fullmatch(r"\d+\.\d+\.\d+", version) else "1.0.0"


def set_teams_app_version(version):
    values = state()
    values["config"]["teams_app_version"] = version
    save_state(values)
    return version


def webchat_base_url(config=None):
    config = config or state()["config"]
    base = public_url() or LOCAL_URL
    tenant = (config.get("tenant") or "default").strip("/") or "default"
    return f"{base}/v1/messaging/webchat/{tenant}"


def webchat_app_url(config=None):
    config = config or state()["config"]
    base = public_url() or LOCAL_URL
    tenant = quote((config.get("tenant") or "default").strip("/") or "default")
    return f"{base}/webchat/teams?tenant={tenant}"


def setup_status():
    values = state()
    cfg = values["config"]
    webchat_runtime = is_webchat_runtime(cfg)
    legacy_azure_runtime = is_legacy_azure_runtime(cfg)
    greentic_teams_runtime = is_greentic_teams_runtime(cfg)
    ensure_teams_app_identity(cfg)
    reconcile = values.get("last_reconcile") or {}
    current_ingress = ingress_url(cfg)
    if isinstance(reconcile, dict) and reconcile.get("target_messaging_endpoint") != current_ingress:
        reconcile = {}
    teams_publish = values.get("last_teams_app_publish") or {}
    current_app_id = teams_app_id(cfg)
    if teams_publish.get("teams_app_id") != current_app_id or teams_publish.get("manifest_version") != teams_app_version(cfg):
        teams_publish = {}
    teams_install = values.get("last_teams_app_install") or {}
    if teams_install.get("catalog_app_id") != teams_publish.get("catalog_app_id") or teams_install.get("manifest_version") != teams_publish.get("manifest_version"):
        teams_install = {}
    setup_result = values.get("last_setup_result") or {}
    if not legacy_azure_runtime and isinstance(setup_result, dict):
        step = setup_result.get("step") or ""
        if not (
            step.startswith("start_graph")
            or step.startswith("wait_for_graph")
            or step.startswith("restart_graph")
            or step in ("publish_teams_app", "install_teams_app_for_user", "greentic_bot_service_ready")
        ):
            setup_result = {}
    blocked = None
    if isinstance(reconcile, dict) and not reconcile.get("ok"):
        blocked = reconcile.get("admin_guidance")
    if not blocked and isinstance(setup_result, dict):
        result = setup_result.get("result") if isinstance(setup_result.get("result"), dict) else {}
        blocked = result.get("admin_guidance")
    pending_oauth_detail = None
    pending_oauth_kind = (cfg.get("oauth_kind") or "").strip()
    if pending_oauth_kind:
        pending_oauth_code = (cfg.get(oauth_user_code_key(pending_oauth_kind)) or "").strip()
        if pending_oauth_code:
            pending_oauth_detail = f"{pending_oauth_kind} device code {pending_oauth_code}"
    items = [
        checklist_item("Graph admin consent", bool(cfg.get("graph_access_token") or values.get("last_app_registration") or teams_publish.get("ok") or teams_install.get("ok")), pending_oauth_detail if (cfg.get("oauth_kind") == "graph" and not cfg.get("graph_access_token")) else None),
        checklist_item(
            "Teams app identity",
            bool(cfg.get("teams_app_id") or cfg.get("bot_app_id")),
            cfg.get("teams_app_id") or cfg.get("bot_app_id") or None,
        ),
    ]
    if legacy_azure_runtime:
        items.extend([
        checklist_item(
            "Entra bot app registration",
            bool(cfg.get("bot_app_id")),
            cfg.get("bot_app_id") or None,
        ),
        checklist_item("Bot client secret generated", bool(cfg.get("bot_app_password"))),
        checklist_item("Azure management login", bool(cfg.get("azure_management_access_token"))),
        checklist_item(
            "Azure subscription selected",
            bool(cfg.get("azure_subscription_id")),
            cfg.get("azure_subscription_id") or None,
        ),
        checklist_item(
            "Resource group selected",
            bool(cfg.get("azure_resource_group")),
            cfg.get("azure_resource_group") or None,
        ),
        checklist_item(
            "Azure Bot resource name selected",
            bool(cfg.get("azure_bot_name")),
            cfg.get("azure_bot_name") or None,
        ),
        checklist_item(
            "Azure Bot endpoint updated",
            bool(reconcile.get("ok")),
            reconcile.get("target_messaging_endpoint"),
            blocked=bool(blocked),
        ),
        checklist_item(
            "Teams channel enabled",
            bool(((reconcile.get("teams_channel") or {}).get("ok"))),
            None,
            blocked=bool(blocked),
        ),
        ])
    else:
        greentic_registration_seen = bool(
            values.get("last_activity")
            or (
                isinstance(reconcile, dict)
                and reconcile.get("ok")
                and reconcile.get("target_messaging_endpoint") == current_ingress
            )
        )
        items.append(checklist_item(
            "Greentic Bot Service registration" if greentic_teams_runtime else "Greentic Webchat Service ready",
            greentic_registration_seen if greentic_teams_runtime else True,
            "Bot Framework endpoint registered" if greentic_registration_seen else ("Register this bot app ID with the Greentic Bot Service endpoint" if greentic_teams_runtime else webchat_base_url(cfg)),
        ))
        if greentic_teams_runtime:
            items.insert(2, checklist_item(
                "Greentic bot app ID",
                bool(cfg.get("bot_app_id")),
                cfg.get("bot_app_id") or "Use the Greentic Bot Service bot app id",
            ))
    items.extend([
        checklist_item(
            "Teams app published",
            bool(teams_publish.get("ok")),
            teams_app_info(cfg).get("add_to_teams_url") if teams_publish.get("ok") else None,
        ),
        checklist_item(
            "Teams app installed for user",
            bool(teams_install.get("ok")),
            teams_install.get("open_bot_chat_url"),
        ),
        checklist_item(
            "First Greentic chat message received" if webchat_runtime else "First Teams Bot Framework POST received",
            bool(values.get("last_webchat_conversation") if webchat_runtime else values.get("last_activity")),
            "Open Greentic chat and send a message" if webchat_runtime and not values.get("last_webchat_conversation") else ("No Bot Framework POST has reached this tester yet" if not webchat_runtime and not values.get("last_activity") else None),
        ),
    ])
    first_message_received = bool(values.get("last_webchat_conversation") if webchat_runtime else values.get("last_activity"))
    if not first_message_received and teams_install.get("ok"):
        next_text = "Open Greentic chat, send a plain text message, then send the test card." if webchat_runtime else "Open the bot chat link, send a plain text message, then wait for First Teams message received before sending a card."
    else:
        next_text = (setup_result.get("next") if isinstance(setup_result, dict) else None) or "Click Run next setup step."
    return {
        "ok": all(item["state"] == "done" for item in items),
        "items": items,
        "selected": {
            "subscription_id": cfg.get("azure_subscription_id"),
            "resource_group": cfg.get("azure_resource_group"),
            "resource_group_location": cfg.get("azure_resource_group_location"),
            "bot_name": cfg.get("azure_bot_name"),
            "bot_app_id": cfg.get("bot_app_id"),
            "runtime_provider": cfg.get("runtime_provider"),
            "greentic_chat_url": webchat_app_url(cfg) if webchat_runtime else teams_app_info(cfg).get("open_bot_chat_url"),
            "messaging_endpoint": webchat_base_url(cfg) if webchat_runtime else current_ingress,
            "registration_note": "Native Teams cards require the bot app id to be registered with a Bot Framework-compatible service endpoint." if greentic_teams_runtime else None,
        },
        "blocked": blocked,
        "last_step": setup_result.get("step") if isinstance(setup_result, dict) else None,
        "next": next_text,
    }


def discover_azure_defaults(config):
    token = azure_management_token()
    if not token.get("ok"):
        return token
    access_token = token["access_token"]
    values = state()
    cfg = values["config"]
    actions = []

    subscription_id = (cfg.get("azure_subscription_id") or config.get("azure_subscription_id") or "").strip()
    if not subscription_id:
        subscriptions = azure_request("GET", "/subscriptions?api-version=2022-12-01", access_token)
        if azure_auth_expired(subscriptions):
            return start_management_login_after_expiry(config, {**subscriptions, "step": "discover_subscriptions"})
        if not subscriptions.get("ok"):
            return {**subscriptions, "step": "discover_subscriptions"}
        items = (subscriptions.get("body") or {}).get("value") or []
        enabled = [
            item for item in items
            if ((item.get("state") or "").lower() in ("enabled", "warned") or not item.get("state"))
        ]
        chosen = enabled[0] if enabled else (items[0] if items else None)
        if not chosen:
            return {"ok": False, "error": "no Azure subscriptions found for this account"}
        subscription_id = chosen.get("subscriptionId")
        cfg["azure_subscription_id"] = subscription_id
        actions.append({
            "action": "selected_subscription",
            "subscription_id": subscription_id,
            "display_name": chosen.get("displayName"),
            "candidates": len(items),
        })

    location = (cfg.get("azure_resource_group_location") or os.environ.get("AZURE_RESOURCE_GROUP_LOCATION") or "westeurope").strip()
    cfg["azure_resource_group_location"] = location

    resource_group = (cfg.get("azure_resource_group") or config.get("azure_resource_group") or "").strip()
    bot_name = (cfg.get("azure_bot_name") or config.get("azure_bot_name") or "").strip()
    if not (resource_group and bot_name):
        bot_discovery = discover_existing_azure_bot(config, access_token, subscription_id)
        if bot_discovery.get("step") == "start_azure_management_login":
            return bot_discovery
        if bot_discovery.get("ok") and not bot_discovery.get("skipped"):
            return bot_discovery

    if not resource_group:
        default_group = os.environ.get("AZURE_RESOURCE_GROUP") or "greentic-bots"
        groups = azure_request(
            "GET",
            f"/subscriptions/{subscription_id}/resourcegroups?api-version=2021-04-01",
            access_token,
        )
        if azure_auth_expired(groups):
            return start_management_login_after_expiry(config, {**groups, "step": "discover_resource_groups"})
        if not groups.get("ok"):
            if groups.get("status") == 403:
                resource_group = default_group
                actions.append({
                    "action": "assumed_resource_group",
                    "resource_group": resource_group,
                    "reason": "resource group listing was denied; continuing with the default name",
                    "discovery_error": groups.get("body"),
                })
                cfg["azure_resource_group"] = resource_group
            else:
                return {**groups, "step": "discover_resource_groups"}
        if not resource_group:
            items = (groups.get("body") or {}).get("value") or []
            match = next((item for item in items if (item.get("name") or "").lower() == default_group.lower()), None)
            if match:
                resource_group = match.get("name")
                actions.append({"action": "selected_resource_group", "resource_group": resource_group})
            else:
                create = azure_request(
                    "PUT",
                    f"/subscriptions/{subscription_id}/resourcegroups/{default_group}?api-version=2021-04-01",
                    {"location": location},
                    access_token,
                )
                if azure_auth_expired(create):
                    return start_management_login_after_expiry(config, {**create, "step": "create_resource_group", "resource_group": default_group})
                if not create.get("ok"):
                    return {**create, "step": "create_resource_group", "resource_group": default_group}
                resource_group = default_group
                actions.append({"action": "created_resource_group", "resource_group": resource_group, "location": location})
            cfg["azure_resource_group"] = resource_group

    bot_name = (cfg.get("azure_bot_name") or config.get("azure_bot_name") or "").strip()
    if not bot_name:
        bot_name = slugify_resource_name(cfg.get("bot_display_name") or cfg.get("bot_app_id") or "greentic-teams-bot")
        cfg["azure_bot_name"] = bot_name
        actions.append({"action": "derived_bot_name", "azure_bot_name": bot_name})

    cfg.setdefault("azure_location", "global")
    values["last_azure_discovery"] = {"ok": True, "actions": actions}
    save_state(values)
    append_event("azure-defaults-discovered", {"actions": actions})
    return {"ok": True, "actions": actions, "config": sanitize(state())["config"]}


def next_setup_step(config):
    pending_kind = (config.get("oauth_kind") or "").strip()
    pending_device = (config.get(oauth_device_code_key(pending_kind)) or "").strip() if pending_kind else ""
    if not is_legacy_azure_runtime(config) and pending_kind and pending_kind != "graph":
        clear_pending_oauth()
        return next_setup_step(state()["config"])
    if pending_kind and pending_device:
        token_key = oauth_token_key(pending_kind)
        result = complete_oauth_device(pending_kind, config)
        if result.get("ok"):
            return next_setup_step(state()["config"])
        if not result.get("ok") and oauth_error_code(result) == 54005:
            clear_pending_oauth()
            if (state()["config"].get(token_key) or "").strip():
                return next_setup_step(state()["config"])
        if not result.get("ok") and oauth_device_code_invalid(result):
            return restart_oauth_device(pending_kind, result)
        if not result.get("ok") and oauth_error_name(result) in ("authorization_pending", "slow_down"):
            return pending_oauth_result(pending_kind, result)
        return {
            "ok": result.get("ok", False),
            "step": f"complete_{pending_kind}_login",
            "next": "click again to continue setup" if result.get("ok") else "start that login again, then click this button after authorizing",
            "result": result,
        }

    if not is_legacy_azure_runtime(config):
        ensure_teams_app_identity(config)
        if is_greentic_teams_runtime(state()["config"]) and not all((state()["config"].get(key) or "").strip() for key in ("bot_app_id", "bot_app_password")):
            if not (state()["config"].get("graph_access_token") or "").strip():
                result = start_oauth_device("graph", state()["config"])
                return {
                    "ok": result.get("ok", False),
                    "step": "start_graph_login",
                    "next": "authorize Graph in the opened browser, then click again",
                    "result": result,
                }
            result = reconcile_bot_app(state()["config"])
            if graph_auth_expired(result):
                return start_graph_login_after_expiry(state()["config"], result)
            return {
                "ok": result.get("ok", False),
                "step": "create_or_reuse_bot_app",
                "next": "click again to continue setup" if result.get("ok") else "fix app registration error and retry",
                "result": result,
            }
        if is_greentic_teams_runtime(state()["config"]):
            last = state().get("last_reconcile") or {}
            current_endpoint = ingress_url(state()["config"])
            if not (isinstance(last, dict) and last.get("ok") and last.get("target_messaging_endpoint") == current_endpoint):
                if not (state()["config"].get("azure_management_access_token") or "").strip():
                    result = start_oauth_device("management", state()["config"])
                    return {
                        "ok": result.get("ok", False),
                        "step": "start_azure_management_login",
                        "next": "authorize Azure management in the opened browser, then click again",
                        "result": result,
                    }
                missing = [
                    key for key in ("azure_subscription_id", "azure_resource_group", "azure_bot_name")
                    if not (state()["config"].get(key) or "").strip()
                ]
                if missing:
                    result = discover_azure_defaults(state()["config"])
                    if result.get("step") == "start_azure_management_login":
                        return result
                    if not result.get("ok"):
                        return {
                            "ok": False,
                            "step": "discover_azure_defaults",
                            "missing": missing,
                            "next": "fix Azure discovery permissions or fill the missing fields manually, then click again",
                            "result": result,
                        }
                    return {
                        "ok": True,
                        "step": "discover_azure_defaults",
                        "next": "click again to register the bot endpoint for Teams",
                        "result": result,
                    }
                result = reconcile_azure_bot(state()["config"])
                if azure_auth_expired(result):
                    return start_management_login_after_expiry(state()["config"], result)
                if isinstance(result, dict) and result.get("http_status") == 403:
                    rediscovered = rediscover_azure_bot_after_scope_failure(state()["config"], result)
                    if rediscovered is not result:
                        return rediscovered
                return {
                    "ok": result.get("ok", False),
                    "step": "reconcile_bot_framework_registration",
                    "next": "click again to continue setup" if result.get("ok") else "fix Bot Framework registration endpoint and retry",
                    "result": result,
                }
        if not (state()["config"].get("graph_access_token") or "").strip():
            result = start_oauth_device("graph", state()["config"])
            return {
                "ok": result.get("ok", False),
                "step": "start_graph_login",
                "next": "authorize Graph in the opened browser, then click again",
                "result": result,
            }
        if not current_teams_app_publish(state()["config"]).get("ok"):
            publish = publish_teams_app(state()["config"])
            if graph_auth_needs_login(publish):
                return start_graph_login_after_expiry(state()["config"], publish)
            return {
                "ok": publish.get("ok", False),
                "step": "publish_teams_app",
                "next": "open the Teams bot chat link, then send a message" if publish.get("ok") else "fix Teams app catalog publishing and retry",
                "result": publish,
            }
        if not current_teams_app_install(state()["config"]).get("ok"):
            install = install_teams_app_for_me(state()["config"])
            if graph_auth_needs_login(install):
                return start_graph_login_after_expiry(state()["config"], install)
            return {
                "ok": install.get("ok", False),
                "step": "install_teams_app_for_user",
                "next": "open the Teams bot chat and send hello" if install.get("ok") else "fix Teams app user installation and retry",
                "result": install,
            }
        if is_greentic_teams_runtime(state()["config"]):
            return {
                "ok": True,
                "step": "greentic_bot_service_ready",
                "next": "open the Teams bot chat, send hello, then send the test card",
                "result": {
                    "ok": True,
                    "runtime_provider": state()["config"].get("runtime_provider"),
                    "teams_app": teams_app_info(state()["config"]),
                },
            }
        return {
            "ok": True,
            "step": "greentic_bot_service_ready",
            "next": "open Greentic chat, send hello, then send the test card",
            "result": {
                "ok": True,
                "runtime_provider": state()["config"].get("runtime_provider"),
                "webchat_url": webchat_app_url(state()["config"]),
                "directline_base": webchat_base_url(state()["config"]),
                "teams_app": teams_app_info(state()["config"]),
            },
        }

    if not (config.get("bot_app_id") or "").strip() or not (config.get("bot_app_password") or "").strip():
        if not (config.get("graph_access_token") or "").strip():
            result = start_oauth_device("graph", config)
            return {
                "ok": result.get("ok", False),
                "step": "start_graph_login",
                "next": "authorize Graph in the opened browser, then click again",
                "result": result,
            }
        result = reconcile_bot_app(config)
        if graph_auth_expired(result):
            return start_graph_login_after_expiry(config, result)
        return {
            "ok": result.get("ok", False),
            "step": "create_or_reuse_bot_app",
            "next": "click again to continue setup" if result.get("ok") else "fix app registration error and retry",
            "result": result,
        }

    if not (config.get("azure_management_access_token") or "").strip():
        result = start_oauth_device("management", config)
        return {
            "ok": result.get("ok", False),
            "step": "start_azure_management_login",
            "next": "authorize Azure management in the opened browser, then click again",
            "result": result,
        }

    missing = [
        key for key in ("azure_subscription_id", "azure_resource_group", "azure_bot_name")
        if not (config.get(key) or "").strip()
    ]
    if missing:
        result = discover_azure_defaults(config)
        if result.get("step") == "start_azure_management_login":
            return result
        if not result.get("ok"):
            return {
                "ok": False,
                "step": "discover_azure_defaults",
                "missing": missing,
                "next": "fix Azure discovery permissions or fill the missing fields manually, then click again",
                "result": result,
            }
        return {
            "ok": True,
            "step": "discover_azure_defaults",
            "next": "click again to create/update the Azure Bot resource",
            "result": result,
        }

    result = reconcile_azure_bot(config)
    if azure_auth_expired(result):
        return start_management_login_after_expiry(config, result)
    if isinstance(result, dict) and result.get("http_status") == 403:
        rediscovered = rediscover_azure_bot_after_scope_failure(config, result)
        if rediscovered is not result:
            return rediscovered
    if result.get("ok") and not (state().get("last_teams_app_publish") or {}).get("ok"):
        if not (state()["config"].get("graph_access_token") or "").strip():
            login = start_oauth_device("graph", state()["config"])
            return {
                "ok": login.get("ok", False),
                "step": "start_graph_login",
                "next": "authorize Graph in the opened browser, then click again",
                "result": login,
            }
        publish = publish_teams_app(state()["config"])
        if graph_auth_needs_login(publish):
            return start_graph_login_after_expiry(state()["config"], publish)
        return {
            "ok": publish.get("ok", False),
            "step": "publish_teams_app",
            "next": "open the Add to Teams link, install the app, then send a message to the bot" if publish.get("ok") else "fix Teams app catalog publishing and retry",
            "result": publish,
        }
    if result.get("ok") and not (state().get("last_teams_app_install") or {}).get("ok"):
        if not (state()["config"].get("graph_access_token") or "").strip():
            login = start_oauth_device("graph", state()["config"])
            return {
                "ok": login.get("ok", False),
                "step": "start_graph_login",
                "next": "authorize Graph in the opened browser, then click again",
                "result": login,
            }
        install = install_teams_app_for_me(state()["config"])
        if graph_auth_needs_login(install):
            return start_graph_login_after_expiry(state()["config"], install)
        return {
            "ok": install.get("ok", False),
            "step": "install_teams_app_for_user",
            "next": "open the bot chat link and send hello" if install.get("ok") else "fix Teams app user installation and retry",
            "result": install,
        }
    return {
        "ok": result.get("ok", False),
        "step": "reconcile_azure_bot_resource",
        "next": "open the bot chat link, send a plain text message, then wait for First Teams message received",
        "result": result,
    }


def graph_access_token(config):
    token = (config.get("graph_access_token") or os.environ.get("GRAPH_ACCESS_TOKEN") or "").strip()
    if token:
        return {"ok": True, "access_token": token, "source": "device_or_env"}
    return {"ok": False, "error": "complete Graph device login first"}


def graph_request(method, path, config, body=None):
    token = graph_access_token(config)
    if not token.get("ok"):
        return token
    url = f"https://graph.microsoft.com/v1.0{path}"
    return json_request(method, url, body, headers={"Authorization": f"Bearer {token['access_token']}"})


def graph_binary_request(method, path, config, payload, content_type):
    token = graph_access_token(config)
    if not token.get("ok"):
        return token
    url = f"https://graph.microsoft.com/v1.0{path}"
    return binary_request(
        method,
        url,
        payload,
        headers={
            "Authorization": f"Bearer {token['access_token']}",
            "Content-Type": content_type,
        },
    )


def botframework_sdk_enabled():
    return bool(BOTFRAMEWORK_SDK_URL)


def proxy_botframework_activity(body, headers):
    if not botframework_sdk_enabled():
        return {"ok": False, "status": 0, "error": "Bot Framework SDK sidecar is not enabled"}
    req_headers = {"Content-Type": "application/json"}
    authorization = headers.get("Authorization") or headers.get("authorization")
    if authorization:
        req_headers["Authorization"] = authorization
    channel_id = headers.get("ChannelId") or headers.get("channelid")
    if channel_id:
        req_headers["ChannelId"] = channel_id
    payload = json.dumps(body).encode("utf-8")
    return binary_request("POST", f"{BOTFRAMEWORK_SDK_URL}/api/messages", payload, headers=req_headers, timeout=25)


def odata_string(value):
    return str(value).replace("'", "''")


def reconcile_bot_app(config):
    display_name = (config.get("bot_display_name") or config.get("azure_bot_name") or "Greentic Teams Bot").strip()
    if not display_name:
        return {"ok": False, "error": "bot_display_name is required"}
    select = "id,appId,displayName,signInAudience"
    configured_app_id = (config.get("bot_app_id") or "").strip()
    filter_expr = (
        f"appId eq '{odata_string(configured_app_id)}'"
        if configured_app_id
        else f"displayName eq '{odata_string(display_name)}'"
    )
    lookup = graph_request("GET", f"/applications?$filter={url_escape(filter_expr)}&$select={url_escape(select)}", config)
    if not lookup.get("ok"):
        append_event("bot-app-reconcile", {"action": "lookup_failed", "result": lookup})
        return lookup
    items = ((lookup.get("body") or {}).get("value") or [])
    action = "reuse"
    if items:
        app = items[0]
        if configured_app_id:
            action = "reuse_by_app_id"
    else:
        if configured_app_id:
            return {
                "ok": False,
                "error": "configured bot_app_id was not found in Microsoft Graph applications",
                "bot_app_id": configured_app_id,
            }
        create = graph_request("POST", "/applications", config, {
            "displayName": display_name,
            "signInAudience": "AzureADMultipleOrgs",
        })
        if not create.get("ok"):
            append_event("bot-app-reconcile", {"action": "create_failed", "result": create})
            return create
        app = create.get("body") or {}
        action = "create"
    app_object_id = app.get("id")
    app_id = app.get("appId")
    values = state()
    values["config"]["bot_display_name"] = display_name
    if app_id:
        values["config"]["bot_app_id"] = app_id
    secret_action = "keep_existing_secret"
    secret_result = None
    if not (values["config"].get("bot_app_password") or "").strip():
        if not app_object_id:
            return {"ok": False, "error": "app object id missing; cannot add password", "app": app}
        secret_result = graph_request("POST", f"/applications/{app_object_id}/addPassword", config, {
            "passwordCredential": {
                "displayName": "Greentic Teams Bot setup secret",
            }
        })
        if not secret_result.get("ok"):
            append_event("bot-app-reconcile", {"action": "add_password_failed", "result": secret_result})
            return secret_result
        secret_text = (secret_result.get("body") or {}).get("secretText")
        if secret_text:
            values["config"]["bot_app_password"] = secret_text
            secret_action = "generated_secret"
    result = {
        "ok": True,
        "action": action,
        "secret_action": secret_action,
        "bot_app_id": app_id,
        "app_object_id": app_object_id,
        "display_name": display_name,
    }
    values["last_app_registration"] = result
    save_state(values)
    append_event("bot-app-reconcile", result)
    return result


DEFAULT_BOT_TOKEN_ENDPOINT = "https://login.microsoftonline.com/botframework.com/oauth2/v2.0/token"
DEFAULT_BOT_TOKEN_SCOPE = "https://api.botframework.com/.default"


def looks_like_guid(value):
    return bool(re.fullmatch(r"[0-9a-fA-F]{8}-[0-9a-fA-F]{4}-[0-9a-fA-F]{4}-[0-9a-fA-F]{4}-[0-9a-fA-F]{12}", value or ""))


def bot_token_tenant(config, activity=None):
    channel_tenant = (((activity or {}).get("channelData") or {}).get("tenant") or {}).get("id")
    for value in (channel_tenant, config.get("bot_app_tenant_id"), config.get("azure_auth_tenant"), os.environ.get("AZURE_TENANT_ID")):
        value = (value or "").strip()
        if looks_like_guid(value):
            return value
    return ""


def bot_token_endpoint(config, tenant_id=None):
    configured = (config.get("bot_token_endpoint") or "").strip()
    if tenant_id and (not configured or configured == DEFAULT_BOT_TOKEN_ENDPOINT or "login.microsoftonline.com/botframework.com/" in configured):
        return f"https://login.microsoftonline.com/{tenant_id}/oauth2/v2.0/token"
    return configured or DEFAULT_BOT_TOKEN_ENDPOINT


def acquire_bot_token(config, activity=None):
    if config.get("bot_access_token"):
        return {"ok": True, "access_token": config["bot_access_token"], "source": "manual"}
    app_id = (config.get("bot_app_id") or "").strip()
    secret = (config.get("bot_app_password") or "").strip()
    if not app_id or not secret:
        return {"ok": False, "error": "bot_app_id and bot_app_password are required"}
    tenant_id = bot_token_tenant(config, activity)
    endpoint = bot_token_endpoint(config, tenant_id)
    result = form_request(
        endpoint,
        {
            "grant_type": "client_credentials",
            "client_id": app_id,
            "client_secret": secret,
            "scope": config.get("bot_token_scope") or DEFAULT_BOT_TOKEN_SCOPE,
        },
    )
    if not result["ok"]:
        result["token_endpoint"] = endpoint
        result["tenant_id"] = tenant_id or None
        return result
    token = (result.get("body") or {}).get("access_token")
    if not token:
        return {"ok": False, "error": "token response missing access_token", "response": result}
    return {"ok": True, "access_token": token, "source": "client_credentials", "token_endpoint": endpoint, "tenant_id": tenant_id or None}


def adaptive_card(text="Teams Bot Framework test card"):
    def submit_action(title, action_id, style=None, extra=None):
        value = {"action_id": action_id}
        if extra:
            value.update(extra)
        data = {
            "action_id": action_id,
            "msteams": {
                "type": "messageBack",
                "displayText": title,
                "text": action_id,
                "value": json.dumps(value),
            },
        }
        if extra:
            data.update(extra)
        action = {
            "type": "Action.Submit",
            "title": title,
            "data": data,
            "msTeams": {
                "feedback": {
                    "hide": True,
                },
            },
        }
        if style:
            action["style"] = style
            action["msTeams"]["style"] = style
        return action

    return {
        "$schema": "http://adaptivecards.io/schemas/adaptive-card.json",
        "type": "AdaptiveCard",
        "version": "1.5",
        "body": [
            {
                "type": "TextBlock",
                "text": "Greentic Teams Bot Test",
                "weight": "Bolder",
                "size": "Large",
                "wrap": True,
            },
            {"type": "TextBlock", "text": text, "wrap": True},
            {
                "type": "Input.Text",
                "id": "greentic_tester_text",
                "label": "Text returned by Action.Submit",
                "placeholder": "Type text and press Show next",
                "isMultiline": False,
            },
        ],
        "actions": [
            submit_action("Show next", "greentic_show_next", extra={"routeToCardId": "greentic_next_card"}),
            submit_action("Default", "default_action"),
            submit_action("Positive", "positive_action", style="positive"),
            submit_action("Destructive", "destructive_action", style="destructive"),
        ],
    }


def outgoing_activity(text):
    return {
        "type": "message",
        "text": text or "Teams Bot Framework Adaptive Card",
        "attachments": [
            {
                "contentType": "application/vnd.microsoft.card.adaptive",
                "content": adaptive_card(text),
            }
        ],
    }


def action_submit_payload(activity):
    raw_value = activity.get("value")
    if isinstance(raw_value, str) and raw_value.strip():
        try:
            raw_value = json.loads(raw_value)
        except json.JSONDecodeError:
            raw_value = {"text": raw_value}
    value = raw_value if isinstance(raw_value, dict) else {}
    action = value.get("action") if isinstance(value.get("action"), dict) else {}
    data = action.get("data") if isinstance(action.get("data"), dict) else {}
    msteams = value.get("msteams") if isinstance(value.get("msteams"), dict) else {}
    if not msteams and isinstance(data.get("msteams"), dict):
        msteams = data.get("msteams")
    raw_msteams_value = msteams.get("value")
    if isinstance(raw_msteams_value, str) and raw_msteams_value.strip():
        try:
            raw_msteams_value = json.loads(raw_msteams_value)
        except json.JSONDecodeError:
            raw_msteams_value = {"text": raw_msteams_value}
    msteams_value = raw_msteams_value if isinstance(raw_msteams_value, dict) else {}
    state_value = value.get("state") if isinstance(value.get("state"), dict) else {}
    merged = {}
    for source in (value, data, msteams_value, state_value):
        for key, item in source.items():
            if key not in ("action", "state", "msteams"):
                merged[key] = item
    if msteams.get("text"):
        merged.setdefault("text", msteams.get("text"))
    if msteams.get("displayText"):
        merged.setdefault("displayText", msteams.get("displayText"))
    if action.get("id"):
        merged.setdefault("action_id", action.get("id"))
    if action.get("verb"):
        merged.setdefault("action_id", action.get("verb"))
        merged.setdefault("verb", action.get("verb"))
    if action.get("type"):
        merged.setdefault("action_type", action.get("type"))
    return merged


def submit_action_id(activity):
    payload = action_submit_payload(activity)
    text = (activity.get("text") or "").strip()
    return (
        payload.get("action_id")
        or payload.get("action")
        or (text if text.endswith("_action") or text == "greentic_show_next" else "")
        or activity.get("name")
        or "unknown_action"
    )


def submit_text(activity):
    payload = action_submit_payload(activity)
    for key in ("greentic_tester_text", "displayText"):
        if isinstance(payload.get(key), str) and payload[key].strip():
            return payload[key].strip()
    if isinstance(activity.get("text"), str) and activity["text"].strip():
        text = activity["text"].strip()
        if not (text.endswith("_action") or text == "greentic_show_next"):
            return text
    if isinstance(payload.get("text"), str) and payload["text"].strip():
        return payload["text"].strip()
    return ""


def action_result_card(action_id, text):
    shown_text = text or "(empty)"
    return {
        "$schema": "http://adaptivecards.io/schemas/adaptive-card.json",
        "type": "AdaptiveCard",
        "version": "1.5",
        "body": [
            {
                "type": "TextBlock",
                "text": "Button clicked",
                "weight": "Bolder",
                "size": "Large",
                "wrap": True,
            },
            {
                "type": "FactSet",
                "facts": [
                    {"title": "Button", "value": str(action_id or "unknown_action")},
                    {"title": "Text", "value": shown_text},
                ],
            },
        ],
    }


def action_result_activity(action_id, text):
    return {
        "type": "message",
        "text": f"Button clicked: {action_id}; text: {text or '(empty)'}",
        "attachments": [
            {
                "contentType": "application/vnd.microsoft.card.adaptive",
                "content": action_result_card(action_id, text),
            }
        ],
    }


def post_activity_to_conversation(config, activity, outgoing, event_kind):
    service_url = (activity.get("serviceUrl") or "").rstrip("/")
    conversation_id = ((activity.get("conversation") or {}).get("id") or "").strip()
    if not service_url or not conversation_id:
        return {"ok": False, "error": "activity missing serviceUrl or conversation.id"}
    token = acquire_bot_token(config, activity)
    if not token.get("ok"):
        return token
    url = f"{service_url}/v3/conversations/{conversation_id}/activities"
    result = json_request(
        "POST",
        url,
        outgoing,
        headers={"Authorization": f"Bearer {token['access_token']}"},
    )
    event = {
        "service_url": service_url,
        "conversation_id": conversation_id,
        "result": scrub_token_result(result),
    }
    append_event(event_kind, event)
    return event


def send_action_result_card(config, activity, action_id, text):
    result = post_activity_to_conversation(
        config,
        activity,
        action_result_activity(action_id, text),
        "bot-action-result-send",
    )
    append_event("bot-action-result-complete", {
        "action_id": action_id,
        "submitted_text": text,
        "send_result": scrub_token_result(result),
    })
    return result


def adaptive_card_invoke_body(message="Greentic received Action.Submit"):
    return {
        "statusCode": 200,
        "type": "application/vnd.microsoft.activity.message",
        "value": message,
    }


def invoke_http_response(message="Greentic received Action.Submit"):
    body = adaptive_card_invoke_body(message)
    mode = (state()["config"].get("invoke_response_mode") or os.environ.get("GREENTIC_TEAMS_INVOKE_RESPONSE_MODE") or "botframework").strip().lower()
    if mode == "botframework":
        return {"status": 200, "body": body}
    if mode == "activity":
        return {"type": "invokeResponse", "value": {"status": 200, "body": body}}
    return body


def directline_prefix(config=None):
    return f"{webchat_base_url(config)}/v3/directline"


def webchat_conversations():
    conversations = state().get("webchat_conversations")
    return conversations if isinstance(conversations, dict) else {}


def webchat_conversation(tenant, conversation_id):
    return webchat_conversations().get(conversation_id) or {
        "tenant": tenant,
        "conversationId": conversation_id,
        "activities": [],
        "watermark": "0",
        "created_at": now_iso(),
    }


def save_webchat_conversation(conversation):
    values = state()
    conversations = values.get("webchat_conversations")
    if not isinstance(conversations, dict):
        conversations = {}
    conversation["watermark"] = str(len(conversation.get("activities") or []))
    conversations[conversation["conversationId"]] = conversation
    values["webchat_conversations"] = conversations
    values["last_webchat_conversation"] = {
        "tenant": conversation.get("tenant"),
        "conversationId": conversation.get("conversationId"),
        "activity_count": len(conversation.get("activities") or []),
        "updated_at": now_iso(),
    }
    save_state(values)
    return conversation


def webchat_activity(activity_type, text="", from_role="bot", attachments=None, value=None):
    activity = {
        "id": f"webchat-{int(time.time() * 1000)}-{uuid.uuid4().hex[:8]}",
        "type": activity_type,
        "timestamp": now_iso(),
        "from": {"id": from_role, "name": "Greentic" if from_role == "bot" else "You"},
    }
    if text:
        activity["text"] = text
    if attachments:
        activity["attachments"] = attachments
    if value is not None:
        activity["value"] = value
    return activity


def create_webchat_conversation(tenant):
    conversation_id = str(uuid.uuid4())
    conversation = webchat_conversation(tenant, conversation_id)
    conversation["activities"].append(webchat_activity("message", "Greentic Bot Service connected.", "bot"))
    save_webchat_conversation(conversation)
    append_event("webchat-conversation-created", {"tenant": tenant, "conversationId": conversation_id})
    return {
        "conversationId": conversation_id,
        "token": f"local-{conversation_id}",
        "expires_in": 3600,
        "streamUrl": "",
    }


def webchat_result_activity(action_id, text):
    return webchat_activity(
        "message",
        f"Button clicked: {action_id}; text: {text or '(empty)'}",
        "bot",
        attachments=[{
            "contentType": "application/vnd.microsoft.card.adaptive",
            "content": action_result_card(action_id, text),
        }],
    )


def append_webchat_inbound(tenant, conversation_id, activity):
    conversation = webchat_conversation(tenant, conversation_id)
    incoming = dict(activity or {})
    incoming.setdefault("type", "message")
    incoming.setdefault("from", {"id": "user", "name": "You"})
    incoming.setdefault("id", f"webchat-user-{int(time.time() * 1000)}-{uuid.uuid4().hex[:8]}")
    incoming.setdefault("timestamp", now_iso())
    conversation.setdefault("activities", []).append(incoming)
    value = action_submit_payload(incoming)
    action_id = submit_action_id(incoming) if value or incoming.get("name") else ""
    if action_id and action_id != "unknown_action":
        text = submit_text(incoming)
        conversation["activities"].append(webchat_result_activity(action_id, text))
        append_event("webchat-action-received", {"conversationId": conversation_id, "action_id": action_id, "submitted_text": text})
    else:
        text = activity_text(incoming)
        envelope = normalize_activity({
            "type": "message",
            "id": incoming["id"],
            "text": text,
            "conversation": {"id": conversation_id, "conversationType": "personal"},
            "from": incoming.get("from") or {},
            "recipient": {"id": "greentic-webchat", "name": "Greentic"},
            "channelId": "webchat",
        }, tenant, "webchat")
        values = state()
        values["last_envelope"] = envelope
        save_state(values)
        append_event("webchat-message-received", {"activity": incoming, "envelope": envelope})
    save_webchat_conversation(conversation)
    return {"id": incoming["id"]}


def send_webchat_card(config, text=None):
    last = state().get("last_webchat_conversation") or {}
    conversation_id = (last.get("conversationId") or "").strip()
    tenant = (last.get("tenant") or config.get("tenant") or "default").strip("/") or "default"
    if not conversation_id:
        return {
            "ok": False,
            "error": "open Greentic chat and send a message first so the tester has a webchat conversation",
            "next": "open the Greentic chat link, send hello, then click Send card again",
            "open_greentic_chat_url": teams_app_info(config).get("open_bot_chat_url"),
            "webchat_url": webchat_app_url(config),
            "directline_base": directline_prefix(config),
        }
    conversation = webchat_conversation(tenant, conversation_id)
    outgoing = outgoing_activity(text)
    outgoing["id"] = f"webchat-bot-{int(time.time() * 1000)}-{uuid.uuid4().hex[:8]}"
    outgoing["timestamp"] = now_iso()
    outgoing["from"] = {"id": "bot", "name": "Greentic"}
    conversation.setdefault("activities", []).append(outgoing)
    save_webchat_conversation(conversation)
    event = {
        "ok": True,
        "runtime_provider": "greentic-webchat",
        "conversationId": conversation_id,
        "activity": outgoing,
    }
    append_event("webchat-card-send", event)
    values = state()
    values["last_send"] = event
    save_state(values)
    return event


def no_last_conversation_error():
    values = state()
    cfg = values["config"]
    events = recent_events()
    public_ping_seen = any((event.get("kind") == "public-ping-received") for event in events[-50:])
    last_public_ping = next(
        (event for event in reversed(events) if event.get("kind") == "public-ping-received"),
        None,
    )
    last_bot_activity = next(
        (event for event in reversed(events) if event.get("kind") == "bot-activity-received"),
        None,
    )
    return {
        "ok": False,
        "error": "no Teams Bot Framework activity has reached this tester yet, so serviceUrl/conversation.id are not available",
        "next": "open the bot chat link below, send a plain text message such as hello, then wait until First Teams message received is done",
        "open_bot_chat_url": teams_app_info(cfg).get("open_bot_chat_url"),
        "add_to_teams_url": teams_app_info(cfg).get("add_to_teams_url"),
        "messaging_endpoint": public_ingress_url(cfg) or ingress_url(cfg),
        "teams_app_installed": bool((values.get("last_teams_app_install") or {}).get("ok")),
        "teams_app_install": values.get("last_teams_app_install"),
        "public_tunnel_self_test_seen": public_ping_seen,
        "last_public_tunnel_self_test": last_public_ping,
        "last_bot_activity_received": last_bot_activity,
        "diagnostic": "If public_tunnel_self_test_seen is true and this still happens, Teams is not routing the chat message to the Bot Framework-compatible endpoint registered for this bot app id.",
    }


def send_card(config, text=None):
    if is_webchat_runtime(config):
        return send_webchat_card(config, text)
    activity = state().get("last_activity")
    if not activity:
        return no_last_conversation_error()
    event = post_activity_to_conversation(config, activity, outgoing_activity(text), "bot-card-send")
    values = state()
    values["last_send"] = event
    save_state(values)
    return event


def public_tunnel_self_test(config):
    base = public_url()
    if not base.startswith("https://"):
        return {"ok": False, "error": "public HTTPS tunnel is not ready"}
    url = f"{base}/api/diagnostic/public-ping"
    result = json_request("POST", url, {"sent_at": now_iso(), "public_url": base})
    event = {"url": url, "result": scrub_token_result(result)}
    append_event("public-ping-send", event)
    return {"ok": result.get("ok", False), **event}


def scrub_token_result(value):
    return json.loads(json.dumps(value))


def strip_html(value):
    value = value or ""
    value = re.sub(r"<br\\s*/?>", "\n", value, flags=re.IGNORECASE)
    value = re.sub(r"<[^>]+>", "", value)
    return html.unescape(value).strip()


def activity_text(activity):
    if isinstance(activity.get("text"), str) and activity["text"].strip():
        return activity["text"].strip()
    value = action_submit_payload(activity)
    for key in ("greentic_tester_text", "text", "displayText"):
        if isinstance(value.get(key), str) and value[key].strip():
            return value[key].strip()
    if isinstance(activity.get("name"), str) and activity["name"].strip():
        return activity["name"].strip()
    return strip_html(((activity.get("body") or {}).get("content") or ""))


def lifecycle_key(provider, scope, conversation, user, reason):
    def part(value):
        value = str(value or "").strip()
        return value.replace(":", "_") if value else "_"

    return f"lifecycle.user_entered:{part(provider)}:{part(scope)}:{part(conversation)}:{part(user)}:{part(reason)}"


def normalize_activity(activity, tenant, team):
    activity_id = activity.get("id") or f"activity-{int(time.time() * 1000)}"
    conversation = activity.get("conversation") or {}
    sender = activity.get("from") or {}
    recipient = activity.get("recipient") or {}
    value = action_submit_payload(activity)
    action_id = submit_action_id(activity)
    text = activity_text(activity)
    metadata = {
        "activity_type": activity.get("type"),
        "activity_name": activity.get("name"),
        "service_url": activity.get("serviceUrl"),
        "conversation_id": conversation.get("id"),
        "conversation_type": conversation.get("conversationType"),
        "channel_id": activity.get("channelId"),
        "tenant_id": ((activity.get("channelData") or {}).get("tenant") or {}).get("id"),
        "team_id": ((activity.get("channelData") or {}).get("team") or {}).get("id"),
        "raw_value": value or None,
        "action_id": action_id,
    }
    if activity.get("type") == "conversationUpdate" and activity.get("membersAdded"):
        human_members = [
            member for member in activity.get("membersAdded") or []
            if (member.get("id") or "") != (recipient.get("id") or "")
        ]
        user_id = (human_members[0].get("id") if human_members else sender.get("id")) or ""
        metadata.update({
            "event_type": "channel.user.entered",
            "autoStart": "true",
            "provider": "teams",
            "reason": "members_added",
            "user_id": user_id,
            "idempotency_key": lifecycle_key(
                "teams",
                ((activity.get("channelData") or {}).get("tenant") or {}).get("id"),
                conversation.get("id"),
                user_id,
                "members_added",
            ),
        })
        text = ""
    metadata = {k: v for k, v in metadata.items() if v not in (None, "", {})}
    envelope = {
        "id": f"teams-bot:{activity_id}",
        "provider": "messaging.teams.bot",
        "provider_message_id": activity_id,
        "source": "teams",
        "text": text,
        "from": [
            {
                "id": sender.get("id") or "",
                "kind": "user",
                "name": sender.get("name"),
            }
        ],
        "to": [
            {
                "id": conversation.get("id") or f"{tenant}:{team}",
                "kind": "conversation",
                "name": conversation.get("name"),
            }
        ],
        "metadata": metadata,
    }
    if action_id:
        envelope["event"] = {"kind": "action", "id": action_id, "value": value}
    if recipient:
        envelope["metadata"]["recipient_id"] = recipient.get("id")
        envelope["metadata"]["recipient_name"] = recipient.get("name")
    return envelope


def azure_management_token():
    token = os.environ.get("AZURE_MANAGEMENT_TOKEN")
    if token:
        return {"ok": True, "access_token": token, "source": "AZURE_MANAGEMENT_TOKEN"}
    az_token = azure_cli_management_token()
    if az_token.get("ok"):
        return az_token
    saved = (state().get("config") or {}).get("azure_management_access_token")
    if saved:
        return {"ok": True, "access_token": saved, "source": "device_login"}
    tenant = os.environ.get("AZURE_TENANT_ID")
    client_id = os.environ.get("AZURE_CLIENT_ID")
    secret = os.environ.get("AZURE_CLIENT_SECRET")
    if not (tenant and client_id and secret):
        return {
            "ok": False,
            "error": "set AZURE_MANAGEMENT_TOKEN or AZURE_TENANT_ID/AZURE_CLIENT_ID/AZURE_CLIENT_SECRET",
        }
    return form_request(
        f"https://login.microsoftonline.com/{tenant}/oauth2/v2.0/token",
        {
            "grant_type": "client_credentials",
            "client_id": client_id,
            "client_secret": secret,
            "scope": "https://management.azure.com/.default",
        },
    )


def azure_cli_management_token():
    if not shutil.which("az"):
        return {"ok": False, "error": "az CLI not found"}
    try:
        completed = subprocess.run(
            [
                "az",
                "account",
                "get-access-token",
                "--resource",
                "https://management.azure.com/",
                "-o",
                "json",
            ],
            capture_output=True,
            text=True,
            timeout=20,
            check=False,
        )
    except Exception as exc:
        return {"ok": False, "error": f"az CLI token failed: {exc}"}
    if completed.returncode != 0:
        detail = (completed.stderr or completed.stdout or "").strip()
        return {"ok": False, "error": f"az CLI token failed: {detail}"}
    try:
        body = json.loads(completed.stdout or "{}")
    except json.JSONDecodeError as exc:
        return {"ok": False, "error": f"az CLI token response was not JSON: {exc}"}
    token = (body.get("accessToken") or "").strip()
    if not token:
        return {"ok": False, "error": "az CLI token response missing accessToken"}
    return {"ok": True, "access_token": token, "source": "az_cli"}


def azure_bot_resource_body(config, target):
    display_name = (config.get("bot_display_name") or config.get("azure_bot_name") or "Greentic Teams Bot").strip()
    app_id = (config.get("bot_app_id") or "").strip()
    tenant = (config.get("azure_auth_tenant") or os.environ.get("AZURE_TENANT_ID") or "").strip()
    properties = {
        "displayName": display_name,
        "endpoint": target,
        "msaAppId": app_id,
        "msaAppType": "MultiTenant",
        "publicNetworkAccess": "Enabled",
    }
    if tenant and tenant not in ("organizations", "common"):
        properties["msaAppTenantId"] = tenant
    return {
        "location": config.get("azure_location") or "global",
        "kind": "azurebot",
        "sku": {"name": "F0"},
        "properties": properties,
    }


def ensure_teams_channel(base, api, token):
    url = f"{base}/channels/MsTeamsChannel?api-version={api}"
    body = {
        "location": "global",
        "properties": {
            "channelName": "MsTeamsChannel",
            "properties": {
                "isEnabled": True,
                "acceptedTerms": True,
            },
        },
    }
    return json_request("PUT", url, body, headers={"Authorization": f"Bearer {token}"})


def reconcile_azure_bot(config):
    subscription_id = config.get("azure_subscription_id") or os.environ.get("AZURE_SUBSCRIPTION_ID")
    resource_group = config.get("azure_resource_group") or os.environ.get("AZURE_RESOURCE_GROUP")
    bot_name = config.get("azure_bot_name") or os.environ.get("AZURE_BOT_NAME")
    target = public_ingress_url(config)
    if not target:
        result = {
            "ok": False,
            "skipped": True,
            "reason": "Cloudflare public HTTPS URL is not ready; wait for cloudflared and retry",
            "current_public_url": public_url(),
            "target_messaging_endpoint": None,
        }
        append_event("azure-bot-reconcile", result)
        return result
    if not (subscription_id and resource_group and bot_name):
        result = {
            "ok": False,
            "skipped": True,
            "reason": "Azure subscription, resource group, and bot name are required",
            "target_messaging_endpoint": target,
        }
        append_event("azure-bot-reconcile", result)
        return result
    if not (config.get("bot_app_id") or "").strip():
        app_result = reconcile_bot_app(config)
        if not app_result.get("ok"):
            result = {**app_result, "target_messaging_endpoint": target}
            append_event("azure-bot-reconcile", result)
            return result
        config = state()["config"]
    token = azure_management_token()
    if not token.get("ok"):
        result = {**token, "target_messaging_endpoint": target}
        append_event("azure-bot-reconcile", result)
        return result
    base = (
        "https://management.azure.com/subscriptions/"
        f"{subscription_id}/resourceGroups/{resource_group}/providers/Microsoft.BotService/botServices/{bot_name}"
    )
    api = "2022-09-15"
    get_result = json_request(
        "GET",
        f"{base}?api-version={api}",
        headers={"Authorization": f"Bearer {token['access_token']}"},
    )
    current = None
    if get_result.get("ok"):
        current = ((get_result.get("body") or {}).get("properties") or {}).get("endpoint")
    if current == target:
        channel_result = ensure_teams_channel(base, api, token["access_token"])
        result = {
            "ok": True,
            "action": "keep",
            "current_messaging_endpoint": current,
            "target_messaging_endpoint": target,
            "teams_channel": {
                "ok": channel_result.get("ok"),
                "status": channel_result.get("status"),
            },
        }
        append_event("azure-bot-reconcile", result)
        values = state()
        values["last_reconcile"] = result
        save_state(values)
        return result
    body = azure_bot_resource_body(config, target)
    if get_result.get("status") == 404:
        write_result = json_request(
            "PUT",
            f"{base}?api-version={api}",
            body,
            headers={"Authorization": f"Bearer {token['access_token']}"},
        )
        action = "create"
        attempted_action = "Microsoft.BotService/botServices/write"
    else:
        write_result = json_request(
            "PATCH",
            f"{base}?api-version={api}",
            {"properties": {"endpoint": target}},
            headers={"Authorization": f"Bearer {token['access_token']}"},
        )
        action = "update"
        attempted_action = "Microsoft.BotService/botServices/write"
    channel_result = ensure_teams_channel(base, api, token["access_token"]) if write_result.get("ok") else {"ok": False, "status": 0}
    result = {
        "ok": bool(write_result.get("ok")),
        "action": action if write_result.get("ok") else f"{action}_failed",
        "current_messaging_endpoint": current,
        "target_messaging_endpoint": target,
        "http_status": write_result.get("status"),
        "response": write_result.get("body") if not write_result.get("ok") else None,
        "teams_channel": {
            "ok": channel_result.get("ok"),
            "status": channel_result.get("status"),
            "response": channel_result.get("body") if not channel_result.get("ok") else None,
        },
    }
    guidance = azure_rbac_guidance(write_result, config, attempted_action)
    if guidance:
        result["admin_guidance"] = guidance
        result["admin_summary"] = guidance["summary"]
    append_event("azure-bot-reconcile", result)
    values = state()
    values["last_reconcile"] = result
    save_state(values)
    return result


def public_base_url_from_messaging_endpoint(endpoint):
    marker = "/v1/messaging/ingress/"
    if not endpoint or marker not in endpoint:
        return ""
    return endpoint.split(marker, 1)[0].rstrip("/")


def register_bot_framework_endpoint(request):
    request = request or {}
    provider_id = (request.get("provider_id") or "messaging-teams").strip()
    channel = (request.get("channel") or "").strip().lower()
    app_id = (request.get("bot_app_id") or "").strip()
    password = (request.get("bot_app_password") or "").strip()
    target = (request.get("messaging_endpoint") or "").strip()
    tenant = (request.get("tenant") or "").strip() or "default"
    team = (request.get("team") or "").strip() or "default"

    missing = [
        key for key, value in (
            ("bot_app_id", app_id),
            ("bot_app_password", password),
            ("messaging_endpoint", target),
        )
        if not value
    ]
    if missing:
        result = {
            "ok": False,
            "error": "missing required Bot Framework registration fields",
            "missing": missing,
            "provider_id": provider_id,
            "channel": channel or None,
            "target_messaging_endpoint": target or None,
        }
        append_event("bot-framework-registration-failed", result)
        return result
    if provider_id != "messaging-teams":
        result = {
            "ok": False,
            "error": "unsupported provider for Bot Framework registration",
            "provider_id": provider_id,
            "expected_provider_id": "messaging-teams",
            "target_messaging_endpoint": target,
        }
        append_event("bot-framework-registration-failed", result)
        return result
    if channel and channel != "msteams":
        result = {
            "ok": False,
            "error": "unsupported Bot Framework channel",
            "channel": channel,
            "expected_channel": "msteams",
            "target_messaging_endpoint": target,
        }
        append_event("bot-framework-registration-failed", result)
        return result

    values = state()
    cfg = values["config"]
    previous = values.get("last_reconcile") or {}
    current = previous.get("target_messaging_endpoint") if isinstance(previous, dict) else None
    cfg.update({
        "bot_app_id": app_id,
        "bot_app_password": password,
        "runtime_provider": "greentic-teams-bot",
        "tenant": tenant,
        "team": team,
    })
    public_base_url = public_base_url_from_messaging_endpoint(target)
    if public_base_url:
        cfg["public_base_url"] = public_base_url
    result = {
        "ok": True,
        "action": "keep" if current == target else "update",
        "current_messaging_endpoint": current,
        "target_messaging_endpoint": target,
        "teams_channel": {"ok": True},
        "provider_id": provider_id,
        "channel": "msteams",
    }
    values["config"] = cfg
    values["last_reconcile"] = result
    save_state(values)
    append_event("bot-framework-registration", result)
    return result


def verify_azure_bot(config):
    subscription_id = config.get("azure_subscription_id") or os.environ.get("AZURE_SUBSCRIPTION_ID")
    resource_group = config.get("azure_resource_group") or os.environ.get("AZURE_RESOURCE_GROUP")
    bot_name = config.get("azure_bot_name") or os.environ.get("AZURE_BOT_NAME")
    target = public_ingress_url(config) or ingress_url(config)
    if not (subscription_id and resource_group and bot_name):
        return {"ok": False, "error": "Azure subscription, resource group, and bot name are required"}
    token = azure_management_token()
    if not token.get("ok"):
        return token
    base = (
        "https://management.azure.com/subscriptions/"
        f"{subscription_id}/resourceGroups/{resource_group}/providers/Microsoft.BotService/botServices/{bot_name}"
    )
    api = "2022-09-15"
    bot = json_request("GET", f"{base}?api-version={api}", headers={"Authorization": f"Bearer {token['access_token']}"})
    if azure_auth_expired(bot):
        return start_management_login_after_expiry(config, {**bot, "step": "verify_azure_bot"})
    if not bot.get("ok"):
        return {**bot, "step": "verify_azure_bot"}
    props = (bot.get("body") or {}).get("properties") or {}
    channel = json_request("GET", f"{base}/channels/MsTeamsChannel?api-version={api}", headers={"Authorization": f"Bearer {token['access_token']}"})
    result = {
        "ok": True,
        "resource_id": (bot.get("body") or {}).get("id"),
        "bot_app_id": props.get("msaAppId"),
        "configured_endpoint": props.get("endpoint"),
        "target_endpoint": target,
        "endpoint_matches": props.get("endpoint") == target,
        "expected_bot_app_id": config.get("bot_app_id"),
        "bot_app_id_matches": props.get("msaAppId") == config.get("bot_app_id"),
        "kind": (bot.get("body") or {}).get("kind"),
        "sku": (bot.get("body") or {}).get("sku"),
        "teams_channel": {
            "ok": channel.get("ok"),
            "status": channel.get("status"),
            "body": channel.get("body"),
        },
    }
    append_event("azure-bot-verify", result)
    return result


def teams_app_short_name(config):
    name = (config.get("bot_display_name") or config.get("azure_bot_name") or "Greentic Bot").strip()
    return name[:30] or "Greentic Bot"


def teams_app_id(config):
    if is_greentic_teams_runtime(config) and (config.get("bot_app_id") or "").strip():
        return (config.get("bot_app_id") or "").strip()
    configured = (config.get("teams_app_id") or config.get("bot_app_id") or "").strip()
    if configured:
        return configured
    return ensure_teams_app_identity(config)


def teams_app_info(config):
    app_id = teams_app_id(config)
    publish = (state().get("last_teams_app_publish") or {})
    if publish.get("teams_app_id") != app_id:
        publish = {}
    link_id = (publish.get("catalog_app_id") or app_id or "").strip()
    chat_app_id = link_id
    bot_app_id = (config.get("bot_app_id") or "").strip()
    local_base = LOCAL_URL
    webchat_runtime = is_webchat_runtime(config)
    web_url = webchat_app_url(config)
    entity_id = "greentic-webchat" if webchat_runtime else "conversations"
    entity_url = f"https://teams.microsoft.com/l/entity/{quote(chat_app_id, safe='')}/{quote(entity_id, safe='')}?webUrl={quote(web_url, safe='')}" if publish.get("catalog_app_id") else web_url
    direct_bot_chat_url = f"https://teams.microsoft.com/l/chat/0/0?users={quote('28:' + bot_app_id)}&message=hello" if bot_app_id else ""
    note = (
        "Open Greentic chat uses the Teams personal tab backed by the Greentic/Webchat provider. Azure Bot Service is not used."
        if webchat_runtime
        else "Open bot chat uses the published Teams bot app. Teams will send bot messages and Adaptive Card submits to the Bot Framework-compatible endpoint registered for this bot app id."
    )
    return {
        "ok": bool(app_id and (webchat_runtime or (config.get("bot_app_id") or "").strip())),
        "runtime_provider": config.get("runtime_provider"),
        "teams_app_id": app_id,
        "catalog_app_id": publish.get("catalog_app_id"),
        "bot_app_id": bot_app_id,
        "app_name": teams_app_short_name(config),
        "add_to_teams_url": f"https://teams.microsoft.com/l/app/{quote(link_id)}?source=app-details-dialog" if publish.get("catalog_app_id") else "",
        "open_bot_chat_url": web_url if webchat_runtime else direct_bot_chat_url,
        "open_teams_entity_url": entity_url,
        "webchat_url": web_url,
        "directline_base": webchat_base_url(config),
        "legacy_open_bot_chat_url": direct_bot_chat_url,
        "manifest_url": f"{local_base}/teams-app/manifest.json",
        "package_url": f"{local_base}/teams-app/package.zip",
        "note": note,
    }


def teams_app_manifest(config):
    app_id = teams_app_id(config)
    bot_app_id = (config.get("bot_app_id") or "").strip()
    app_name = teams_app_short_name(config)
    webchat_runtime = is_webchat_runtime(config)
    if not app_id:
        return {"ok": False, "error": "teams_app_id is required before generating a Teams app manifest"}
    if not webchat_runtime and not bot_app_id:
        return {"ok": False, "error": "bot_app_id is required before generating a native Teams bot app manifest"}
    public = public_url() or LOCAL_URL
    domain = urlparse(public).netloc or "localhost"
    manifest = {
        "$schema": "https://developer.microsoft.com/json-schemas/teams/v1.17/MicrosoftTeams.schema.json",
        "manifestVersion": "1.17",
        "version": teams_app_version(config),
        "id": app_id,
        "developer": {
            "name": "Greentic",
            "websiteUrl": "https://greentic.ai",
            "privacyUrl": "https://greentic.ai/privacy",
            "termsOfUseUrl": "https://greentic.ai/terms",
        },
        "name": {
            "short": app_name,
            "full": app_name,
        },
        "description": {
            "short": "Greentic Teams test app",
            "full": "Test app package for opening a native Greentic bot chat in Microsoft Teams during local validation.",
        },
        "icons": {
            "outline": "outline.png",
            "color": "color.png",
        },
        "accentColor": "#2F6FED",
        "validDomains": [domain],
    }
    if webchat_runtime:
        manifest["staticTabs"] = [
            {
                "entityId": "greentic-webchat",
                "name": "Chat",
                "contentUrl": webchat_app_url(config),
                "websiteUrl": webchat_app_url(config),
                "scopes": ["personal"],
            }
        ]
    else:
        manifest["bots"] = [
            {
                "botId": bot_app_id,
                "scopes": ["personal", "team", "groupchat"],
                "supportsFiles": False,
                "isNotificationOnly": False,
            }
        ]
    return manifest


def png_rgba(width, height, pixel_at):
    raw = bytearray()
    for y in range(height):
        raw.append(0)
        for x in range(width):
            raw.extend(pixel_at(x, y))

    def chunk(kind, data):
        return (
            struct.pack(">I", len(data))
            + kind
            + data
            + struct.pack(">I", zlib.crc32(kind + data) & 0xFFFFFFFF)
        )

    return (
        b"\x89PNG\r\n\x1a\n"
        + chunk(b"IHDR", struct.pack(">IIBBBBB", width, height, 8, 6, 0, 0, 0))
        + chunk(b"IDAT", zlib.compress(bytes(raw)))
        + chunk(b"IEND", b"")
    )


def teams_color_icon():
    def px(x, y):
        if 46 <= x <= 146 and 46 <= y <= 146:
            return (30, 126, 84, 255)
        if 74 <= x <= 118 and 70 <= y <= 88:
            return (255, 255, 255, 255)
        if 74 <= x <= 94 and 88 <= y <= 122:
            return (255, 255, 255, 255)
        if 104 <= x <= 126 and 104 <= y <= 122:
            return (255, 255, 255, 255)
        return (47, 111, 237, 255)
    return png_rgba(192, 192, px)


def teams_outline_icon():
    def px(x, y):
        border = x in (7, 8, 23, 24) and 7 <= y <= 24
        border = border or y in (7, 8, 23, 24) and 7 <= x <= 24
        g = (12 <= x <= 20 and y in (12, 13, 19, 20)) or (12 <= y <= 20 and x in (12, 13)) or (18 <= x <= 22 and y in (16, 17))
        return (255, 255, 255, 255) if border or g else (0, 0, 0, 0)
    return png_rgba(32, 32, px)


def teams_app_package(config):
    manifest = teams_app_manifest(config)
    if isinstance(manifest, dict) and manifest.get("ok") is False:
        return None, manifest
    out = io.BytesIO()
    with zipfile.ZipFile(out, "w", compression=zipfile.ZIP_DEFLATED) as archive:
        archive.writestr("manifest.json", json.dumps(manifest, indent=2))
        archive.writestr("color.png", teams_color_icon())
        archive.writestr("outline.png", teams_outline_icon())
    return out.getvalue(), None


def teams_app_catalog_versions(catalog_app_id, config):
    if not catalog_app_id:
        return []
    result = graph_request(
        "GET",
        f"/appCatalogs/teamsApps/{quote(catalog_app_id, safe='')}/appDefinitions?$select=id,version,displayName",
        config,
    )
    if not result.get("ok"):
        append_event("teams-app-definitions-lookup-failed", {"catalog_app_id": catalog_app_id, "result": result})
        return []
    versions = []
    for item in ((result.get("body") or {}).get("value") or []):
        version = (item.get("version") or "").strip()
        if re.fullmatch(r"\d+\.\d+\.\d+", version):
            versions.append(version)
    return versions


def next_teams_app_catalog_version(catalog_app_id, config):
    versions = teams_app_catalog_versions(catalog_app_id, config)
    current = teams_app_version(config)
    highest = current
    for version in versions:
        if parse_semver(version) >= parse_semver(highest):
            highest = version
    return bump_semver(highest)


def publish_teams_app(config):
    app_id = teams_app_id(config)
    if not app_id:
        return {"ok": False, "error": "teams_app_id is required before publishing the Teams app"}

    select = url_escape("id,externalId,displayName,distributionMethod")
    filter_expr = url_escape(f"externalId eq '{odata_string(app_id)}'")
    existing = graph_request("GET", f"/appCatalogs/teamsApps?$filter={filter_expr}&$select={select}", config)
    if not existing.get("ok"):
        return {**existing, "step": "lookup_teams_app_catalog"}
    items = (existing.get("body") or {}).get("value") or []
    if items:
        item = items[0]
        catalog_app_id = item.get("id")
        version = set_teams_app_version(next_teams_app_catalog_version(catalog_app_id, state()["config"]))
        package, error = teams_app_package(state()["config"])
        if error:
            return error
        updated = graph_binary_request(
            "POST",
            f"/appCatalogs/teamsApps/{quote(catalog_app_id, safe='')}/appDefinitions",
            state()["config"],
            package,
            "application/zip",
        )
        if not updated.get("ok") and updated.get("status") == 409:
            version = set_teams_app_version(bump_semver(version))
            package, error = teams_app_package(state()["config"])
            if error:
                return error
            updated = graph_binary_request(
                "POST",
                f"/appCatalogs/teamsApps/{quote(catalog_app_id, safe='')}/appDefinitions",
                state()["config"],
                package,
                "application/zip",
            )
        if not updated.get("ok"):
            return {**updated, "step": "update_teams_app_catalog_definition", "catalog_app_id": catalog_app_id, "manifest_version": version}
        body = updated.get("body") or {}
        result = {
            "ok": True,
            "action": "update",
            "teams_app_id": app_id,
            "catalog_app_id": catalog_app_id,
            "manifest_version": version,
            "display_name": item.get("displayName"),
            "definition_id": body.get("id"),
            "add_to_teams_url": f"https://teams.microsoft.com/l/app/{quote(catalog_app_id)}?source=app-details-dialog" if catalog_app_id else "",
        }
        values = state()
        values["last_teams_app_publish"] = result
        save_state(values)
        append_event("teams-app-publish", result)
        return result

    package, error = teams_app_package(config)
    if error:
        return error
    published = graph_binary_request("POST", "/appCatalogs/teamsApps", config, package, "application/zip")
    if not published.get("ok"):
        return {**published, "step": "publish_teams_app_catalog"}
    body = published.get("body") or {}
    catalog_app_id = body.get("id")
    result = {
        "ok": True,
        "action": "publish",
        "teams_app_id": app_id,
        "catalog_app_id": catalog_app_id,
        "manifest_version": teams_app_version(state()["config"]),
        "display_name": body.get("displayName") or teams_app_short_name(config),
        "add_to_teams_url": f"https://teams.microsoft.com/l/app/{quote(catalog_app_id)}?source=app-details-dialog" if catalog_app_id else "",
    }
    values = state()
    values["last_teams_app_publish"] = result
    save_state(values)
    append_event("teams-app-publish", result)
    return result


def current_teams_app_publish(config=None):
    config = config or state()["config"]
    publish = state().get("last_teams_app_publish") or {}
    if publish.get("teams_app_id") != teams_app_id(config):
        return {}
    if publish.get("manifest_version") != teams_app_version(config):
        return {}
    return publish


def current_teams_app_install(config=None):
    config = config or state()["config"]
    publish = current_teams_app_publish(config)
    install = state().get("last_teams_app_install") or {}
    if install.get("catalog_app_id") != publish.get("catalog_app_id"):
        return {}
    if install.get("manifest_version") != publish.get("manifest_version"):
        return {}
    return install


def assume_manual_teams_app_install(config, publish, previous):
    catalog_app_id = (publish.get("catalog_app_id") or "").strip()
    result = {
        "ok": True,
        "action": "manual_unverified",
        "catalog_app_id": catalog_app_id,
        "installed_app_id": None,
        "manifest_version": publish.get("manifest_version"),
        "add_to_teams_url": teams_app_info(config).get("add_to_teams_url"),
        "open_bot_chat_url": teams_app_info(config).get("open_bot_chat_url"),
        "warning": "Graph could not verify or install the Teams app for this user. Continuing with the manual Teams install flow; the next Bot Framework message will prove routing.",
        "previous": previous,
    }
    values = state()
    values["last_teams_app_install"] = result
    save_state(values)
    append_event("teams-app-install-manual", result)
    return result


def install_teams_app_for_me(config):
    publish = current_teams_app_publish(config)
    catalog_app_id = (publish.get("catalog_app_id") or "").strip()
    if not catalog_app_id:
        return {"ok": False, "error": "publish the Teams app before installing it for the signed-in user"}

    installed = graph_request("GET", "/me/teamwork/installedApps?$expand=teamsApp", config)
    if not installed.get("ok"):
        if graph_permission_denied(installed):
            return assume_manual_teams_app_install(config, publish, {**installed, "step": "list_user_installed_teams_apps"})
        return {**installed, "step": "list_user_installed_teams_apps"}
    items = (installed.get("body") or {}).get("value") or []
    match = next(
        (
            item for item in items
            if ((item.get("teamsApp") or {}).get("id") or "") == catalog_app_id
            or ((item.get("teamsApp") or {}).get("externalId") or "") == teams_app_id(config)
        ),
        None,
    )
    if match:
        result = {
            "ok": True,
            "action": "keep",
            "catalog_app_id": catalog_app_id,
            "installed_app_id": match.get("id"),
            "manifest_version": publish.get("manifest_version"),
            "add_to_teams_url": teams_app_info(config).get("add_to_teams_url"),
            "open_bot_chat_url": teams_app_info(config).get("open_bot_chat_url"),
        }
        values = state()
        values["last_teams_app_install"] = result
        save_state(values)
        append_event("teams-app-install", result)
        return result

    body = {
        "teamsApp@odata.bind": f"https://graph.microsoft.com/v1.0/appCatalogs/teamsApps/{catalog_app_id}",
    }
    created = graph_request("POST", "/me/teamwork/installedApps", config, body)
    if not created.get("ok"):
        if graph_permission_denied(created):
            return assume_manual_teams_app_install(config, publish, {**created, "step": "install_user_teams_app"})
        return {**created, "step": "install_user_teams_app"}
    result = {
        "ok": True,
        "action": "install",
        "catalog_app_id": catalog_app_id,
        "installed_app_id": ((created.get("body") or {}).get("id")),
        "manifest_version": publish.get("manifest_version"),
        "add_to_teams_url": teams_app_info(config).get("add_to_teams_url"),
        "open_bot_chat_url": teams_app_info(config).get("open_bot_chat_url"),
    }
    values = state()
    values["last_teams_app_install"] = result
    save_state(values)
    append_event("teams-app-install", result)
    return result


def webchat_page(tenant):
    tenant = html.escape(tenant or "default")
    return f"""<!doctype html>
<html>
<head>
  <meta charset="utf-8">
  <title>Greentic Chat</title>
  <style>
    body{{font-family:-apple-system,BlinkMacSystemFont,Segoe UI,sans-serif;margin:0;background:#f6f7f9;color:#1f2937}}
    main{{max-width:860px;margin:0 auto;min-height:100vh;display:flex;flex-direction:column}}
    header{{padding:18px 20px;border-bottom:1px solid #d9dee7;background:#fff}}
    h1{{font-size:20px;margin:0}}
    #log{{flex:1;padding:18px 20px;overflow:auto}}
    .msg{{background:#fff;border:1px solid #d9dee7;border-radius:8px;padding:12px;margin:0 0 10px;max-width:720px}}
    .user{{margin-left:auto;background:#e8f2ff}}
    .card{{border-top:1px solid #e5e7eb;margin-top:10px;padding-top:10px}}
    .card input{{width:100%;box-sizing:border-box;padding:8px;margin:8px 0;border:1px solid #c8d0dc;border-radius:6px}}
    .card button{{margin:4px 6px 0 0;padding:8px 10px;border:1px solid #8aa1c4;background:#fff;border-radius:6px;cursor:pointer}}
    form{{display:flex;gap:8px;padding:14px 20px;border-top:1px solid #d9dee7;background:#fff}}
    form input{{flex:1;padding:10px;border:1px solid #c8d0dc;border-radius:6px}}
    form button{{padding:10px 14px;border:0;background:#216e4e;color:white;border-radius:6px}}
  </style>
</head>
<body>
<main>
  <header><h1>Greentic Chat</h1></header>
  <div id="log"></div>
  <form id="send"><input id="text" placeholder="Message Greentic" autocomplete="off"><button>Send</button></form>
</main>
<script>
const tenant="{tenant}";
let conversationId=sessionStorage.getItem("greenticConversationId")||"";
let watermark="0";
const log=document.getElementById("log");
function esc(v){{return String(v??"").replace(/[&<>"']/g,ch=>({{"&":"&amp;","<":"&lt;",">":"&gt;",'"':"&quot;","'":"&#39;"}}[ch]))}}
async function api(path, body){{const r=await fetch(path,{{method:"POST",headers:{{"Content-Type":"application/json"}},body:JSON.stringify(body||{{}})}}); return await r.json()}}
async function ensureConversation(){{
  if(conversationId) return;
  const j=await api(`/v1/messaging/webchat/${{encodeURIComponent(tenant)}}/v3/directline/conversations`,{{}});
  conversationId=j.conversationId;
  sessionStorage.setItem("greenticConversationId",conversationId);
}}
function renderCard(card){{
  const input=(card.body||[]).find(x=>x.type==="Input.Text");
  const inputId=input?.id||"greentic_tester_text";
  const actions=(card.actions||[]).map(a=>`<button type="button" data-action="${{esc(JSON.stringify(a.data||{{}}))}}">${{esc(a.title||"Action")}}</button>`).join("");
  return `<div class="card">${{(card.body||[]).filter(x=>x.type==="TextBlock").map(x=>`<p>${{esc(x.text)}}</p>`).join("")}}${{input?`<input data-input-id="${{esc(inputId)}}" placeholder="${{esc(input.placeholder||"")}}">`:""}}<div>${{actions}}</div></div>`;
}}
function render(activity){{
  const fromUser=(activity.from||{{}}).id==="user";
  const atts=(activity.attachments||[]).map(a=>a.contentType==="application/vnd.microsoft.card.adaptive"?renderCard(a.content):"").join("");
  const div=document.createElement("div");
  div.className=`msg ${{fromUser?"user":"bot"}}`;
  div.innerHTML=`<div>${{esc(activity.text||"")}}</div>${{atts}}`;
  div.querySelectorAll("button[data-action]").forEach(btn=>btn.addEventListener("click",async()=>{{
    const box=div.querySelector("[data-input-id]");
    const data=JSON.parse(btn.dataset.action||"{{}}");
    if(box) data[box.dataset.inputId]=box.value;
    await api(`/v1/messaging/webchat/${{encodeURIComponent(tenant)}}/v3/directline/conversations/${{conversationId}}/activities`,{{type:"message",text:data.action_id||btn.textContent,from:{{id:"user",name:"You"}},value:data}});
    await poll();
  }}));
  log.appendChild(div);
  log.scrollTop=log.scrollHeight;
}}
async function poll(){{
  if(!conversationId) return;
  const r=await fetch(`/v1/messaging/webchat/${{encodeURIComponent(tenant)}}/v3/directline/conversations/${{conversationId}}/activities?watermark=${{encodeURIComponent(watermark)}}`);
  const j=await r.json();
  (j.activities||[]).forEach(render);
  watermark=j.watermark||watermark;
}}
document.getElementById("send").addEventListener("submit",async e=>{{
  e.preventDefault();
  await ensureConversation();
  const input=document.getElementById("text");
  const text=input.value.trim();
  if(!text) return;
  input.value="";
  await api(`/v1/messaging/webchat/${{encodeURIComponent(tenant)}}/v3/directline/conversations/${{conversationId}}/activities`,{{type:"message",text,from:{{id:"user",name:"You"}}}});
  await poll();
}});
ensureConversation().then(poll);
setInterval(poll,2000);
</script>
</body>
</html>"""


def page():
    cfg = state()["config"]
    public = public_url() or "waiting for cloudflared..."
    ingress = ingress_url(cfg)
    return f"""<!doctype html>
<html>
<head>
  <meta charset="utf-8">
  <title>Greentic Teams Tester</title>
  <script type="module" src="/messaging-teams/assets/setup/greentic-teams-setup.js"></script>
  <style>
    body{{font-family:-apple-system,BlinkMacSystemFont,Segoe UI,sans-serif;margin:24px;max-width:1180px}}
    section{{border:1px solid #ddd;padding:16px;margin:12px 0}}
    label{{display:block;margin:8px 0}}
    input,textarea{{width:100%;box-sizing:border-box;padding:8px}}
    button{{margin:4px 6px 4px 0;padding:8px 12px}}
    pre{{white-space:pre-wrap;background:#f7f7f7;padding:12px;overflow:auto}}
    .row{{display:grid;grid-template-columns:1fr 1fr;gap:12px}}
    .muted{{color:#666}}
    .checklist{{list-style:none;padding-left:0}}
    .checklist li{{padding:4px 0}}
    .done{{color:#0a7f42;font-weight:600}}
    .pending{{color:#666}}
    .blocked{{color:#b42318;font-weight:700}}
    .callout{{border-left:4px solid #b42318;background:#fff4f2;padding:10px 12px;margin:12px 0}}
    code{{word-break:break-all}}
  </style>
</head>
<body>
  <h1>Greentic Teams Tester</h1>
  <section>
    <h2>Embedded Setup Component</h2>
    <p class="muted">This is the simplified reusable admin setup component. Use the raw controls below only for diagnostics.</p>
    <greentic-teams-setup api-base="" locale="en"></greentic-teams-setup>
  </section>
  <section>
    <h2>Endpoint</h2>
    <p>Public URL: <code id="publicUrl">{html.escape(public)}</code></p>
    <p>Greentic Webchat endpoint: <code id="webchatUrl">{html.escape(webchat_base_url(cfg))}</code></p>
    <p>Bot Framework ingress endpoint: <code id="ingressUrl">{html.escape(ingress)}</code></p>
    <p>Bot Framework SDK sidecar: <code id="botFrameworkSdkUrl">{html.escape(BOTFRAMEWORK_SDK_URL or "disabled")}</code></p>
    <p class="muted">Default setup publishes a native Teams bot app. Teams routes messages through the Bot Framework-compatible service registered for the bot app ID; the Teams app package does not carry a messaging endpoint.</p>
    <button onclick="publicTunnelSelfTest()">Test public tunnel to this tester</button>
    <pre id="tunnelOut"></pre>
  </section>
  <section>
    <h2>OAuth Setup</h2>
    <p class="muted">Use an admin-capable account. Graph login publishes and installs the Teams app. Azure login is only needed for legacy Bot Framework testing.</p>
    <div class="row">
      <label>Azure auth tenant<input id="azure_auth_tenant"></label>
      <label>Graph setup client ID<input id="graph_setup_client_id"></label>
    </div>
    <div class="row">
      <label>Azure setup client ID<input id="azure_setup_client_id"></label>
    </div>
    <button onclick="startGraphLogin()">Start Graph app-registration login</button>
    <button onclick="completeGraphLogin()">Complete Graph login</button>
    <button onclick="startAzureLogin()">Start legacy Azure Bot resource login</button>
    <button onclick="completeAzureLogin()">Complete Azure login</button>
    <button onclick="reconcileBotApp()">Create/reuse bot app registration</button>
    <button onclick="setupNext()">Run next setup step</button>
    <p class="muted">The one-button setup advances one step per click. In the default native Teams mode it publishes the Teams app but does not create or update Azure Bot Service.</p>
    <pre id="oauthOut"></pre>
  </section>
  <section>
    <h2>Admin Setup Status</h2>
    <div id="setupStatus"></div>
  </section>
  <section>
    <h2>Add to Teams</h2>
    <div id="teamsApp"></div>
  </section>
  <section>
    <h2>Bot Identity</h2>
    <div class="row">
      <label>Tenant<input id="tenant"></label>
      <label>Team<input id="team"></label>
    </div>
    <label>Runtime provider<input id="runtime_provider"></label>
    <label>Teams app version<input id="teams_app_version"></label>
    <label>Teams app ID<input id="teams_app_id"></label>
    <label>Bot display name<input id="bot_display_name"></label>
    <label>Microsoft Bot app ID<input id="bot_app_id"></label>
    <label>Microsoft Bot app password<input id="bot_app_password" type="password"></label>
    <button onclick="saveConfig()">Save config</button>
    <button onclick="testBotToken()">Test Bot Connector token</button>
    <pre id="configOut"></pre>
  </section>
  <section>
    <h2>Legacy Azure Bot Endpoint Reconcile</h2>
    <div class="row">
      <label>Azure subscription ID<input id="azure_subscription_id"></label>
      <label>Azure resource group<input id="azure_resource_group"></label>
    </div>
    <div class="row">
      <label>Resource group location<input id="azure_resource_group_location"></label>
      <label>Azure Bot name<input id="azure_bot_name"></label>
    </div>
    <div class="row">
      <label>Azure location<input id="azure_location"></label>
    </div>
    <button onclick="reconcile()">Reconcile messaging endpoint</button>
    <button onclick="verifyAzureBot()">Verify Azure Bot resource</button>
    <pre id="reconcileOut"></pre>
  </section>
  <section>
    <h2>Send Adaptive Card</h2>
    <label>Card text<textarea id="card_text">Greentic Teams adaptive card test</textarea></label>
    <button onclick="sendCard()">Send card to last conversation</button>
    <pre id="sendOut"></pre>
  </section>
  <section>
    <h2>Last Inbound</h2>
    <button onclick="simulateConversationUpdate()">Simulate conversationUpdate</button>
    <pre id="lastInbound"></pre>
  </section>
  <section>
    <h2>Events</h2>
    <button onclick="refresh()">Refresh</button>
    <pre id="events"></pre>
  </section>
<script>
const ids = ["tenant","team","runtime_provider","teams_app_version","teams_app_id","bot_display_name","bot_app_id","bot_app_password","azure_auth_tenant","graph_setup_client_id","azure_setup_client_id","azure_subscription_id","azure_resource_group","azure_resource_group_location","azure_bot_name","azure_location"];
function val(id){{return document.getElementById(id).value}}
function set(id,value){{document.getElementById(id).textContent = typeof value === "string" ? value : JSON.stringify(value,null,2)}}
function collect(){{const config={{}}; ids.forEach(id=>config[id]=val(id)); return {{config}}}}
async function api(path, body, target){{const r=await fetch(path,{{method:"POST",headers:{{"Content-Type":"application/json"}},body:JSON.stringify(body||{{}})}}); const j=await r.json(); set(target,j); await refresh(); return j}}
function esc(value){{return String(value ?? "").replace(/[&<>"']/g,ch=>({{"&":"&amp;","<":"&lt;",">":"&gt;",'"':"&quot;","'":"&#39;"}}[ch]))}}
function renderSetupStatus(status){{
  const target=document.getElementById("setupStatus");
  if(!target || !status) return;
  const icon={{done:"OK",pending:"..",blocked:"!!"}};
  const items=(status.items||[]).map(item=>`<li class="${{item.state}}"><span>${{icon[item.state]||"○"}}</span> ${{esc(item.label)}}${{item.detail?` <span class="muted">${{esc(item.detail)}}</span>`:""}}</li>`).join("");
  const selected=status.selected||{{}};
  let html=`<ul class="checklist">${{items}}</ul>`;
  html+=`<p class="muted">Next: ${{esc(status.next||"Click Run next setup step.")}}</p>`;
  html+=`<p><strong>Selected runtime</strong><br>Provider: <code>${{esc(selected.runtime_provider||"greentic-teams-bot")}}</code><br>Open chat: <code>${{esc(selected.greentic_chat_url||"")}}</code><br>Ingress endpoint: <code>${{esc(selected.messaging_endpoint||"")}}</code></p>`;
  if(selected.registration_note) html+=`<p class="muted">${{esc(selected.registration_note)}}</p>`;
  if(status.blocked){{
    const b=status.blocked;
    html+=`<div class="callout"><strong>${{esc(b.title||"Setup blocked")}}</strong><br>${{esc(b.summary||"")}}<br><br>Required action: <code>${{esc(b.missing_action||"")}}</code><br>Recommended role: ${{esc(b.recommended_role||"")}}<br>Recommended scope: <code>${{esc(b.recommended_scope||"")}}</code><br><br>${{esc(b.next||"")}}</div>`;
  }}
  target.innerHTML=html;
}}
function renderTeamsApp(app){{
  const target=document.getElementById("teamsApp");
  if(!target || !app) return;
  if(!app.ok){{
    target.innerHTML=`<p class="muted">Complete bot app setup first; a Microsoft Bot app ID is required.</p>`;
    return;
  }}
  const addLink=app.add_to_teams_url?`<p><a href="${{esc(app.add_to_teams_url)}}" target="_blank" rel="noopener">Add to Teams</a></p>`:`<p class="muted">Publish the Teams app before using Add to Teams.</p>`;
  const openLink=app.open_bot_chat_url?`<p><a href="${{esc(app.open_bot_chat_url)}}" target="_blank" rel="noopener">Open bot chat</a></p>`:"";
  target.innerHTML=`<p>Teams app ID: <code>${{esc(app.teams_app_id)}}</code><br>Catalog app ID: <code>${{esc(app.catalog_app_id||"not published")}}</code><br>Bot app ID: <code>${{esc(app.bot_app_id)}}</code></p>
  <p><button onclick="publishTeamsApp()">Publish Teams app to tenant catalog</button></p>
  <p><button onclick="installTeamsApp()">Install Teams app for me</button></p>
  <p><a href="/teams-app/package.zip">Download Teams app package</a></p>
  ${{addLink}}
  ${{openLink}}
  ${{app.runtime_provider==="greentic-webchat"?`<p><a href="${{esc(app.webchat_url)}}" target="_blank" rel="noopener">Open local webchat</a></p>`:""}}
  <p class="muted">${{esc(app.note)}}</p>`;
}}
async function load(){{const r=await fetch("/api/state"); const s=await r.json(); const c=s.values.config||{{}}; ids.forEach(id=>{{if(document.getElementById(id)) document.getElementById(id).value=c[id]||""}}); renderSetupStatus(s.setup_status); renderTeamsApp(s.teams_app); set("lastInbound",{{activity:s.values.last_activity,webchat:s.values.last_webchat_conversation,envelope:s.values.last_envelope}}); set("events",s.events); document.getElementById("publicUrl").textContent=s.public_url||"waiting for cloudflared..."; document.getElementById("ingressUrl").textContent=s.ingress_url||""; document.getElementById("webchatUrl").textContent=s.teams_app.directline_base||""; document.getElementById("botFrameworkSdkUrl").textContent=s.botframework_sdk?.enabled?s.botframework_sdk.url:"disabled"}}
async function refresh(){{await load()}}
async function saveConfig(){{await api("/api/config",collect(),"configOut")}}
async function testBotToken(){{await api("/api/bot/token",collect(),"configOut")}}
async function publicTunnelSelfTest(){{await api("/api/diagnostic/public-tunnel",collect(),"tunnelOut")}}
function findDeviceLogin(value){{
  if(!value || typeof value !== "object") return null;
  const url=value.verification_uri||value.verification_url;
  if(url) return {{url, userCode:value.user_code, message:value.message}};
  return findDeviceLogin(value.body)||findDeviceLogin(value.result)||null;
}}
function maybeOpenDeviceLogin(j){{
  const login=findDeviceLogin(j);
  if(login && login.url) window.open(login.url,"_blank","noopener");
}}
function pendingDeviceLogin(j){{
  const body=j?.result?.body||j?.body||{{}};
  return String(j?.step||"").startsWith("wait_for_") && (body.error==="authorization_pending" || body.error==="slow_down");
}}
function pendingIntervalMs(j){{
  const body=j?.result?.body||j?.body||{{}};
  const seconds=Number(body.interval||5);
  return Math.max(5, seconds) * 1000;
}}
async function startGraphLogin(){{maybeOpenDeviceLogin(await api("/api/oauth/graph/start",collect(),"oauthOut"))}}
async function completeGraphLogin(){{await api("/api/oauth/graph/complete",collect(),"oauthOut")}}
async function startAzureLogin(){{maybeOpenDeviceLogin(await api("/api/oauth/management/start",collect(),"oauthOut"))}}
async function completeAzureLogin(){{await api("/api/oauth/management/complete",collect(),"oauthOut")}}
async function reconcileBotApp(){{await api("/api/bot-app/reconcile",collect(),"oauthOut")}}
async function setupNext(){{
  let j=await api("/api/setup/next",collect(),"oauthOut");
  maybeOpenDeviceLogin(j);
  for(let attempts=0; attempts<180 && pendingDeviceLogin(j); attempts++){{
    await new Promise(resolve=>setTimeout(resolve,pendingIntervalMs(j)));
    j=await api("/api/setup/next",collect(),"oauthOut");
  }}
}}
async function publishTeamsApp(){{maybeOpenDeviceLogin(await api("/api/teams-app/publish",collect(),"oauthOut"))}}
async function installTeamsApp(){{maybeOpenDeviceLogin(await api("/api/teams-app/install-me",collect(),"oauthOut"))}}
async function reconcile(){{await api("/api/azure/reconcile",collect(),"reconcileOut")}}
async function verifyAzureBot(){{await api("/api/azure/verify",collect(),"reconcileOut")}}
async function sendCard(){{await api("/api/send/card",{{...collect(), text:val("card_text")}},"sendOut")}}
async function simulateConversationUpdate(){{await api("/api/simulate/conversation-update",collect(),"lastInbound")}}
load(); setInterval(refresh,3000);
</script>
</body>
</html>"""


class Handler(BaseHTTPRequestHandler):
    server_version = "GreenticTeamsBotTest/1.0"

    def log_message(self, fmt, *args):
        append_event("server-log", fmt % args)

    def read_body(self):
        length = int(self.headers.get("content-length") or 0)
        raw = self.rfile.read(length) if length else b""
        if not raw:
            return {}
        try:
            return json.loads(raw.decode("utf-8"))
        except json.JSONDecodeError:
            return {"_raw": raw.decode("utf-8", errors="replace")}

    def send_json(self, value, status=200):
        body = json.dumps(value, indent=2).encode("utf-8")
        self.send_response(status)
        self.send_header("Content-Type", "application/json")
        self.send_header("Content-Length", str(len(body)))
        self.end_headers()
        self.wfile.write(body)

    def send_empty(self, status=200):
        self.send_response(status)
        self.send_header("Content-Length", "0")
        self.end_headers()

    def send_text(self, value, status=200, content_type="text/plain", no_store=False):
        body = value.encode("utf-8")
        self.send_response(status)
        self.send_header("Content-Type", content_type)
        if no_store:
            self.send_header("Cache-Control", "no-store")
        self.send_header("Content-Length", str(len(body)))
        self.end_headers()
        self.wfile.write(body)

    def send_bytes(self, body, status=200, content_type="application/octet-stream", filename=None):
        self.send_response(status)
        self.send_header("Content-Type", content_type)
        self.send_header("Content-Length", str(len(body)))
        if filename:
            self.send_header("Content-Disposition", f'attachment; filename="{filename}"')
        self.end_headers()
        self.wfile.write(body)

    def do_GET(self):
        parsed = urlparse(self.path)
        if parsed.path == "/":
            self.send_text(page(), content_type="text/html")
            return
        if parsed.path == "/webchat/teams":
            query = parse_qs(parsed.query)
            tenant = (query.get("tenant") or ["default"])[0] or "default"
            self.send_text(webchat_page(tenant), content_type="text/html")
            return
        if parsed.path == "/messaging-teams/assets/setup/greentic-teams-setup.js":
            path = ROOT_DIR / "messaging-teams" / "assets" / "setup" / "greentic-teams-setup.js"
            try:
                self.send_text(path.read_text(encoding="utf-8"), content_type="text/javascript; charset=utf-8", no_store=True)
            except FileNotFoundError:
                self.send_json({"ok": False, "error": "web component source not found", "path": str(path)}, status=404)
            return
        if parsed.path == "/api/state":
            values = state()
            self.send_json({
                "ok": True,
                "public_url": public_url(),
                "ingress_url": ingress_url(values["config"]),
                "botframework_sdk": {
                    "enabled": botframework_sdk_enabled(),
                    "url": BOTFRAMEWORK_SDK_URL,
                },
                "values": sanitize(values),
                "setup_status": setup_status(),
                "teams_app": teams_app_info(values["config"]),
                "events": recent_events(),
            })
            return
        if parsed.path == "/teams-app/manifest.json":
            manifest = teams_app_manifest(state()["config"])
            if manifest.get("ok") is False:
                self.send_json(manifest, status=400)
                return
            self.send_json(manifest)
            return
        if parsed.path == "/teams-app/package.zip":
            body, error = teams_app_package(state()["config"])
            if error:
                self.send_json(error, status=400)
                return
            self.send_bytes(body, content_type="application/zip", filename="greentic-teams-bot.zip")
            return
        match = re.fullmatch(r"/v1/messaging/webchat/([^/]+)/v3/directline/conversations/([^/]+)/activities", parsed.path)
        if match:
            tenant, conversation_id = match.groups()
            query = parse_qs(parsed.query)
            watermark = 0
            try:
                watermark = int((query.get("watermark") or ["0"])[0] or "0")
            except ValueError:
                watermark = 0
            conversation = webchat_conversation(tenant, conversation_id)
            activities = conversation.get("activities") or []
            self.send_json({
                "activities": activities[watermark:],
                "watermark": str(len(activities)),
            })
            return
        self.send_json({"ok": False, "error": "not found", "path": parsed.path}, status=404)

    def do_POST(self):
        parsed = urlparse(self.path)
        body = self.read_body()
        try:
            if parsed.path == "/api/config":
                self.send_json({"ok": True, "values": save_client_state(body)})
                return
            if parsed.path == "/api/bot/token":
                values = save_client_state(body)
                result = acquire_bot_token(state()["config"])
                safe = dict(result)
                if safe.get("access_token"):
                    safe["access_token"] = "set"
                append_event("bot-token-test", safe)
                self.send_json(safe)
                return
            if parsed.path == "/api/oauth/graph/start":
                save_client_state(body)
                self.send_json(start_oauth_device("graph", state()["config"]))
                return
            if parsed.path == "/api/oauth/graph/complete":
                save_client_state(body)
                self.send_json(complete_oauth_device("graph", state()["config"]))
                return
            if parsed.path == "/api/oauth/management/start":
                save_client_state(body)
                self.send_json(start_oauth_device("management", state()["config"]))
                return
            if parsed.path == "/api/oauth/management/complete":
                save_client_state(body)
                self.send_json(complete_oauth_device("management", state()["config"]))
                return
            if parsed.path == "/api/bot-app/reconcile":
                save_client_state(body)
                self.send_json(reconcile_bot_app(state()["config"]))
                return
            if parsed.path == "/api/setup/next":
                save_client_state(body)
                self.send_json(remember_setup_result(next_setup_step(state()["config"])))
                return
            if parsed.path == "/api/azure/reconcile":
                save_client_state(body)
                self.send_json(reconcile_azure_bot(state()["config"]))
                return
            if parsed.path == "/v1/setup/bot-framework/registration":
                self.send_json(register_bot_framework_endpoint(body))
                return
            if parsed.path == "/api/azure/verify":
                save_client_state(body)
                self.send_json(verify_azure_bot(state()["config"]))
                return
            if parsed.path == "/api/teams-app/publish":
                save_client_state(body)
                result = publish_teams_app(state()["config"])
                if graph_auth_needs_login(result):
                    result = start_graph_login_after_expiry(state()["config"], result)
                self.send_json(result)
                return
            if parsed.path == "/api/teams-app/install-me":
                save_client_state(body)
                result = install_teams_app_for_me(state()["config"])
                if graph_auth_needs_login(result):
                    result = start_graph_login_after_expiry(state()["config"], result)
                self.send_json(result)
                return
            if parsed.path == "/api/diagnostic/public-tunnel":
                save_client_state(body)
                self.send_json(public_tunnel_self_test(state()["config"]))
                return
            if parsed.path == "/api/diagnostic/public-ping":
                append_event("public-ping-received", {"body": body, "headers": {k: v for k, v in self.headers.items()}})
                self.send_json({"ok": True, "received_at": now_iso()})
                return
            if parsed.path == "/api/send/card":
                save_client_state(body)
                self.send_json(send_card(state()["config"], body.get("text")))
                return
            if parsed.path == "/api/simulate/conversation-update":
                save_client_state(body)
                values = state()
                cfg = values["config"]
                tenant = cfg.get("tenant") or "default"
                team = cfg.get("team") or "default"
                activity = {
                    "type": "conversationUpdate",
                    "id": f"activity-{int(time.time() * 1000)}",
                    "serviceUrl": "https://smba.trafficmanager.net/emea/",
                    "channelId": "msteams",
                    "conversation": {"id": "conv-local", "conversationType": "personal"},
                    "from": {"id": "user-local", "name": "Local User"},
                    "recipient": {"id": cfg.get("bot_app_id") or "28:bot-local", "name": cfg.get("bot_display_name") or "Greentic"},
                    "membersAdded": [
                        {"id": cfg.get("bot_app_id") or "28:bot-local", "name": cfg.get("bot_display_name") or "Greentic"},
                        {"id": "user-local", "name": "Local User"},
                    ],
                    "channelData": {"tenant": {"id": tenant}, "team": {"id": team}},
                }
                envelope = normalize_activity(activity, tenant, team)
                values["last_activity"] = activity
                values["last_activity_received_at"] = now_iso()
                values["last_envelope"] = envelope
                save_state(values)
                append_event("simulate-lifecycle", {"activity": activity, "envelope": envelope})
                self.send_json({"ok": True, "activity": activity, "envelope": envelope})
                return
            match = re.fullmatch(r"/v1/messaging/webchat/([^/]+)/v3/directline/tokens/generate", parsed.path)
            if match:
                tenant = match.group(1)
                self.send_json({
                    "token": f"local-{tenant}-{uuid.uuid4()}",
                    "expires_in": 3600,
                })
                return
            match = re.fullmatch(r"/v1/messaging/webchat/([^/]+)/v3/directline/conversations", parsed.path)
            if match:
                self.send_json(create_webchat_conversation(match.group(1)), status=201)
                return
            match = re.fullmatch(r"/v1/messaging/webchat/([^/]+)/v3/directline/conversations/([^/]+)/activities", parsed.path)
            if match:
                tenant, conversation_id = match.groups()
                self.send_json(append_webchat_inbound(tenant, conversation_id, body), status=201)
                return
            match = re.fullmatch(r"/v1/messaging/ingress/messaging-teams/([^/]+)/([^/]+)", parsed.path)
            if match:
                tenant, team = match.groups()
                if botframework_sdk_enabled():
                    proxied = proxy_botframework_activity(body, self.headers)
                    append_event("botframework-sdk-proxy", {"status": proxied.get("status"), "ok": proxied.get("ok"), "body": proxied.get("body"), "error": proxied.get("error")})
                    if proxied.get("ok"):
                        status = proxied.get("status") or 200
                        response_body = proxied.get("body")
                        if response_body is None:
                            self.send_empty(status)
                        else:
                            self.send_json(response_body, status=status)
                        return
                    self.send_json({**proxied, "step": "proxy_to_botframework_sdk"}, status=502)
                    return
                envelope = normalize_activity(body, tenant, team)
                values = state()
                values["config"]["tenant"] = tenant
                values["config"]["team"] = team
                values["last_activity"] = body
                values["last_activity_received_at"] = now_iso()
                values["last_envelope"] = envelope
                save_state(values)
                append_event("bot-activity-received", {"activity": body, "envelope": envelope})
                if body.get("type") == "invoke":
                    payload = action_submit_payload(body)
                    action_id = submit_action_id(body)
                    text = submit_text(body)
                    Thread(
                        target=send_action_result_card,
                        args=(
                            dict(state()["config"]),
                            body,
                            action_id,
                            text,
                        ),
                        daemon=True,
                    ).start()
                    append_event("bot-action-invoke-ack", {
                        "action_id": action_id,
                        "submitted_text": text,
                        "action_type": payload.get("action_type"),
                        "mode": (state()["config"].get("invoke_response_mode") or os.environ.get("GREENTIC_TEAMS_INVOKE_RESPONSE_MODE") or "adaptive").strip().lower(),
                    })
                    if payload.get("action_type") != "Action.Execute":
                        self.send_empty()
                        return
                    self.send_json(invoke_http_response())
                    return
                if body.get("type") == "message" and (envelope.get("event") or {}).get("kind") == "action":
                    action_id = submit_action_id(body)
                    text = submit_text(body)
                    Thread(
                        target=send_action_result_card,
                        args=(
                            dict(state()["config"]),
                            body,
                            action_id,
                            text,
                        ),
                        daemon=True,
                    ).start()
                    append_event("bot-action-message-ack", {
                        "action_id": action_id,
                        "submitted_text": text,
                    })
                    self.send_empty()
                    return
                self.send_json({"ok": True, "envelopes": [envelope]})
                return
            self.send_json({"ok": False, "error": "not found", "path": parsed.path}, status=404)
        except Exception as exc:
            append_event("server-error", {"path": parsed.path, "error": str(exc)})
            self.send_json({"ok": False, "error": str(exc)}, status=500)


if __name__ == "__main__":
    mark_server_started()
    ThreadingHTTPServer(("127.0.0.1", PORT), Handler).serve_forever()
PY

cat > "${WORK_DIR}/botframework-node-bot.js" <<'NODE'
const fs = require('fs');
const path = require('path');
const express = require('express');
const { BotFrameworkAdapter, CardFactory } = require('botbuilder');

const workDir = process.env.WORK_DIR;
const port = Number(process.env.BOTFRAMEWORK_PORT || '8794');
const valuesPath = path.join(workDir, 'teams-bot-values.json');
const eventsPath = path.join(workDir, 'teams-bot-events.jsonl');

function nowIso() {
  return new Date().toISOString().replace(/\.\d{3}Z$/, 'Z');
}

function readJson(file, fallback) {
  try {
    return JSON.parse(fs.readFileSync(file, 'utf8'));
  } catch {
    return fallback;
  }
}

function writeJson(file, value) {
  fs.writeFileSync(file, `${JSON.stringify(value, null, 2)}\n`, 'utf8');
}

function appendEvent(kind, payload) {
  fs.appendFileSync(eventsPath, `${JSON.stringify({ ts: nowIso(), kind, payload })}\n`, 'utf8');
}

function state() {
  const values = readJson(valuesPath, {});
  values.config = values.config || {};
  return values;
}

function saveState(values) {
  const current = state();
  current.config = { ...(current.config || {}), ...(values.config || {}) };
  for (const key of [
    'last_activity',
    'last_activity_received_at',
    'last_envelope',
    'last_send',
    'last_setup_result',
  ]) {
    if (Object.prototype.hasOwnProperty.call(values, key)) {
      current[key] = values[key];
    }
  }
  writeJson(valuesPath, current);
  return current;
}

function configured(name, fallback = '') {
  const cfg = state().config || {};
  return String(cfg[name] || process.env[name.toUpperCase()] || fallback || '').trim();
}

function parseMaybeJson(value) {
  if (typeof value !== 'string') return value;
  try {
    return JSON.parse(value);
  } catch {
    return value;
  }
}

function submitPayload(activity) {
  const value = parseMaybeJson(activity.value || {});
  if (value && typeof value === 'object') {
    const nested = parseMaybeJson(value?.msteams?.value);
    return nested && typeof nested === 'object' ? { ...value, ...nested } : value;
  }
  return {};
}

function submitActionId(activity) {
  const payload = submitPayload(activity);
  return payload.action_id || payload.action || payload.verb || activity.text || activity.name || 'unknown_action';
}

function submitText(activity) {
  const payload = submitPayload(activity);
  const state = activity.value && typeof activity.value === 'object' && activity.value.state && typeof activity.value.state === 'object'
    ? activity.value.state
    : {};
  return payload.greentic_tester_text || state.greentic_tester_text || payload.text || payload.input || '';
}

function normalizeActivity(activity, tenant, team) {
  const conversation = activity.conversation || {};
  const from = activity.from || {};
  const payload = submitPayload(activity);
  const isAction = Boolean(activity.value) || activity.type === 'invoke';
  return {
    id: activity.id || '',
    provider: 'messaging-teams',
    tenant,
    team,
    event: {
      kind: isAction ? 'action' : 'message',
      text: activity.text || '',
      action_id: isAction ? submitActionId(activity) : undefined,
      submitted_text: isAction ? submitText(activity) : undefined,
      payload,
    },
    sender: {
      id: from.id || '',
      name: from.name || '',
    },
    conversation: {
      id: conversation.id || '',
      type: conversation.conversationType || '',
      service_url: activity.serviceUrl || '',
    },
    raw: activity,
  };
}

function resultCard(actionId, text) {
  return {
    $schema: 'http://adaptivecards.io/schemas/adaptive-card.json',
    type: 'AdaptiveCard',
    version: '1.5',
    body: [
      {
        type: 'TextBlock',
        text: 'Button clicked',
        weight: 'Bolder',
        size: 'Large',
        wrap: true,
      },
      {
        type: 'FactSet',
        facts: [
          { title: 'Button', value: String(actionId || 'unknown_action') },
          { title: 'Text', value: String(text || '(empty)') },
        ],
      },
    ],
  };
}

function resultActivity(actionId, text) {
  return {
    type: 'message',
    text: `Button clicked: ${actionId || 'unknown_action'}; text: ${text || '(empty)'}`,
    attachments: [CardFactory.adaptiveCard(resultCard(actionId, text))],
  };
}

const cfg = state().config || {};
const adapter = new BotFrameworkAdapter({
  appId: cfg.bot_app_id || process.env.GREENTIC_TEAMS_BOT_APP_ID || '',
  appPassword: cfg.bot_app_password || process.env.GREENTIC_TEAMS_BOT_APP_PASSWORD || '',
  channelAuthTenant: cfg.bot_app_tenant_id || process.env.GREENTIC_TEAMS_BOT_APP_TENANT_ID || process.env.AZURE_TENANT_ID || undefined,
  channelService: process.env.ChannelService || undefined,
});

adapter.onTurnError = async (context, error) => {
  appendEvent('botframework-sdk-error', { error: String(error && error.stack || error) });
  try {
    await context.sendActivity('Bot Framework SDK error. Check tester events.');
  } catch {}
};

const app = express();
app.use(express.json({ limit: '2mb' }));

app.get('/healthz', (_req, res) => {
  res.json({ ok: true, runtime: 'botbuilder-node' });
});

app.post('/api/messages', async (req, res) => {
  try {
    await adapter.processActivity(req, res, async (context) => {
    const activity = context.activity || {};
    const current = state();
    const tenant = current.config?.tenant || 'default';
    const team = current.config?.team || 'default';
    const envelope = normalizeActivity(activity, tenant, team);
    saveState({
      config: { tenant, team },
      last_activity: activity,
      last_activity_received_at: nowIso(),
      last_envelope: envelope,
    });
    appendEvent('botframework-sdk-activity-received', {
      type: activity.type,
      name: activity.name,
      text: activity.text,
      conversation_id: activity.conversation && activity.conversation.id,
      service_url: activity.serviceUrl,
      envelope,
    });

    if (activity.type === 'message' && activity.value) {
      const actionId = submitActionId(activity);
      const text = submitText(activity);
      await context.sendActivity(resultActivity(actionId, text));
      return;
    }

    if (activity.type === 'invoke') {
      const actionId = submitActionId(activity);
      const text = submitText(activity);
      await context.sendActivity(resultActivity(actionId, text));
      await context.sendActivity({
        type: 'invokeResponse',
        value: {
          status: 200,
          body: resultActivity(actionId, text),
        },
      });
      return;
    }

    if (activity.type === 'message' && activity.text) {
      await context.sendActivity(`Greentic Bot Framework SDK received: ${activity.text}`);
    }
    });
  } catch (error) {
    appendEvent('botframework-sdk-process-error', { error: String(error && error.stack || error) });
    if (!res.headersSent) {
      res.status(401).send(String(error && error.message || error));
    }
  }
});

app.listen(port, '127.0.0.1', () => {
  appendEvent('botframework-sdk-started', { port, runtime: 'botbuilder-node' });
  console.log(`Bot Framework SDK sidecar listening on http://127.0.0.1:${port}`);
});
NODE

: > "${WORK_DIR}/public-url.txt"

export ROOT_DIR WORK_DIR PORT LOCAL_URL BOTFRAMEWORK_PORT

BOTFRAMEWORK_PID=""
if [ "${BOTFRAMEWORK_SDK}" = "botbuilder-node" ]; then
  if ! command -v node >/dev/null 2>&1 || ! command -v npm >/dev/null 2>&1; then
    echo "Node.js and npm are required for GREENTIC_TEAMS_BOT_FRAMEWORK=botbuilder-node" >&2
    exit 1
  fi
  if [ ! -d "${WORK_DIR}/node_modules/botbuilder" ] || [ ! -d "${WORK_DIR}/node_modules/express" ]; then
    npm install --silent --prefix "${WORK_DIR}" botbuilder@4 express@4
  fi
  BOTFRAMEWORK_SDK_URL="http://127.0.0.1:${BOTFRAMEWORK_PORT}"
  export BOTFRAMEWORK_SDK_URL
  node "${WORK_DIR}/botframework-node-bot.js" >"${WORK_DIR}/botframework-node.log" 2>&1 &
  BOTFRAMEWORK_PID=$!
  BOTFRAMEWORK_READY=0
  for _ in {1..100}; do
    if curl -fsS "${BOTFRAMEWORK_SDK_URL}/healthz" >/dev/null 2>&1; then
      BOTFRAMEWORK_READY=1
      break
    fi
    sleep 0.1
  done
  if [ "${BOTFRAMEWORK_READY}" -ne 1 ]; then
    echo "Bot Framework SDK sidecar failed to start; log follows:" >&2
    cat "${WORK_DIR}/botframework-node.log" >&2 || true
    exit 1
  fi
elif [ "${BOTFRAMEWORK_SDK}" != "none" ]; then
  echo "unsupported GREENTIC_TEAMS_BOT_FRAMEWORK=${BOTFRAMEWORK_SDK}; use botbuilder-node or none" >&2
  exit 2
fi

python3 "${WORK_DIR}/server.py" &
SERVER_PID=$!

cleanup() {
  if [ -n "${BOTFRAMEWORK_PID:-}" ]; then
    kill "${BOTFRAMEWORK_PID}" >/dev/null 2>&1 || true
  fi
  kill "${SERVER_PID}" >/dev/null 2>&1 || true
  if [ -n "${CLOUDFLARED_PID:-}" ]; then
    kill "${CLOUDFLARED_PID}" >/dev/null 2>&1 || true
  fi
}
trap cleanup EXIT

SERVER_READY=0
for _ in {1..50}; do
  if curl -fsS "${LOCAL_URL}/api/state" >/dev/null 2>&1; then
    SERVER_READY=1
    break
  fi
  sleep 0.1
done
if [ "${SERVER_READY}" -ne 1 ]; then
  echo "local Greentic Teams tester failed to start" >&2
  exit 1
fi

if command -v "${CLOUDFLARED_BIN}" >/dev/null 2>&1; then
  "${CLOUDFLARED_BIN}" tunnel --url "${LOCAL_URL}" --no-autoupdate >"${WORK_DIR}/cloudflared.log" 2>&1 &
  CLOUDFLARED_PID=$!
  for _ in {1..80}; do
    PUBLIC_URL="$(grep -o 'https://[-a-zA-Z0-9.]*\.trycloudflare\.com' "${WORK_DIR}/cloudflared.log" | grep -v '^https://api\.trycloudflare\.com$' | head -n1 || true)"
    if [ -n "${PUBLIC_URL}" ]; then
      echo "${PUBLIC_URL}" > "${WORK_DIR}/public-url.txt"
      break
    fi
    sleep 0.25
  done
else
  echo "cloudflared not found; continuing with local URL only" >&2
fi

PUBLIC_URL="$(cat "${WORK_DIR}/public-url.txt")"
echo "Greentic Teams tester: ${LOCAL_URL}"
if [ -n "${PUBLIC_URL}" ]; then
  echo "Public URL: ${PUBLIC_URL}"
  echo "Greentic Webchat endpoint: ${PUBLIC_URL}/v1/messaging/webchat/default"
  echo "Bot Framework ingress endpoint: ${PUBLIC_URL}/v1/messaging/ingress/messaging-teams/default/default"
else
  echo "Public URL: not ready"
  echo "Bot Framework ingress endpoint: not ready; wait for cloudflared, then use the tester UI to retry setup"
fi
echo "Work dir: ${WORK_DIR}"

if [ "${NO_OPEN}" -eq 0 ]; then
  if command -v open >/dev/null 2>&1; then
    open "${LOCAL_URL}" >/dev/null 2>&1 || true
  fi
fi

wait "${SERVER_PID}"
