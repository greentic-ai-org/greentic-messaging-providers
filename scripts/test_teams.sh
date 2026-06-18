#!/usr/bin/env bash
set -euo pipefail

PORT=8792
NO_BUILD=0
NO_OPEN=0

usage() {
  cat <<'EOF'
Usage: scripts/test_teams.sh [--port <port>] [--no-build] [--no-open]

Starts a local Microsoft Teams Graph tester UI with a cloudflared public URL.
The tester supports Microsoft OAuth, Graph discovery, provider-path sends,
Graph subscription calls, validationToken handling, and incoming event logs.

Environment:
  CLOUDFLARED_BIN
  GREENTIC_TEAMS_CLIENT_ID
  GREENTIC_TEAMS_TENANT_ID
  GREENTIC_TEAMS_SCOPES
EOF
}

while [[ $# -gt 0 ]]; do
  case "$1" in
    --port)
      PORT="${2:?--port requires a value}"
      shift 2
      ;;
    --no-build)
      NO_BUILD=1
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
TESTER="${ROOT_DIR}/target/debug/greentic-messaging-tester"
CLOUDFLARED_BIN="${CLOUDFLARED_BIN:-cloudflared}"
WORK_DIR="${TMPDIR:-/tmp}/greentic-teams-test-${PORT}"
LOCAL_URL="http://localhost:${PORT}"

if [[ "${NO_BUILD}" -eq 0 ]]; then
  "${ROOT_DIR}/scripts/build_providers.sh" teams
  cargo build -p greentic-messaging-tester --manifest-path "${ROOT_DIR}/Cargo.toml"
fi

mkdir -p "${WORK_DIR}"

cat > "${WORK_DIR}/server.py" <<'PY'
import base64
import html as html_lib
import json
import os
import re
import time
from datetime import datetime, timedelta, timezone
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
from pathlib import Path
from urllib.parse import parse_qs, unquote, urlencode, urlparse
from urllib.request import Request, urlopen
from urllib.error import HTTPError, URLError

ROOT = Path(os.environ["ROOT_DIR"])
WORK = Path(os.environ["WORK_DIR"])
PORT = int(os.environ["PORT"])
LOCAL_BASE_URL = os.environ.get("LOCAL_URL") or f"http://127.0.0.1:{PORT}"
VALUES = WORK / "teams-values.json"
EVENTS = WORK / "teams-events.jsonl"
PUBLIC_URL_FILE = WORK / "public-url.txt"

DEFAULT_CLIENT_ID = "6c115a7a-f656-49c4-975a-5e831efae833"
OLD_DEFAULT_CLIENT_IDS = {"7631ab74-7abe-4d2d-895e-624b2dc3983e"}
DEFAULT_TENANT_ALIAS = "organizations"
REQUIRED_SCOPES = [
    "offline_access",
    "openid",
    "profile",
    "User.Read",
    "Team.ReadBasic.All",
    "Channel.ReadBasic.All",
    "ChannelMessage.Send",
    "ChannelMessage.Read.All",
    "Chat.Read",
]
DEFAULT_SCOPES = " ".join(REQUIRED_SCOPES)
DEVICE_LOGIN_CHECKLIST = [
    "App registration must be multi-tenant: signInAudience = AzureADMultipleOrgs",
    "Public client/device code flow must be enabled",
    "Tenant may require admin consent",
    "Teams message subscriptions require delegated read scopes such as ChannelMessage.Read.All for channel messages and Chat.Read for chat messages",
    "Device/token endpoints must use organizations, not the developer's tenant ID",
]


def b64url_json(segment):
    padded = segment + "=" * (-len(segment) % 4)
    return json.loads(base64.urlsafe_b64decode(padded.encode("utf-8")).decode("utf-8"))


def tenant_from_id_token(id_token):
    try:
        parts = id_token.split(".")
        if len(parts) < 2:
            return None
        claims = b64url_json(parts[1])
        return claims.get("tid")
    except Exception:
        return None


def ensure_scopes(scopes):
    seen = {}
    for scope in (scopes or "").split():
        seen[scope.lower()] = scope
    for scope in REQUIRED_SCOPES:
        seen.setdefault(scope.lower(), scope)
    ordered = []
    emitted = set()
    for scope in (scopes or "").split() + REQUIRED_SCOPES:
        key = scope.lower()
        if key not in emitted and key in seen:
            ordered.append(seen[key])
            emitted.add(key)
    return " ".join(ordered)


def read_json(path, default):
    try:
        return json.loads(path.read_text(encoding="utf-8"))
    except FileNotFoundError:
        return default
    except json.JSONDecodeError:
        return default


def write_json(path, data):
    path.write_text(json.dumps(data, indent=2) + "\n", encoding="utf-8")


def public_url():
    try:
        return PUBLIC_URL_FILE.read_text(encoding="utf-8").strip()
    except FileNotFoundError:
        return ""


def graph_datetime(dt):
    return dt.astimezone(timezone.utc).replace(microsecond=0).isoformat().replace("+00:00", "Z")


