#!/usr/bin/env bash
# Start a local Webex tester UI with a cloudflared public URL.
#
# The page asks only for a Webex bot token, registers the Webex webhook with an
# internally generated secret, captures the first inbound room/person, and lets
# you test sending back to that captured conversation.
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
import secrets
import string
import subprocess
import threading
import time
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
from pathlib import Path
from urllib.parse import quote, urlparse
from urllib.error import HTTPError, URLError
from urllib.request import Request, urlopen


ROOT = Path(os.environ["GREENTIC_ROOT"]).resolve()
WORK = Path(os.environ["GREENTIC_WEBEX_WORK"]).resolve()
TESTER = Path(os.environ["GREENTIC_TESTER_BIN"]).resolve()
VALUES = WORK / "webex-values.json"
EVENTS = WORK / "events.jsonl"
PUBLIC_URL_FILE = WORK / "public-url.txt"
CONVERSATION = WORK / "conversation.json"
CARD_DIR = WORK / "cards"

EVENT_LOCK = threading.Lock()


def public_url() -> str:
    try:
        return PUBLIC_URL_FILE.read_text(encoding="utf-8").strip()
    except FileNotFoundError:
        return ""


def webhook_path(tenant: str = "default", channel: str = "default") -> str:
    return f"/v1/messaging/ingress/messaging-webex/{quote(tenant or 'default', safe='')}/{quote(channel or 'default', safe='')}"


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


def generated_secret() -> str:
    alphabet = string.ascii_letters + string.digits
    return "".join(secrets.choice(alphabet) for _ in range(20))


def existing_webhook_secret() -> str:
    if VALUES.exists():
        try:
            values = json.loads(VALUES.read_text(encoding="utf-8"))
            secret = values.get("secrets", {}).get("WEBEX_WEBHOOK_SECRET")
            if isinstance(secret, str) and len(secret) == 20:
                return secret
        except json.JSONDecodeError:
            pass
    return generated_secret()


def read_conversation() -> dict:
    if not CONVERSATION.exists():
        return {}
    try:
        value = json.loads(CONVERSATION.read_text(encoding="utf-8"))
        return value if isinstance(value, dict) else {}
    except json.JSONDecodeError:
        return {}


def write_conversation(value: dict) -> None:
    CONVERSATION.write_text(json.dumps(value, indent=2) + "\n", encoding="utf-8")


def is_webex_bot_email(value: object) -> bool:
    return isinstance(value, str) and value.lower().endswith("@webex.bot")


def update_conversation_from_result(result: dict) -> dict:
    parsed = result.get("json")
    if not isinstance(parsed, dict):
        return read_conversation()
    envelopes = parsed.get("ingress_envelopes")
    if not isinstance(envelopes, list):
        envelopes = parsed.get("envelopes")
    if not isinstance(envelopes, list):
        return read_conversation()
    current = read_conversation()
    for envelope in envelopes:
        if not isinstance(envelope, dict):
            continue
        metadata = envelope.get("metadata") if isinstance(envelope.get("metadata"), dict) else {}
        room_id = metadata.get("webex.roomId") or envelope.get("session_id")
        person_email = metadata.get("webex.personEmail")
        person_id = metadata.get("webex.personId")
        message_id = metadata.get("webex.messageId")
        is_bot_sender = is_webex_bot_email(person_email)
        if room_id:
            current["room_id"] = room_id
        if person_email and not is_bot_sender:
            current["person_email"] = person_email
        if person_id and not is_bot_sender:
            current["person_id"] = person_id
        if message_id:
            current["message_id"] = message_id
        if is_bot_sender:
            current["last_bot_email"] = person_email
        current["updated_at"] = time.strftime("%Y-%m-%dT%H:%M:%SZ", time.gmtime())
    if current:
        write_conversation(current)
    return current