def default_subscription_expiration():
    return graph_datetime(datetime.now(timezone.utc) + timedelta(days=2))


def parse_graph_datetime(value):
    return datetime.fromisoformat(value.replace("Z", "+00:00")).astimezone(timezone.utc)


def state():
    values = read_json(VALUES, {})
    values.setdefault("config", {})
    values.setdefault("secrets", {})
    values.setdefault("http", "real")
    cfg = values["config"]
    secrets_obj = values["secrets"]
    env_tenant_id = os.environ.get("GREENTIC_TEAMS_TENANT_ID", DEFAULT_TENANT_ALIAS)
    if not cfg.get("tenant_id") or (cfg.get("tenant_id") == "common" and not os.environ.get("GREENTIC_TEAMS_TENANT_ID")):
        cfg["tenant_id"] = env_tenant_id
    env_client_id = os.environ.get("GREENTIC_TEAMS_CLIENT_ID", DEFAULT_CLIENT_ID)
    if cfg.get("client_id") in OLD_DEFAULT_CLIENT_IDS:
        cfg["client_id"] = env_client_id
    cfg.setdefault("client_id", env_client_id)
    cfg.setdefault("graph_base_url", "https://graph.microsoft.com/v1.0")
    cfg.setdefault("auth_base_url", "https://login.microsoftonline.com")
    cfg.setdefault("token_scope", "https://graph.microsoft.com/.default")
    cfg["oauth_scopes"] = ensure_scopes(cfg.get("oauth_scopes") or os.environ.get("GREENTIC_TEAMS_SCOPES", DEFAULT_SCOPES))
    cfg.setdefault("public_base_url", public_url())
    secrets_obj.setdefault("MS_GRAPH_CLIENT_ID", cfg.get("client_id") or env_client_id)
    return values


def save_state(data):
    current = state()
    current["config"].update(data.get("config", {}))
    current["secrets"].update(data.get("secrets", {}))
    if data.get("http"):
        current["http"] = data["http"]
    write_json(VALUES, current)
    return sanitized(current)


def sanitized(data):
    clone = json.loads(json.dumps(data))
    for key in ("MS_GRAPH_REFRESH_TOKEN", "MS_GRAPH_ACCESS_TOKEN", "MS_GRAPH_DEVICE_CODE"):
        if clone.get("secrets", {}).get(key):
            clone["secrets"][key] = "set"
    return clone


def append_event(kind, payload):
    with EVENTS.open("a", encoding="utf-8") as f:
        f.write(json.dumps({"ts": time.time(), "kind": kind, "payload": payload}) + "\n")


def recent_events():
    try:
        lines = EVENTS.read_text(encoding="utf-8").splitlines()[-100:]
    except FileNotFoundError:
        lines = []
    return [json.loads(line) for line in lines if line.strip()]


def graph_error_hint(exc):
    msg = str(exc)
    if "Caller does not have access" in msg and "/messages" in msg:
        return {
            "error": msg,
            "hint": "This token/app likely still lacks subscription read consent. Re-login after the scopes field includes ChannelMessage.Read.All. If Microsoft does not prompt for it, add ChannelMessage.Read.All as a delegated API permission and grant admin consent in Entra, then start device login again.",
            "required_scopes": " ".join(REQUIRED_SCOPES),
        }
    return {"error": msg}


def graph_request(method, path, token, body=None):
    base = state()["config"].get("graph_base_url") or "https://graph.microsoft.com/v1.0"
    url = path if path.startswith("http") else base.rstrip("/") + "/" + path.lstrip("/")
    data = None if body is None else json.dumps(body).encode("utf-8")
    headers = {"Authorization": f"Bearer {token}", "Accept": "application/json"}
    if data is not None:
        headers["Content-Type"] = "application/json"
    req = Request(url, data=data, headers=headers, method=method)
    try:
        with urlopen(req, timeout=30) as resp:
            raw = resp.read().decode("utf-8")
            return json.loads(raw) if raw else {}
    except HTTPError as exc:
        raw = exc.read().decode("utf-8", errors="replace")
        raise RuntimeError(f"Graph HTTP {exc.code}: {raw}") from exc
    except URLError as exc:
        raise RuntimeError(f"Graph request failed: {exc}") from exc


def token():
    data = state()
    tok = data.get("secrets", {}).get("MS_GRAPH_ACCESS_TOKEN")
    if tok:
        return tok
    raise RuntimeError("MS_GRAPH_ACCESS_TOKEN is missing; exchange or refresh tokens first")


def token_endpoint(tenant_id):
    return f"https://login.microsoftonline.com/{tenant_id}/oauth2/v2.0/token"


def device_code_endpoint(tenant_id):
    return f"https://login.microsoftonline.com/{tenant_id}/oauth2/v2.0/devicecode"


def device_error(message, details=None, pending=False):
    return {
        "ok": False,
        "pending": pending,
        "error": message,
        "details": details or {},
        "checklist": DEVICE_LOGIN_CHECKLIST,
    }