def update_conversation_from_webhook_body(body_text: str) -> dict:
    try:
        parsed = json.loads(body_text)
    except json.JSONDecodeError:
        return read_conversation()
    if not isinstance(parsed, dict):
        return read_conversation()
    data = parsed.get("data")
    if not isinstance(data, dict):
        data = {}
    current = read_conversation()
    room_id = data.get("roomId")
    person_email = data.get("personEmail")
    person_id = data.get("personId")
    message_id = data.get("id")
    is_bot_sender = is_webex_bot_email(person_email)
    if room_id:
        current["room_id"] = room_id
    if person_email and not is_bot_sender:
        current["person_email"] = person_email
    if person_id and not is_bot_sender:
        current["person_id"] = person_id
    if message_id:
        current["message_id"] = message_id
    if is_bot_sender:
        current["last_bot_email"] = person_email
    if room_id or person_email or person_id or message_id:
        current["updated_at"] = time.strftime("%Y-%m-%dT%H:%M:%SZ", time.gmtime())
        write_conversation(current)
    return current


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
        "webex_content_image_url": data.get("webex_content_image_url") or "",
    }
    return {
        "config": config,
        "secrets": {
            "WEBEX_BOT_TOKEN": data.get("bot_token") or "",
            "WEBEX_WEBHOOK_SECRET": existing_webhook_secret(),
        },
        "http": "real",
        "state": {},
    }


def clean_values(values: dict) -> dict:
    values = json.loads(json.dumps(values))
    values["config"] = {k: v for k, v in values["config"].items() if v not in ("", None)}
    values["secrets"] = {k: v for k, v in values["secrets"].items() if v not in ("", None)}
    return values


def webex_bot_token() -> str:
    if not VALUES.exists():
        return ""
    try:
        values = json.loads(VALUES.read_text(encoding="utf-8"))
    except json.JSONDecodeError:
        return ""
    return values.get("secrets", {}).get("WEBEX_BOT_TOKEN") or ""


def webex_api_base() -> str:
    if not VALUES.exists():
        return "https://webexapis.com/v1"
    try:
        values = json.loads(VALUES.read_text(encoding="utf-8"))
    except json.JSONDecodeError:
        return "https://webexapis.com/v1"
    return values.get("config", {}).get("api_base_url") or "https://webexapis.com/v1"


def webex_content_image_url() -> str:
    if VALUES.exists():
        try:
            values = json.loads(VALUES.read_text(encoding="utf-8"))
            configured = values.get("config", {}).get("webex_content_image_url") or ""
            if configured:
                return configured
        except json.JSONDecodeError:
            pass
    return os.environ.get("WEBEX_CONTENT_IMAGE_URL") or "https://webexapis.com/v1/contents/example"


def webex_get_message(message_id: str) -> dict:
    token = webex_bot_token()
    if not token:
        raise RuntimeError("WEBEX_BOT_TOKEN is required before fetching Webex messages")
    url = f"{webex_api_base().rstrip('/')}/messages/{quote(message_id, safe='')}"
    request = Request(url, headers={"Authorization": f"Bearer {token}", "Accept": "application/json"})
    try:
        with urlopen(request, timeout=30) as response:
            raw = response.read().decode("utf-8")
            return json.loads(raw) if raw else {}
    except HTTPError as exc:
        raw = exc.read().decode("utf-8", errors="replace")
        raise RuntimeError(f"Webex GET message failed status={exc.code} body={raw}") from exc
    except URLError as exc:
        raise RuntimeError(f"Webex GET message failed: {exc}") from exc