def start_device_login(data):
    current = state()
    tenant_id = data.get("tenant_id") or current["config"].get("tenant_id") or DEFAULT_TENANT_ALIAS
    client_id = data.get("client_id") or current["config"].get("client_id")
    scopes = ensure_scopes(data.get("scopes") or current["config"].get("oauth_scopes") or os.environ.get("GREENTIC_TEAMS_SCOPES", DEFAULT_SCOPES))
    if not client_id:
        raise RuntimeError("Microsoft OAuth client_id is required.")
    save_state({
        "config": {
            "tenant_id": tenant_id,
            "client_id": client_id,
            "oauth_scopes": scopes,
            "device_login_tenant": tenant_id,
        },
        "secrets": {"MS_GRAPH_CLIENT_ID": client_id},
    })
    form = {"client_id": client_id, "scope": scopes}
    req = Request(device_code_endpoint(tenant_id), data=urlencode(form).encode("utf-8"), headers={"Content-Type": "application/x-www-form-urlencoded"}, method="POST")
    try:
        with urlopen(req, timeout=30) as resp:
            result = json.loads(resp.read().decode("utf-8"))
    except HTTPError as exc:
        raw = exc.read().decode("utf-8", errors="replace")
        try:
            details = json.loads(raw)
        except json.JSONDecodeError:
            details = {"error": raw}
        return device_error("Microsoft device-code request failed.", details)
    save_state({
        "secrets": {"MS_GRAPH_DEVICE_CODE": result.get("device_code", ""), "MS_GRAPH_CLIENT_ID": client_id},
        "config": {"device_login_interval": result.get("interval", 5), "device_login_tenant": tenant_id},
    })
    safe = dict(result)
    if safe.get("device_code"):
        safe["device_code"] = "set"
    safe["login_url"] = safe.get("verification_uri") or safe.get("verification_url") or "https://microsoft.com/devicelogin"
    safe["tenant_endpoint"] = tenant_id
    return safe


def complete_device_login(data):
    current = state()
    cfg = current["config"]
    tenant_id = cfg.get("device_login_tenant") or data.get("tenant_id") or cfg.get("tenant_id") or DEFAULT_TENANT_ALIAS
    client_id = data.get("client_id") or cfg.get("client_id")
    device_code = data.get("device_code") or current["secrets"].get("MS_GRAPH_DEVICE_CODE")
    if not client_id or not device_code:
        raise RuntimeError("Start device login first.")
    form = {
        "client_id": client_id,
        "grant_type": "urn:ietf:params:oauth:grant-type:device_code",
        "device_code": device_code,
    }
    interval = int(cfg.get("device_login_interval") or 5)
    deadline = time.time() + int(data.get("timeout_seconds") or 300)
    while True:
        req = Request(token_endpoint(tenant_id), data=urlencode(form).encode("utf-8"), headers={"Content-Type": "application/x-www-form-urlencoded"}, method="POST")
        try:
            with urlopen(req, timeout=30) as resp:
                result = json.loads(resp.read().decode("utf-8"))
            break
        except HTTPError as exc:
            raw = exc.read().decode("utf-8", errors="replace")
            try:
                details = json.loads(raw)
            except json.JSONDecodeError:
                details = {"error": raw}
            err = details.get("error")
            if err == "authorization_pending":
                if time.time() >= deadline:
                    return device_error("Timed out waiting for Microsoft device login.", details, pending=True)
                time.sleep(interval)
                continue
            if err == "slow_down":
                interval += 5
                if time.time() >= deadline:
                    return device_error("Timed out waiting for Microsoft device login.", details, pending=True)
                time.sleep(interval)
                continue
            return device_error("Microsoft device token request failed.", details)
    discovered_tenant = tenant_from_id_token(result.get("id_token", "")) or tenant_id
    save_state({
        "config": {"tenant_id": discovered_tenant, "client_id": client_id, "auth_tenant_alias": tenant_id},
        "secrets": {
            "MS_GRAPH_CLIENT_ID": client_id,
            "MS_GRAPH_REFRESH_TOKEN": result.get("refresh_token", ""),
            "MS_GRAPH_ACCESS_TOKEN": result.get("access_token", ""),
            "MS_GRAPH_DEVICE_CODE": "",
        },
    })
    safe = dict(result)
    for key in ("refresh_token", "access_token", "id_token"):
        if safe.get(key):
            safe[key] = "set"
    return {"ok": True, "token": safe, "discovery": post_login_discovery()}


def refresh_token(data):
    current = state()
    cfg = current["config"]
    tenant_id = data.get("tenant_id") or cfg.get("auth_tenant_alias") or cfg.get("device_login_tenant") or DEFAULT_TENANT_ALIAS
    client_id = data.get("client_id") or cfg.get("client_id")
    refresh = data.get("refresh_token") or current["secrets"].get("MS_GRAPH_REFRESH_TOKEN")
    if not client_id or not refresh:
        raise RuntimeError("client_id and refresh token are required")
    scopes = ensure_scopes(data.get("scopes") or cfg.get("oauth_scopes") or os.environ.get("GREENTIC_TEAMS_SCOPES", DEFAULT_SCOPES))
    form = {
        "client_id": client_id,
        "grant_type": "refresh_token",
        "refresh_token": refresh,
        "scope": scopes,
    }
    req = Request(token_endpoint(tenant_id), data=urlencode(form).encode("utf-8"), headers={"Content-Type": "application/x-www-form-urlencoded"}, method="POST")
    with urlopen(req, timeout=30) as resp:
        result = json.loads(resp.read().decode("utf-8"))
    save_state({"secrets": {"MS_GRAPH_REFRESH_TOKEN": result.get("refresh_token", refresh), "MS_GRAPH_ACCESS_TOKEN": result.get("access_token", "")}})
    return {"access_token": "set", "refresh_token": "set" if result.get("refresh_token") else "unchanged", "expires_in": result.get("expires_in")}


def post_login_discovery():
    out = {}
    try:
        out["me"] = graph_request("GET", "/me", token())
    except Exception as exc:
        out["me_error"] = str(exc)
    try:
        out["joinedTeams"] = graph_request("GET", "/me/joinedTeams", token())
    except Exception as exc:
        out["joinedTeams_error"] = str(exc)
    teams = out.get("joinedTeams", {}).get("value") or []
    if teams:
        team_id = teams[0].get("id")
        if team_id:
            save_state({"config": {"team_id": team_id}})
            try:
                channels = graph_request("GET", f"/teams/{team_id}/channels", token())
                out["channels"] = channels
                channel_values = channels.get("value") or []
                if channel_values and channel_values[0].get("id"):
                    save_state({"config": {"channel_id": channel_values[0]["id"]}})
            except Exception as exc:
                out["channels_error"] = str(exc)
    append_event("post_login_discovery", sanitized({"discovery": out}))
    return out


def write_provider_values():
    current = state()
    cfg = current["config"]
    secrets_obj = current["secrets"]
    cfg["public_base_url"] = public_url()
    cfg.setdefault("graph_base_url", "https://graph.microsoft.com/v1.0")
    cfg.setdefault("auth_base_url", "https://login.microsoftonline.com")
    cfg.setdefault("token_scope", "https://graph.microsoft.com/.default")
    secrets_obj.setdefault("MS_GRAPH_CLIENT_ID", cfg.get("client_id", ""))
    write_json(VALUES, current)
    return VALUES


def subscription_body(data):
    cfg = state()["config"]
    public = public_url().rstrip("/")
    ingress_url = f"{public}/v1/messaging/ingress/messaging-teams-graph/default/default"
    resource = data.get("resource")
    if not resource:
        if data.get("chat_id") or cfg.get("chat_id"):
            resource = f"/chats/{data.get('chat_id') or cfg.get('chat_id')}/messages"
        else:
            resource = f"/teams/{data.get('team_id') or cfg.get('team_id')}/channels/{data.get('channel_id') or cfg.get('channel_id')}/messages"
    expiration = data.get("expiration") or default_subscription_expiration()
    body = {
        "changeType": data.get("change_type") or "created",
        "notificationUrl": data.get("notification_url") or ingress_url,
        "lifecycleNotificationUrl": data.get("lifecycle_notification_url") or ingress_url,
        "resource": resource,
        "expirationDateTime": expiration,
        "clientState": data.get("client_state") or "greentic-teams-test",
    }
    return body


def validate_subscription_body(body):
    warnings = []
    lifecycle_url = body.get("lifecycleNotificationUrl")
    if not lifecycle_url:
        warnings.append("lifecycleNotificationUrl is required for Teams message subscriptions longer than 1 hour.")
    elif lifecycle_url != body.get("notificationUrl"):
        warnings.append("lifecycleNotificationUrl differs from notificationUrl; verify both endpoints handle validationToken and lifecycle events.")
    try:
        expiration = parse_graph_datetime(body.get("expirationDateTime", ""))
        now = datetime.now(timezone.utc)
        if expiration > now + timedelta(days=3):
            warnings.append("Teams chatMessage subscriptions are limited to 4320 minutes; choose an expiration within 3 days.")
        if expiration > now + timedelta(hours=1) and not lifecycle_url:
            warnings.append("Microsoft Graph will reject this Teams subscription because expiration is more than 1 hour without lifecycleNotificationUrl.")
    except ValueError:
        warnings.append("expirationDateTime is not a valid Graph timestamp.")
    return {"ok": not warnings, "warnings": warnings, "body": body}