def adaptive_card(case_id: str, version: str = "1.3") -> dict:
    base = {"type": "AdaptiveCard", "version": version, "$schema": "http://adaptivecards.io/schemas/adaptive-card.json"}
    if case_id == "minimal_text":
        return {**base, "body": [{"type": "TextBlock", "text": "Minimal Webex card", "wrap": True}]}
    if case_id == "submit_action":
        return {
            **base,
            "body": [{"type": "TextBlock", "text": "Submit action card", "wrap": True}],
            "actions": [{"type": "Action.Submit", "title": "Submit", "data": {"case": "submit_action", "ok": True}}],
        }
    if case_id == "open_url":
        return {
            **base,
            "body": [{"type": "TextBlock", "text": "OpenUrl action card", "wrap": True}],
            "actions": [{"type": "Action.OpenUrl", "title": "Open Greentic", "url": "https://greentic.ai"}],
        }
    if case_id == "column_set":
        return {
            **base,
            "body": [{
                "type": "ColumnSet",
                "columns": [
                    {"type": "Column", "width": "stretch", "items": [{"type": "TextBlock", "text": "Column title", "weight": "Bolder", "wrap": True}]},
                    {"type": "Column", "width": "stretch", "items": [{"type": "TextBlock", "text": "Column subtitle", "isSubtle": True, "wrap": True}]},
                ],
            }],
        }
    if case_id == "container_style":
        return {
            **base,
            "body": [{
                "type": "Container",
                "style": "accent",
                "isVisible": True,
                "items": [{"type": "TextBlock", "text": "Accent container with boolean isVisible", "wrap": True}],
            }],
        }
    if case_id == "image":
        return {
            **base,
            "body": [
                {"type": "Image", "url": "https://www.gstatic.com/webp/gallery/1.jpg", "altText": "Sample landscape", "size": "Medium"},
                {"type": "TextBlock", "text": "Image card text", "wrap": True},
            ],
        }
    if case_id == "hr_like":
        return {
            **base,
            "body": [
                {"type": "TextBlock", "text": "HR onboarding", "weight": "Bolder", "size": "Large", "wrap": True},
                {"type": "TextBlock", "text": "Welcome flow progress", "isSubtle": True, "wrap": True},
                {
                    "type": "Container",
                    "style": "accent",
                    "isVisible": True,
                    "items": [
                        {"type": "TextBlock", "text": "Step 2 of 4 complete", "wrap": True},
                        {"type": "TextBlock", "text": "Next: confirm equipment and start date.", "wrap": True},
                    ],
                },
            ],
            "actions": [
                {"type": "Action.Submit", "title": "Confirm", "data": {"action": "confirm", "confirmed": True}},
                {"type": "Action.Submit", "title": "Need help", "data": {"action": "help", "urgent": False}},
                {"type": "Action.Submit", "title": "Remind me", "data": {"action": "remind", "tomorrow": True}},
            ],
        }
    if case_id == "version_12":
        return adaptive_card("minimal_text", version="1.2")
    if case_id == "string_is_visible":
        return {
            **base,
            "body": [{
                "type": "Container",
                "style": "accent",
                "isVisible": "true",
                "items": [{"type": "TextBlock", "text": "String isVisible compatibility test", "wrap": True}],
            }],
        }
    if case_id == "top_level_rtl_false":
        return {
            **base,
            "lang": "en",
            "rtl": False,
            "body": [{"type": "TextBlock", "text": "Top-level rtl=false compatibility test", "wrap": True}],
        }
    if case_id == "submit_positive_style":
        return {
            **base,
            "body": [{"type": "TextBlock", "text": "Action.Submit positive style compatibility test", "wrap": True}],
            "actions": [{"type": "Action.Submit", "title": "Positive", "style": "positive", "data": {"case": "submit_positive_style"}}],
        }
    if case_id == "submit_destructive_style":
        return {
            **base,
            "body": [{"type": "TextBlock", "text": "Action.Submit destructive style compatibility test", "wrap": True}],
            "actions": [{"type": "Action.Submit", "title": "Destructive", "style": "destructive", "data": {"case": "submit_destructive_style"}}],
        }
    if case_id == "five_submit_actions":
        return {
            **base,
            "body": [{"type": "TextBlock", "text": "Five Action.Submit buttons compatibility test", "wrap": True}],
            "actions": [
                {"type": "Action.Submit", "title": "One", "data": {"choice": "one"}},
                {"type": "Action.Submit", "title": "Two", "data": {"choice": "two"}},
                {"type": "Action.Submit", "title": "Three", "data": {"choice": "three"}},
                {"type": "Action.Submit", "title": "Four", "data": {"choice": "four"}},
                {"type": "Action.Submit", "title": "Five", "data": {"choice": "five"}},
            ],
        }
    if case_id == "webex_content_image":
        return {
            **base,
            "body": [
                {"type": "Image", "url": webex_content_image_url(), "altText": "Webex content image", "size": "Medium"},
                {"type": "TextBlock", "text": "Webex content image URL compatibility test", "wrap": True},
            ],
        }
    if case_id == "hr_like_string_boolean":
        card = adaptive_card("hr_like")
        card["body"][2]["isVisible"] = "true"
        return card
    if case_id == "hr_like_with_styles_rtl":
        card = adaptive_card("hr_like")
        card["lang"] = "en"
        card["rtl"] = False
        card["actions"][0]["style"] = "positive"
        card["actions"][1]["style"] = "default"
        card["actions"][2]["style"] = "destructive"
        return card
    if case_id == "old_hr_onboarding":
        return {
            **base,
            "lang": "en",
            "rtl": False,
            "body": [
                {
                    "type": "Container",
                    "items": [{
                        "type": "ColumnSet",
                        "columns": [
                            {
                                "type": "Column",
                                "width": "auto",
                                "items": [{
                                    "type": "Image",
                                    "url": webex_content_image_url(),
                                    "size": "Medium",
                                }],
                            },
                            {
                                "type": "Column",
                                "width": "stretch",
                                "items": [
                                    {"type": "TextBlock", "text": "Acme Corp - HR Onboarding", "weight": "Bolder", "size": "Large"},
                                    {"type": "TextBlock", "text": "Welcome to the employee onboarding assistant", "isSubtle": True, "wrap": True},
                                ],
                            },
                        ],
                    }],
                },
                {
                    "type": "Container",
                    "style": "accent",
                    "isVisible": "true",
                    "items": [
                        {"type": "TextBlock", "text": "Current Onboarding: Jane Smith", "weight": "Bolder"},
                        {
                            "type": "ColumnSet",
                            "columns": [{
                                "type": "Column",
                                "width": "stretch",
                                "items": [{"type": "TextBlock", "text": "Progress: 40%", "isSubtle": True}],
                            }],
                        },
                    ],
                },
                {"type": "TextBlock", "text": "What would you like to do?", "spacing": "Medium", "wrap": True},
            ],
            "actions": [
                {"type": "Action.Submit", "title": "Start Onboarding", "style": "positive", "data": {"action_id": "start_onboarding", "routeToCardId": "employee_form_card"}},
                {"type": "Action.Submit", "title": "Check Progress", "data": {"action_id": "check_progress", "routeToCardId": "onboarding_checklist_card"}},
                {"type": "Action.Submit", "title": "Upload Documents", "data": {"action_id": "upload_documents", "routeToCardId": "document_upload_card"}},
                {"type": "Action.Submit", "title": "Request Access", "data": {"action_id": "request_access", "routeToCardId": "access_request_card"}},
                {"type": "Action.Submit", "title": "Reset Onboarding", "style": "destructive", "data": {"action_id": "reset_onboarding"}},
            ],
        }
    if case_id == "new_hr_onboarding":
        card = adaptive_card("old_hr_onboarding")
        card.pop("rtl", None)
        card["body"][0]["items"][0]["columns"][0]["items"][0]["url"] = "https://www.gstatic.com/webp/gallery/1.jpg"
        card["body"][1]["isVisible"] = True
        return card
    raise ValueError(f"unknown card case: {case_id}")