def next_card_payload(submitted_text):
    card_id = "greentic-card-next"
    shown_text = submitted_text or "(empty)"
    card = {
        "$schema": "http://adaptivecards.io/schemas/adaptive-card.json",
        "type": "AdaptiveCard",
        "version": "1.4",
        "body": [
            {
                "type": "TextBlock",
                "text": "Greentic Teams Test - Next Card",
                "weight": "Bolder",
                "size": "Large",
                "wrap": True,
            },
            {
                "type": "TextBlock",
                "text": "Submitted text:",
                "weight": "Bolder",
                "wrap": True,
            },
            {
                "type": "TextBlock",
                "text": shown_text,
                "wrap": True,
            },
        ],
    }
    return {
        "subject": None,
        "summary": f"Greentic Teams next card: {shown_text}",
        "body": {
            "contentType": "html",
            "content": f'<attachment id="{card_id}"></attachment>',
        },
        "attachments": [
            {
                "id": card_id,
                "contentType": "application/vnd.microsoft.card.adaptive",
                "content": json.dumps(card),
            }
        ],
    }


def adaptive_card_payload(text):
    card_id = "greentic-card-1"
    card_text = text or "Hello from Greentic Teams tester"
    card = {
        "$schema": "http://adaptivecards.io/schemas/adaptive-card.json",
        "type": "AdaptiveCard",
        "version": "1.4",
        "body": [
            {
                "type": "TextBlock",
                "text": "Greentic Teams Test",
                "weight": "Bolder",
                "size": "Large",
                "wrap": True,
            },
            {
                "type": "TextBlock",
                "text": card_text,
                "wrap": True,
            },
            {
                "type": "FactSet",
                "facts": [
                    {"title": "Transport", "value": "Microsoft Graph"},
                    {"title": "Card", "value": "Adaptive Card 1.4"},
                ],
            },
            {
                "type": "Input.Text",
                "id": "greentic_tester_text",
                "label": "Text for next card",
                "placeholder": "Type something and press Show next",
                "isMultiline": False,
            },
        ],
        "actions": [
            {
                "type": "Action.Submit",
                "title": "Show next",
                "data": {
                    "action_id": "greentic_show_next",
                    "routeToCardId": "greentic_next_card",
                    "msteams": {
                        "type": "messageBack",
                        "displayText": "Show next",
                        "text": "greentic_show_next",
                        "value": {
                            "action_id": "greentic_show_next",
                            "routeToCardId": "greentic_next_card",
                        },
                    },
                },
            },
            {
                "type": "Action.OpenUrl",
                "title": "Open Greentic",
                "url": "https://greentic.ai",
            }
        ],
    }
    return {
        "subject": None,
        "summary": "Greentic Teams adaptive card test",
        "body": {
            "contentType": "html",
            "content": f'<attachment id="{card_id}"></attachment>',
        },
        "attachments": [
            {
                "id": card_id,
                "contentType": "application/vnd.microsoft.card.adaptive",
                "content": json.dumps(card),
            }
        ],
    }


def graph_send_target_path(data):
    if data.get("kind") == "chat":
        return f"/chats/{data.get('chat_id')}/messages"
    return f"/teams/{data.get('team_id')}/channels/{data.get('channel_id')}/messages"


def extract_submit_text_from_message(message):
    body = message.get("body") or {}
    body_text = strip_html(body.get("content") or "")
    if "greentic_show_next" not in body_text and "Show next" not in body_text:
        return None
    for key in ("greentic_tester_text", "tester_text", "text"):
        value = message.get(key)
        if isinstance(value, str) and value.strip():
            return value.strip()
    return body_text.replace("greentic_show_next", "").replace("Show next", "").strip()


def maybe_send_next_card(data, enriched):
    for item in enriched:
        message = item.get("raw_message") or {}
        submitted_text = extract_submit_text_from_message(message)
        if submitted_text is None:
            continue
        path = graph_send_target_path(data)
        result = graph_request("POST", path, token(), next_card_payload(submitted_text))
        event = {"submitted_text": submitted_text, "send_path": path, "result": result}
        append_event("teams_show_next_sent", event)
        return event
    return None


def strip_html(value):
    text = re.sub(r"<br\s*/?>", "\n", value or "", flags=re.IGNORECASE)
    text = re.sub(r"<[^>]+>", "", text)
    return html_lib.unescape(text).strip()


def graph_resource_path(notification):
    resource = (
        notification.get("resourceData", {}).get("@odata.id")
        or notification.get("resource")
        or ""
    )
    if not resource:
        return ""
    resource = unquote(resource).strip()
    if resource.startswith("http"):
        return resource
    resource = resource.lstrip("/")

    patterns = [
        (
            r"teams\('([^']+)'\)/channels\('([^']+)'\)/messages\('([^']+)'\)/replies\('([^']+)'\)",
            "/teams/{}/channels/{}/messages/{}/replies/{}",
        ),
        (
            r"teams\('([^']+)'\)/channels\('([^']+)'\)/messages\('([^']+)'\)",
            "/teams/{}/channels/{}/messages/{}",
        ),
        (
            r"chats\('([^']+)'\)/messages\('([^']+)'\)",
            "/chats/{}/messages/{}",
        ),
    ]
    for pattern, template in patterns:
        match = re.fullmatch(pattern, resource)
        if match:
            return template.format(*match.groups())

    message_id = notification.get("resourceData", {}).get("id")
    if message_id and resource.endswith("/messages"):
        return "/" + resource + "/" + message_id
    if message_id and resource.endswith("/replies"):
        return "/" + resource + "/" + message_id
    return "/" + resource


def summarize_chat_message(notification, message):
    body = message.get("body") or {}
    from_user = (((message.get("from") or {}).get("user") or {}).get("displayName"))
    from_app = (((message.get("from") or {}).get("application") or {}).get("displayName"))
    return {
        "subscription_id": notification.get("subscriptionId"),
        "change_type": notification.get("changeType"),
        "resource": notification.get("resource"),
        "message_id": message.get("id") or notification.get("resourceData", {}).get("id"),
        "from": from_user or from_app,
        "createdDateTime": message.get("createdDateTime"),
        "webUrl": message.get("webUrl"),
        "contentType": body.get("contentType"),
        "message_text": strip_html(body.get("content") or ""),
        "message_html": body.get("content"),
    }


def enrich_notification_payload(payload):
    enriched = []
    for notification in payload.get("value") or []:
        path = graph_resource_path(notification)
        item = {"notification": notification, "graph_path": path}
        if path:
            try:
                message = graph_request("GET", path, token())
                item["raw_message"] = message
                item["message"] = summarize_chat_message(notification, message)
            except Exception as exc:
                item["fetch_error"] = str(exc)
        enriched.append(item)
    return enriched


HTML = """<!doctype html>
<html><head><meta charset="utf-8"><title>Greentic Teams Graph Tester</title>
<style>
body{font-family:system-ui,sans-serif;max-width:1100px;margin:24px auto;padding:0 16px;background:#f7f7f8;color:#202124}
section{background:white;border:1px solid #ddd;border-radius:8px;padding:16px;margin:14px 0}
label{display:block;margin:8px 0} input,textarea,select{width:100%;box-sizing:border-box;padding:8px}
button{margin:4px 6px 4px 0;padding:8px 12px} pre{white-space:pre-wrap;background:#111;color:#eee;padding:12px;border-radius:6px;max-height:360px;overflow:auto}
.row{display:grid;grid-template-columns:1fr 1fr;gap:12px}.muted{color:#666}
</style></head><body>
<h1>Greentic Teams Graph Tester</h1>
<section><h2>Connection</h2><p>Public URL: <code id="public"></code></p><p>Ingress: <code id="ingress"></code></p></section>
<section><h2>Connect Microsoft Teams</h2><button onclick="deviceStart()">Start device login</button><button onclick="deviceComplete()">Poll and finish login</button><button onclick="refreshToken()">Refresh token</button><details><summary>Advanced OAuth settings</summary><div class="row"><label>Tenant authority<input id="tenant_id"></label><label>Client ID<input id="client_id"></label></div><label>Scopes<input id="scopes"></label></details><pre id="oauth"></pre></section>
<section><h2>Discovery</h2><button onclick="api('/api/discover/setup',collect(),'discover')">Discover setup data</button><button onclick="api('/api/discover/me',{})">Get me</button><button onclick="api('/api/discover/teams',{})">List joined teams</button><label>Team ID<input id="team_id"></label><button onclick="api('/api/discover/channels',{team_id:val('team_id')})">List channels</button><label>Channel ID<input id="channel_id"></label><label>Chat ID<input id="chat_id"></label><pre id="discover"></pre></section>
<section><h2>Send</h2><p class="muted">The adaptive card includes a text input and a Show next submit button. If Teams exposes the submit through Graph notifications, this tester sends a second card with the submitted text.</p><label>Kind<select id="kind"><option>channel</option><option>chat</option></select></label><label>Card text<textarea id="text">Hello from Greentic Teams tester</textarea></label><label>Manual next-card text<input id="next_text" placeholder="Used only by the manual fallback button"></label><button onclick="directGraph()">Send adaptive card</button><button onclick="sendNextManual()">Send next card manually</button><pre id="send"></pre></section>
<section><h2>Subscriptions</h2><label>Client state<input id="client_state" value="greentic-teams-test"></label><label>Expiration<input id="expiration"></label><label>Lifecycle notification URL<input id="lifecycle_notification_url"></label><button onclick="subPreview()">Preview subscription body</button><button onclick="subCreate()">Create subscription</button><button onclick="simulateValidation()">Simulate validationToken</button><pre id="subs"></pre></section>
<section><h2>Incoming Events</h2><pre id="events"></pre></section>
<script>
const ids=["tenant_id","client_id","team_id","channel_id","chat_id","scopes","kind","text","next_text","client_state","expiration","lifecycle_notification_url"];
function val(id){const el=document.getElementById(id);return el?el.value:""}
function set(id,v){document.getElementById(id).textContent=typeof v==="string"?v:JSON.stringify(v,null,2)}
function targetFor(path,target){return target||(path.includes("discover")?"discover":path.includes("subscription")?"subs":path.includes("send")?"send":"oauth")}
async function api(path,body,target){const r=await fetch(path,{method:"POST",headers:{"Content-Type":"application/json"},body:JSON.stringify(body)});const j=await r.json();set(targetFor(path,target),j);return j}
function mergeScopes(v,required){const seen=new Set();const out=[];((v||'')+' '+(required||'')).split(/\\s+/).filter(Boolean).forEach(s=>{const k=s.toLowerCase();if(!seen.has(k)){seen.add(k);out.push(s)}});return out.join(' ')}
async function load(){const r=await fetch('/api/state');const s=await r.json();const ingress=(s.public_url||'')+'/v1/messaging/ingress/messaging-teams-graph/default/default';document.getElementById('public').textContent=s.public_url||'';document.getElementById('ingress').textContent=ingress;const c=s.values.config||{};const sec=s.values.secrets||{};ids.forEach(id=>{if(document.getElementById(id)&&c[id])document.getElementById(id).value=c[id]});if(document.getElementById('tenant_id'))document.getElementById('tenant_id').value=c.device_login_tenant||c.auth_tenant_alias||'organizations';const scopeEl=document.getElementById('scopes');scopeEl.value=mergeScopes(scopeEl.value||c.oauth_scopes||localStorage.scopes||s.default_scopes,s.default_scopes);localStorage.scopes=scopeEl.value;if(!document.getElementById('expiration').value)document.getElementById('expiration').value=s.default_subscription_expiration;if(!document.getElementById('lifecycle_notification_url').value)document.getElementById('lifecycle_notification_url').value=ingress;set('events',s.events)}
function collect(){return {tenant_id:val('tenant_id'),client_id:val('client_id'),scopes:val('scopes'),team_id:val('team_id'),channel_id:val('channel_id'),chat_id:val('chat_id'),kind:val('kind'),text:val('text'),next_text:val('next_text'),client_state:val('client_state'),expiration:val('expiration'),lifecycle_notification_url:val('lifecycle_notification_url')}}
async function deviceStart(){localStorage.scopes=val('scopes');const j=await api('/api/device/start',collect(),'oauth');const url=j.verification_uri||j.verification_url||j.login_url||'https://microsoft.com/devicelogin';if(url) window.open(url,'_blank','noopener')}
async function deviceComplete(){await api('/api/device/complete',collect(),'oauth')}
async function refreshToken(){await api('/api/token/refresh',collect(),'oauth')}
async function directGraph(){await api('/api/send/direct',collect(),'send')}
async function sendNextManual(){await api('/api/send/next',collect(),'send')}
async function subPreview(){await api('/api/subscriptions/preview',collect(),'subs')}
async function subCreate(){await api('/api/subscriptions/create',collect(),'subs')}
async function simulateValidation(){const r=await fetch('/v1/messaging/ingress/messaging-teams-graph/default/default?validationToken=hello-graph',{method:'POST'});set('subs',await r.text())}
setInterval(load,2000);load();
</script></body></html>"""