CARD_CASES = [
    {"id": "minimal_text", "name": "Minimal text-only card", "expected_actions": 0},
    {"id": "submit_action", "name": "Text card with Action.Submit", "expected_actions": 1},
    {"id": "open_url", "name": "Text card with Action.OpenUrl", "expected_actions": 1},
    {"id": "column_set", "name": "ColumnSet card", "expected_actions": 0},
    {"id": "container_style", "name": "Container style card", "expected_actions": 0, "expects_boolean_fields": True},
    {"id": "image", "name": "Image card", "expected_actions": 0},
    {"id": "hr_like", "name": "HR-like card", "expected_actions": 3, "expects_boolean_fields": True},
    {"id": "version_12", "name": "Version fallback card", "expected_actions": 0},
    {"id": "string_is_visible", "name": "String isVisible card", "expected_actions": 0, "known_schema_risk": "isVisible is a string instead of a boolean"},
    {"id": "top_level_rtl_false", "name": "Top-level rtl=false card", "expected_actions": 0},
    {"id": "submit_positive_style", "name": "Action.Submit positive style card", "expected_actions": 1},
    {"id": "submit_destructive_style", "name": "Action.Submit destructive style card", "expected_actions": 1},
    {"id": "five_submit_actions", "name": "Five Action.Submit buttons card", "expected_actions": 5},
    {"id": "webex_content_image", "name": "Webex content image URL card", "expected_actions": 0},
    {"id": "hr_like_string_boolean", "name": "HR-like card with string boolean", "expected_actions": 3, "known_schema_risk": "isVisible is a string instead of a boolean"},
    {"id": "hr_like_with_styles_rtl", "name": "HR-like card with action styles and rtl", "expected_actions": 3},
    {"id": "old_hr_onboarding", "name": "Old HR onboarding card shape", "expected_actions": 5, "known_schema_risk": "matches old shape with string isVisible and Webex content image URL"},
    {"id": "new_hr_onboarding", "name": "Proposed new HR onboarding card shape", "expected_actions": 5},
]


def collect_types(value: object, out: list[str]) -> None:
    if isinstance(value, dict):
        if value.get("type"):
            out.append(str(value["type"]))
        for child in value.values():
            collect_types(child, out)
    elif isinstance(value, list):
        for child in value:
            collect_types(child, out)


def collect_image_urls(value: object, out: list[str]) -> None:
    if isinstance(value, dict):
        if value.get("type") == "Image" and isinstance(value.get("url"), str):
            out.append(value["url"])
        for child in value.values():
            collect_image_urls(child, out)
    elif isinstance(value, list):
        for child in value:
            collect_image_urls(child, out)


def boolean_field_summary(value: object) -> dict:
    fields = []
    bad = []

    def visit(node: object, path: str) -> None:
        if isinstance(node, dict):
            for name, child in node.items():
                child_path = f"{path}.{name}" if path else name
                if name in {"isVisible", "wrap", "isSubtle", "bleed", "separator"}:
                    fields.append({"path": child_path, "type": type(child).__name__, "value": child})
                    if not isinstance(child, bool):
                        bad.append(child_path)
                visit(child, child_path)
        elif isinstance(node, list):
            for idx, child in enumerate(node):
                visit(child, f"{path}[{idx}]")

    visit(value, "")
    return {"fields": fields, "all_boolean": not bad, "bad_paths": bad}


def card_content_from_message(message: dict) -> tuple[dict | None, str | None]:
    attachments = message.get("attachments")
    if not isinstance(attachments, list) or not attachments:
        return None, None
    first = attachments[0]
    if not isinstance(first, dict):
        return None, None
    content_type = first.get("contentType")
    content = first.get("content")
    if isinstance(content, str):
        try:
            content = json.loads(content)
        except json.JSONDecodeError:
            content = None
    return content if isinstance(content, dict) else None, content_type


def summarize_fetched_message(message: dict, expected_actions: int) -> dict:
    card, content_type = card_content_from_message(message)
    body_types: list[str] = []
    action_types: list[str] = []
    action_styles: list[str | None] = []
    image_urls: list[str] = []
    if isinstance(card, dict):
        collect_image_urls(card, image_urls)
        body = card.get("body")
        if isinstance(body, list):
            for item in body:
                collect_types(item, body_types)
        actions = card.get("actions")
        if isinstance(actions, list):
            for action in actions:
                if isinstance(action, dict) and action.get("type"):
                    action_types.append(str(action["type"]))
                    action_styles.append(action.get("style"))
    booleans = boolean_field_summary(card or {})
    attachment_count = len(message.get("attachments") or [])
    pass_checks = {
        "attachment_exists": attachment_count > 0,
        "content_type_adaptive_card": content_type == "application/vnd.microsoft.card.adaptive",
        "card_type_present": isinstance(card, dict) and card.get("type") == "AdaptiveCard",
        "card_version_present": isinstance(card, dict) and bool(card.get("version")),
        "actions_preserved": len(action_types) == expected_actions,
        "booleans_are_booleans": booleans["all_boolean"],
    }
    return {
        "text": message.get("text"),
        "markdown": message.get("markdown"),
        "attachmentCount": attachment_count,
        "contentType": content_type,
        "cardVersion": card.get("version") if isinstance(card, dict) else None,
        "cardKeys": sorted(card.keys()) if isinstance(card, dict) else [],
        "rtl": card.get("rtl") if isinstance(card, dict) else None,
        "bodyTypes": body_types,
        "actionTypes": action_types,
        "actionStyles": action_styles,
        "imageUrls": image_urls,
        "booleanFields": booleans["fields"],
        "pass": pass_checks,
    }