class Handler(BaseHTTPRequestHandler):
    server_version = "GreenticTeamsTester/1.0"

    def send_json(self, data, status=200):
        raw = json.dumps(data, indent=2).encode("utf-8")
        self.send_response(status)
        self.send_header("Content-Type", "application/json")
        self.send_header("Content-Length", str(len(raw)))
        self.end_headers()
        self.wfile.write(raw)

    def read_json(self):
        length = int(self.headers.get("Content-Length", "0") or "0")
        if not length:
            return {}
        return json.loads(self.rfile.read(length).decode("utf-8"))

    def send_validation_token(self, parsed):
        token_value = (parse_qs(parsed.query).get("validationToken") or [""])[0]
        if not token_value:
            return False
        raw = token_value.encode("utf-8")
        self.send_response(200)
        self.send_header("Content-Type", "text/plain; charset=utf-8")
        self.send_header("Content-Length", str(len(raw)))
        self.end_headers()
        self.wfile.write(raw)
        return True

    def do_GET(self):
        parsed = urlparse(self.path)
        if parsed.path == "/":
            raw = HTML.encode("utf-8")
            self.send_response(200)
            self.send_header("Content-Type", "text/html; charset=utf-8")
            self.send_header("Content-Length", str(len(raw)))
            self.end_headers()
            self.wfile.write(raw)
            return
        if parsed.path == "/api/state":
            self.send_json({"public_url": public_url(), "values": sanitized(state()), "events": recent_events(), "default_scopes": os.environ.get("GREENTIC_TEAMS_SCOPES", DEFAULT_SCOPES), "default_subscription_expiration": default_subscription_expiration()})
            return
        if parsed.path == "/v1/messaging/ingress/messaging-teams-graph/default/default":
            if self.send_validation_token(parsed):
                return
        self.send_json({"error": "not found"}, 404)

    def do_POST(self):
        parsed = urlparse(self.path)
        try:
            if parsed.path == "/v1/messaging/ingress/messaging-teams-graph/default/default" and self.send_validation_token(parsed):
                return
            data = self.read_json()
            if parsed.path == "/api/save":
                self.send_json(save_state(data)); return
            if parsed.path == "/api/device/start":
                self.send_json(start_device_login(data)); return
            if parsed.path == "/api/device/complete":
                self.send_json(complete_device_login(data)); return
            if parsed.path == "/api/token/refresh":
                self.send_json(refresh_token(data)); return
            if parsed.path == "/api/discover/me":
                self.send_json(graph_request("GET", "/me", token())); return
            if parsed.path == "/api/discover/teams":
                self.send_json(graph_request("GET", "/me/joinedTeams", token())); return
            if parsed.path == "/api/discover/setup":
                self.send_json(post_login_discovery()); return
            if parsed.path == "/api/discover/channels":
                self.send_json(graph_request("GET", f"/teams/{data.get('team_id')}/channels", token())); return
            if parsed.path == "/api/send/direct":
                path = graph_send_target_path(data)
                self.send_json(graph_request("POST", path, token(), adaptive_card_payload(data.get("text")))); return
            if parsed.path == "/api/send/next":
                path = graph_send_target_path(data)
                self.send_json(graph_request("POST", path, token(), next_card_payload(data.get("next_text") or data.get("text")))); return
            if parsed.path == "/api/subscriptions/preview":
                body = subscription_body(data)
                self.send_json(validate_subscription_body(body)); return
            if parsed.path == "/api/subscriptions/create":
                try:
                    body = subscription_body(data)
                    validation = validate_subscription_body(body)
                    if validation["warnings"]:
                        append_event("subscription_body_warning", validation)
                    self.send_json(graph_request("POST", "/subscriptions", token(), body))
                except Exception as exc:
                    self.send_json({"ok": False, **graph_error_hint(exc)}, 500)
                return
            if parsed.path == "/api/subscriptions/renew":
                self.send_json(graph_request("PATCH", f"/subscriptions/{data.get('subscription_id')}", token(), {"expirationDateTime": data.get("expiration")})); return
            if parsed.path == "/api/subscriptions/delete":
                self.send_json(graph_request("DELETE", f"/subscriptions/{data.get('subscription_id')}", token())); return
            if parsed.path == "/v1/messaging/ingress/messaging-teams-graph/default/default":
                append_event("graph_notification", data)
                enriched = enrich_notification_payload(data)
                if enriched:
                    append_event("graph_notification_enriched", enriched)
                next_card = maybe_send_next_card(state()["config"], enriched)
                self.send_json({"ok": True, "enriched": enriched, "next_card": next_card}); return
            self.send_json({"error": "not found"}, 404)
        except Exception as exc:
            self.send_json({"ok": False, "error": str(exc)}, 500)