def card_test_target(data: dict) -> tuple[str, str]:
    conversation = read_conversation()
    target = (
        data.get("card_room_id")
        or os.environ.get("WEBEX_ROOM_ID")
        or os.environ.get("WEBEX_TEST_ROOM_ID")
        or data.get("send_to")
        or conversation.get("room_id")
        or ""
    )
    if not target:
        raise RuntimeError("WEBEX_ROOM_ID, WEBEX_TEST_ROOM_ID, card room ID, or captured last room is required")
    return target, "room"


def extract_message_id(send_result: dict) -> str:
    parsed = send_result.get("json")
    if not isinstance(parsed, dict):
        return ""
    result = parsed.get("result")
    if isinstance(result, dict) and isinstance(result.get("message"), str):
        return result["message"]
    for call in parsed.get("http_calls") or []:
        body_b64 = call.get("response", {}).get("body_b64") if isinstance(call, dict) else None
        if not body_b64:
            continue
        try:
            import base64
            body = json.loads(base64.b64decode(body_b64).decode("utf-8"))
        except Exception:
            continue
        if isinstance(body, dict) and isinstance(body.get("id"), str):
            return body["id"]
    return ""


def run_card_compat_tests(data: dict) -> dict:
    values = clean_values(values_from_form(data))
    VALUES.write_text(json.dumps(values, indent=2) + "\n", encoding="utf-8")
    target, target_kind = card_test_target(data)
    CARD_DIR.mkdir(exist_ok=True)
    results = []
    # Webex API acceptance does not prove visible Webex client rendering. These
    # cases verify that provider send preserves payload shape in the Webex API
    # so the returned summaries can be compared with what renders in the client.
    for index, case in enumerate(CARD_CASES, start=1):
        card = adaptive_card(case["id"])
        card_path = CARD_DIR / f"{index:02d}-{case['id']}.json"
        card_path.write_text(json.dumps(card, indent=2) + "\n", encoding="utf-8")
        send = run_tester([
            "send",
            "--provider", "webex",
            "--values", str(VALUES),
            "--to", target,
            "--to-kind", target_kind,
            "--card", str(card_path),
        ])
        message_id = extract_message_id(send)
        verify = None
        error = None
        if send["ok"] and message_id:
            try:
                fetched = webex_get_message(message_id)
                verify = summarize_fetched_message(fetched, case["expected_actions"])
            except Exception as exc:
                error = str(exc)
        elif send["ok"]:
            error = "send succeeded but message id could not be extracted"
        else:
            error = send.get("stderr") or send.get("stdout") or "send failed"
        accepted = bool(send["ok"] and message_id)
        pass_checks = {"api_accepted_message": accepted}
        if verify and isinstance(verify.get("pass"), dict):
            pass_checks.update(verify["pass"])
        expected_keys = [
            "api_accepted_message",
            "attachment_exists",
            "content_type_adaptive_card",
            "card_type_present",
            "card_version_present",
            "actions_preserved",
            "booleans_are_booleans",
        ]
        required_ok_keys = [key for key in expected_keys if key != "booleans_are_booleans" or not case.get("known_schema_risk")]
        results.append({
            "case": case["id"],
            "name": case["name"],
            "message_id": message_id or None,
            "known_schema_risk": case.get("known_schema_risk"),
            "pass": {key: pass_checks.get(key, False) for key in expected_keys},
            "ok": all(pass_checks.get(key, False) for key in required_ok_keys),
            "verification": verify,
            "error": error,
        })
        time.sleep(1.2)
    summary = {
        "ok": all(item["ok"] for item in results),
        "target": target,
        "note": "API preservation does not prove Webex client rendering; compare each case with the visible Webex messages.",
        "results": results,
    }
    append_event("adaptive-card-tests", summary)
    return summary


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
    <div class="row">
      <strong>Last inbound:</strong>
      <code id="conversation">waiting for first Webex message...</code>
    </div>
  </section>
  <section>
    <h2>Setup</h2>
    <div class="grid">
      <label>Webex bot token*<input id="bot_token" type="password" autocomplete="off" placeholder="Bearer token"></label>
      <label>Tenant<input id="tenant" value="default"></label>
      <label>Channel<input id="channel" value="default"></label>
    </div>
    <p class="muted">The tester generates a 20-character webhook secret internally. Room and person defaults are intentionally omitted; send a message to the bot first so the provider can capture the room/person context.</p>
    <div class="row">
      <button id="registerBtn">Register webhook with Webex</button>
      <button id="saveBtn" type="button">Save values only</button>
    </div>
  </section>
  <section>
    <h2>Send</h2>
    <div class="grid">
      <label>Destination ID<input id="send_to" placeholder="Leave blank to use last inbound room"></label>
      <label>Destination kind<select id="send_kind">
        <option value="room">Room</option>
        <option value="person">Person ID</option>
        <option value="email">Person email</option>
      </select></label>
      <label>Message<textarea id="send_text" rows="3">Hello from Greentic Webex tester</textarea></label>
    </div>
    <div class="row">
      <button id="sendBtn">Send Webex message</button>
      <button id="useLastRoomBtn" type="button">Use last room</button>
      <button id="useLastPersonBtn" type="button">Use last person</button>
    </div>
  </section>
  <section>
    <h2>Adaptive Cards</h2>
    <p class="muted">These tests send named Adaptive Card variants through the provider send path and then fetch the created Webex message. API acceptance and payload preservation do not prove Webex client rendering; compare these summaries with the messages you visibly see in Webex.</p>
    <div class="grid">
      <label>Card test room ID<input id="card_room_id" placeholder="Defaults to WEBEX_ROOM_ID or last inbound room"></label>
      <label>Webex content image URL<input id="webex_content_image_url" placeholder="Optional /v1/contents/... URL for image compatibility test"></label>
    </div>
    <div class="row">
      <button id="cardTestsBtn" type="button">Run card compatibility tests</button>
    </div>
  </section>
  <section>
    <h2>Incoming Webhooks</h2>
    <div class="row">
      <button id="refreshBtn" type="button">Refresh</button>
      <button id="simulateMembershipBtn" type="button">Simulate Membership Created</button>
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
const formIds = ["bot_token","tenant","channel","send_to","send_kind","send_text","card_room_id","webex_content_image_url"];
const persistedIds = ["tenant","channel","send_to","send_kind","send_text","card_room_id","webex_content_image_url"];
localStorage.removeItem("webexTester.bot_token");
for (const id of persistedIds) {{
  const saved = localStorage.getItem("webexTester." + id);
  if (saved !== null) document.getElementById(id).value = saved;
  document.getElementById(id).addEventListener("input", e => localStorage.setItem("webexTester." + id, e.target.value));
}}
function formValues() {{
  return Object.fromEntries(formIds.map(id => [id, document.getElementById(id).value.trim()]));
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
  const conv = data.conversation || {{}};
  document.getElementById("conversation").textContent = Object.keys(conv).length ? JSON.stringify(conv) : "waiting for first Webex message...";
  window.lastWebexConversation = conv;
}}
document.getElementById("saveBtn").onclick = () => post("/api/save", formValues());
document.getElementById("registerBtn").onclick = async e => {{ e.target.disabled = true; try {{ await post("/api/register", formValues()); await refresh(); }} finally {{ e.target.disabled = false; }} }};
document.getElementById("sendBtn").onclick = async e => {{ e.target.disabled = true; try {{ await post("/api/send", formValues()); }} finally {{ e.target.disabled = false; }} }};
document.getElementById("cardTestsBtn").onclick = async e => {{ e.target.disabled = true; try {{ await post("/api/card-tests", formValues()); await refresh(); }} finally {{ e.target.disabled = false; }} }};
document.getElementById("simulateMembershipBtn").onclick = async e => {{ e.target.disabled = true; try {{ await post("/api/simulate/membership-created", formValues()); await refresh(); }} finally {{ e.target.disabled = false; }} }};
document.getElementById("useLastRoomBtn").onclick = () => {{
  const conv = window.lastWebexConversation || {{}};
  if (conv.room_id) {{
    document.getElementById("send_to").value = conv.room_id;
    document.getElementById("send_kind").value = "room";
  }}
}};
document.getElementById("useLastPersonBtn").onclick = () => {{
  const conv = window.lastWebexConversation || {{}};
  if (conv.person_email) {{
    document.getElementById("send_to").value = conv.person_email;
    document.getElementById("send_kind").value = "email";
  }} else if (conv.person_id) {{
    document.getElementById("send_to").value = conv.person_id;
    document.getElementById("send_kind").value = "person";
  }}
}};
document.getElementById("refreshBtn").onclick = refresh;
setInterval(refresh, 3000);
refresh();
</script>
</body>
</html>""".encode("utf-8")


def simulate_webex_membership(data: dict) -> dict:
    tenant = data.get("tenant") or "default"
    channel = data.get("channel") or "default"
    path = webhook_path(tenant, channel)
    body = {
        "resource": "memberships",
        "event": "created",
        "data": {
            "id": f"membership-{int(time.time() * 1000)}",
            "roomId": data.get("send_to") or "room-local",
            "personId": "person-local",
            "personEmail": "ada@example.com",
        },
    }
    http_in = {
        "method": "POST",
        "path": path,
        "headers": {"content-type": "application/json"},
        "body": json.dumps(body),
    }
    http_path = WORK / f"webex-lifecycle-{int(time.time() * 1000)}.json"
    http_path.write_text(json.dumps(http_in, indent=2) + "\n", encoding="utf-8")
    result = run_tester([
        "ingress",
        "--provider", "webex",
        "--values", str(VALUES),
        "--http-in", str(http_path),
        "--public-base-url", public_url(),
    ])
    append_event("simulate-lifecycle", {"http_in": str(http_path), "body": body, "result": result})
    return result


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
            self.send_json({
                "public_url": public_url(),
                "webhook_url": callback_url(),
                "conversation": read_conversation(),
                "events": read_events(),
            })
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
            result = run_tester(args)
            append_event("register", {"target": target, "result": result})
            self.send_json(result, 200 if result["ok"] else 500)
            return
        if path == "/api/send":
            data = self.read_json()
            values = clean_values(values_from_form(data))
            VALUES.write_text(json.dumps(values, indent=2) + "\n", encoding="utf-8")
            conversation = read_conversation()
            target = data.get("send_to") or conversation.get("room_id") or conversation.get("person_email") or conversation.get("person_id") or ""
            target_kind = data.get("send_kind") or ("email" if "@" in target else "room")
            if not data.get("send_to") and conversation.get("room_id"):
                target_kind = "room"
            elif not data.get("send_to") and conversation.get("person_email"):
                target_kind = "email"
            elif not data.get("send_to") and conversation.get("person_id"):
                target_kind = "person"
            if not target:
                result = {
                    "ok": False,
                    "status": 2,
                    "stdout": "",
                    "stderr": "No destination is known yet. Send a message to the Webex bot first, or enter a room ID, person ID, or person email.\n",
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
        if path == "/api/card-tests":
            data = self.read_json()
            try:
                result = run_card_compat_tests(data)
                self.send_json(result, 200 if result["ok"] else 500)
            except Exception as exc:
                result = {"ok": False, "error": str(exc)}
                append_event("adaptive-card-tests-error", result)
                self.send_json(result, 500)
            return
        if path == "/api/simulate/membership-created":
            data = self.read_json()
            result = simulate_webex_membership(data)
            self.send_json(result, 200 if result.get("ok") else 500)
            return
        if path.startswith("/v1/messaging/ingress/messaging-webex/"):
            length = int(self.headers.get("content-length") or "0")
            raw = self.rfile.read(length) if length else b""
            body_text = raw.decode("utf-8", errors="replace")
            headers = {k: v for k, v in self.headers.items()}
            conversation = update_conversation_from_webhook_body(body_text)
            append_event("webhook-received", {"path": path, "headers": headers, "body": body_text, "conversation": conversation})
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
            conversation = update_conversation_from_result(result) or conversation
            append_event("ingress", {"http_in": str(http_path), "conversation": conversation, "result": result})
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
WEBHOOK_URL="${PUBLIC_URL}/v1/messaging/ingress/messaging-webex/default/default"
echo "Webex tester UI: ${LOCAL_URL}"
echo "Public URL: ${PUBLIC_URL}"
echo "Default Webex ingress URL: ${WEBHOOK_URL}"
echo "Logs: ${WORK_DIR}"

if [ "${OPEN_BROWSER}" -eq 1 ]; then
  if command -v xdg-open >/dev/null 2>&1; then
    xdg-open "${LOCAL_URL}" >/dev/null 2>&1 || true
  elif command -v open >/dev/null 2>&1; then
    open "${LOCAL_URL}" >/dev/null 2>&1 || true
  fi
fi

wait "${SERVER_PID}"