if __name__ == "__main__":
    write_provider_values()
    ThreadingHTTPServer(("127.0.0.1", PORT), Handler).serve_forever()
PY

export ROOT_DIR WORK_DIR PORT LOCAL_URL
python3 "${WORK_DIR}/server.py" &
SERVER_PID=$!
trap 'kill "${SERVER_PID}" 2>/dev/null || true; [[ -n "${CLOUDFLARED_PID:-}" ]] && kill "${CLOUDFLARED_PID}" 2>/dev/null || true' EXIT

if command -v "${CLOUDFLARED_BIN}" >/dev/null 2>&1; then
  "${CLOUDFLARED_BIN}" tunnel --url "${LOCAL_URL}" --no-autoupdate >"${WORK_DIR}/cloudflared.log" 2>&1 &
  CLOUDFLARED_PID=$!
  for _ in $(seq 1 40); do
    PUBLIC_URL="$(grep -o 'https://[-a-zA-Z0-9.]*\.trycloudflare\.com' "${WORK_DIR}/cloudflared.log" | head -n1 || true)"
    if [[ -n "${PUBLIC_URL}" ]]; then
      echo "${PUBLIC_URL}" > "${WORK_DIR}/public-url.txt"
      break
    fi
    sleep 0.5
  done
else
  echo "cloudflared not found; continuing with local URL only" >&2
fi

if [[ ! -s "${WORK_DIR}/public-url.txt" ]]; then
  echo "${LOCAL_URL}" > "${WORK_DIR}/public-url.txt"
fi

echo "Teams tester UI: ${LOCAL_URL}"
echo "Teams public URL: $(cat "${WORK_DIR}/public-url.txt")"
echo "Teams ingress URL: $(cat "${WORK_DIR}/public-url.txt")/v1/messaging/ingress/messaging-teams-graph/default/default"

if [[ "${NO_OPEN}" -eq 0 ]]; then
  if command -v xdg-open >/dev/null 2>&1; then
    xdg-open "${LOCAL_URL}" >/dev/null 2>&1 || true
  elif command -v open >/dev/null 2>&1; then
    open "${LOCAL_URL}" >/dev/null 2>&1 || true
  fi
fi

wait "${SERVER_PID}"
