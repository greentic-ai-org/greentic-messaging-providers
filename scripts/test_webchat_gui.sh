#!/usr/bin/env bash
# Build/extract the current webchat-gui pack and open a local browser harness.
#
# Usage:
#   scripts/test_webchat_gui.sh [skin] [--no-build] [--no-open] [--port <port>] [--nav-link <spec>] [--nav-links-json <json|@file>] [--demo-links]
#
# Examples:
#   scripts/test_webchat_gui.sh
#   scripts/test_webchat_gui.sh 3aigent
#   scripts/test_webchat_gui.sh cisco --no-build --port 8765
#   scripts/test_webchat_gui.sh 3aigent --demo-links
#   scripts/test_webchat_gui.sh 3aigent --nav-link 'M1|Playground|https://example.com'

set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "${ROOT_DIR}"

SKIN="default"
BUILD=1
OPEN_BROWSER=1
PORT="${PORT:-8787}"
DEMO_LINKS=0
NAV_LINKS_JSON=""
NAV_LINK_ARGS=()

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
    --demo-links)
      DEMO_LINKS=1
      ;;
    --nav-link)
      shift
      if [ -z "${1:-}" ]; then
        echo "--nav-link requires a value" >&2
        exit 2
      fi
      NAV_LINK_ARGS+=("$1")
      ;;
    --nav-link=*)
      NAV_LINK_ARGS+=("${1#--nav-link=}")
      ;;
    --nav-links-json)
      shift
      if [ -z "${1:-}" ]; then
        echo "--nav-links-json requires a JSON value or @file" >&2
        exit 2
      fi
      NAV_LINKS_JSON="$1"
      ;;
    --nav-links-json=*)
      NAV_LINKS_JSON="${1#--nav-links-json=}"
      ;;
    -*)
      echo "unknown argument: $1" >&2
      exit 2
      ;;
    *)
      if [ "${SKIN}" != "default" ]; then
        echo "only one skin argument is supported" >&2
        exit 2
      fi
      SKIN="$1"
      ;;
  esac
  shift
done

VERSION="$(python3 tools/provider_versions.py provider webchat-gui)"
PACK="dist/packs/messaging-webchat-gui.gtpack"
WORK_DIR="${TMPDIR:-/tmp}/greentic-webchat-gui-test-${VERSION}-${SKIN}"
EXTRACT_DIR="${WORK_DIR}/pack"
WWW_DIR="${WORK_DIR}/www"

if [ "${BUILD}" -eq 1 ]; then
  scripts/build_providers.sh webchat-gui
elif [ ! -f "${PACK}" ]; then
  echo "${PACK} not found; run without --no-build first" >&2
  exit 1
fi

if [ ! -f "${PACK}" ]; then
  echo "pack build did not produce ${PACK}" >&2
  exit 1
fi

rm -rf "${WORK_DIR}"
mkdir -p "${EXTRACT_DIR}" "${WWW_DIR}/v1/web/webchat"
unzip -q "${PACK}" -d "${EXTRACT_DIR}"

ASSET_DIR="${EXTRACT_DIR}/assets/webchat-gui"
if [ ! -f "${ASSET_DIR}/index.html" ]; then
  echo "pack is missing assets/webchat-gui/index.html" >&2
  exit 1
fi
if [ ! -f "${ASSET_DIR}/skins/${SKIN}/skin.json" ]; then
  echo "skin '${SKIN}' not found in pack. Available skins:" >&2
  find "${ASSET_DIR}/skins" -mindepth 1 -maxdepth 1 -type d -printf '  %f\n' | sort >&2
  exit 1
fi

python3 - "${ASSET_DIR}" "${SKIN}" "${DEMO_LINKS}" "${NAV_LINKS_JSON}" "${NAV_LINK_ARGS[@]}" <<'PY'
from __future__ import annotations

import json
import sys
from pathlib import Path


asset_dir = Path(sys.argv[1])
skin = sys.argv[2]
demo_links = sys.argv[3] == "1"
nav_links_json = sys.argv[4]
nav_specs = sys.argv[5:]
tenant_path = asset_dir / "config" / "tenants" / f"{skin}.json"

DEMO_NAV_LINKS = [
    {
        "label": "Playground",
        "num": "M1",
        "tooltip": {
            "eyebrow": "Module 1",
            "title": "LLM Behaviour Playground",
            "lede": "LLM Behaviour Playground Edit the system prompt and adjust the temperature live. See how persona, voice, and creativity shift turn-by-turn - every reply is stateless.",
        },
        "url": "https://intro-to-ai-m1.apps.rosa.rosa-3aigent.mqdb.p3.openshiftapps.com/",
    },
    {
        "label": "Red/Blue",
        "num": "M2",
        "tooltip": {
            "eyebrow": "Module 2",
            "title": "Prompt Security - Red/Blue",
            "lede": "Two teams, one model. Blue shapes the system prompt to block numbers; Red crafts user attacks. Each attempt is auto-scored.",
        },
        "url": "https://intro-to-ai-m2.apps.rosa.rosa-3aigent.mqdb.p3.openshiftapps.com/",
    },
    {
        "label": "Compare",
        "num": "M3",
        "tooltip": {
            "eyebrow": "Module 3",
            "title": "Fine-Tuning is not knowledge",
            "lede": "Side-by-side base and fine-tuned answers. Proves fine-tuning shapes behaviour, not facts - the bridge to retrieval-augmented generation.",
        },
        "url": "https://intro-to-ai-m3.apps.rosa.rosa-3aigent.mqdb.p3.openshiftapps.com/",
    },
    {
        "label": "RAG",
        "num": "M4",
        "tooltip": {
            "eyebrow": "Module 4",
            "title": "RAG - The Knowledge Tool",
            "lede": "Ask before ingestion, then ingest the corpus for grounded, cited answers. Retrieval closes the knowledge gap fine-tuning cannot.",
        },
        "url": "https://intro-to-ai-m4.apps.rosa.rosa-3aigent.mqdb.p3.openshiftapps.com/",
    },
]


def parse_nav_json(raw: str) -> list[dict]:
    if not raw:
        return []
    if raw.startswith("@"):
        raw = Path(raw[1:]).read_text(encoding="utf-8")
    data = json.loads(raw)
    if isinstance(data, dict):
        data = data.get("nav_links", [])
    if not isinstance(data, list):
        raise SystemExit("--nav-links-json must be an array or an object with nav_links")
    return data


def parse_nav_spec(spec: str) -> dict:
    parts = [part.strip() for part in spec.split("|")]
    if len(parts) == 1 and "=" in parts[0]:
        label, url = [part.strip() for part in parts[0].split("=", 1)]
        return {"label": label, "url": url}
    if len(parts) == 2:
        label, url = parts
        return {"label": label, "url": url}
    if len(parts) == 3:
        num, label, url = parts
        return {"num": num, "label": label, "url": url}
    if len(parts) >= 6:
        num, label, url, title, eyebrow, lede = parts[:6]
        return {
            "num": num,
            "label": label,
            "url": url,
            "tooltip": {"title": title, "eyebrow": eyebrow, "lede": lede},
        }
    raise SystemExit(
        "invalid --nav-link. Use 'label|url', 'num|label|url', "
        "or 'num|label|url|tooltip-title|tooltip-eyebrow|tooltip-lede'"
    )


nav_links: list[dict] = []
if demo_links:
    nav_links.extend(DEMO_NAV_LINKS)
nav_links.extend(parse_nav_json(nav_links_json))
nav_links.extend(parse_nav_spec(spec) for spec in nav_specs)

if not nav_links:
    raise SystemExit(0)

if tenant_path.exists():
    config = json.loads(tenant_path.read_text(encoding="utf-8"))
else:
    config = {
        "tenant_id": skin,
        "skin": skin,
        "legacy_skin": skin,
        "branding": {"company_name": skin},
        "auth": {"providers": [{"id": "guest", "label": "Continue as Guest", "type": "dummy", "enabled": True}]},
        "webchat": {"directline": {}, "locale": "en-US"},
    }

config["tenant_id"] = config.get("tenant_id") or skin
config["skin"] = skin
config["nav_links"] = nav_links
tenant_path.parent.mkdir(parents=True, exist_ok=True)
tenant_path.write_text(json.dumps(config, indent=2, ensure_ascii=False) + "\n", encoding="utf-8")
print(f"wrote top-bar nav links to {tenant_path}")
PY

ln -s "${ASSET_DIR}" "${WWW_DIR}/v1/web/webchat/${SKIN}"
ln -s "${ASSET_DIR}/skins" "${WWW_DIR}/skins"

cat > "${WWW_DIR}/test.html" <<EOF
<!doctype html>
<html lang="en">
  <head>
    <meta charset="utf-8">
    <meta name="viewport" content="width=device-width, initial-scale=1">
    <title>WebChat GUI Pack Test ${VERSION}</title>
    <style>
      :root { color-scheme: light dark; font-family: system-ui, sans-serif; }
      body { margin: 0; background: #111827; color: #f9fafb; }
      header { height: 48px; display: flex; align-items: center; gap: 16px; padding: 0 16px; border-bottom: 1px solid #374151; }
      strong { font-size: 14px; }
      a { color: #93c5fd; }
      iframe { width: 100vw; height: calc(100vh - 49px); border: 0; display: block; background: white; }
      code { background: #1f2937; padding: 2px 6px; border-radius: 4px; }
    </style>
  </head>
  <body>
    <header>
      <strong>webchat-gui ${VERSION}</strong>
      <span>skin: <code>${SKIN}</code></span>
      <a href="/v1/web/webchat/${SKIN}/?tenant=${SKIN}" target="_blank" rel="noreferrer">open app tab</a>
      <a href="/v1/web/webchat/${SKIN}/embed.js" target="_blank" rel="noreferrer">embed.js</a>
      <span>Direct Line is mocked locally.</span>
    </header>
    <iframe src="/v1/web/webchat/${SKIN}/?tenant=${SKIN}"></iframe>
  </body>
</html>
EOF

cat > "${WORK_DIR}/server.py" <<'PY'
from __future__ import annotations

import json
import os
import posixpath
from http.server import SimpleHTTPRequestHandler, ThreadingHTTPServer
from pathlib import Path
from urllib.parse import unquote, urlparse


ROOT = Path(os.environ["WEBCHAT_TEST_ROOT"]).resolve()
CONVERSATIONS: dict[str, list[dict]] = {}


def json_bytes(value: dict) -> bytes:
    return json.dumps(value).encode("utf-8")


class Handler(SimpleHTTPRequestHandler):
    server_version = "GreenticWebChatGuiTest/1.0"

    def translate_path(self, path: str) -> str:
        path = urlparse(path).path
        path = posixpath.normpath(unquote(path))
        parts = [part for part in path.split("/") if part and part not in (".", "..")]
        resolved = ROOT
        for part in parts:
            resolved = resolved / part
        if path.endswith("/") or resolved.is_dir():
            resolved = resolved / "index.html"
        return str(resolved)

    def end_headers(self) -> None:
        self.send_header("Cache-Control", "no-store")
        self.send_header("Access-Control-Allow-Origin", "*")
        self.send_header("Access-Control-Allow-Headers", "authorization,content-type,x-greentic-locale")
        self.send_header("Access-Control-Allow-Methods", "GET,POST,OPTIONS")
        super().end_headers()

    def do_OPTIONS(self) -> None:
        self.send_response(204)
        self.end_headers()

    def send_json(self, value: dict, status: int = 200) -> None:
        body = json_bytes(value)
        self.send_response(status)
        self.send_header("Content-Type", "application/json")
        self.send_header("Content-Length", str(len(body)))
        self.end_headers()
        self.wfile.write(body)

    def do_POST(self) -> None:
        parsed = urlparse(self.path)
        path = parsed.path
        length = int(self.headers.get("content-length") or 0)
        if length:
            self.rfile.read(length)

        if path.endswith("/token") or path.endswith("/v3/directline/tokens/generate"):
            self.send_json({
                "token": "local-test-token",
                "expires_in": 1800,
                "conversationId": "local-test-conversation",
            })
            return

        if path.endswith("/v3/directline/conversations"):
            conversation_id = "local-test-conversation"
            CONVERSATIONS.setdefault(conversation_id, [{
                "type": "message",
                "id": "welcome",
                "timestamp": "2026-01-01T00:00:00.000Z",
                "from": {"id": "greentic-test-bot", "name": "Greentic Test Bot"},
                "text": "Local webchat-gui test backend is connected.",
            }])
            self.send_json({
                "conversationId": conversation_id,
                "token": "local-test-token",
                "expires_in": 1800,
            })
            return

        if "/v3/directline/conversations/" in path and path.endswith("/activities"):
            conversation_id = path.split("/v3/directline/conversations/", 1)[1].split("/", 1)[0]
            activities = CONVERSATIONS.setdefault(conversation_id, [])
            activity_id = f"activity-{len(activities) + 1}"
            activities.append({
                "type": "message",
                "id": activity_id,
                "timestamp": "2026-01-01T00:00:01.000Z",
                "from": {"id": "greentic-test-bot", "name": "Greentic Test Bot"},
                "text": "Echo from local test backend.",
            })
            self.send_json({"id": activity_id})
            return

        self.send_json({"error": "not found", "path": path}, status=404)

    def do_GET(self) -> None:
        parsed = urlparse(self.path)
        path = parsed.path

        if path == "/":
            self.send_response(302)
            self.send_header("Location", "/test.html")
            self.end_headers()
            return

        if "/v3/directline/conversations/" in path and path.endswith("/activities"):
            conversation_id = path.split("/v3/directline/conversations/", 1)[1].split("/", 1)[0]
            activities = CONVERSATIONS.setdefault(conversation_id, [])
            self.send_json({"activities": activities, "watermark": str(len(activities))})
            return

        super().do_GET()


if __name__ == "__main__":
    port = int(os.environ["WEBCHAT_TEST_PORT"])
    httpd = ThreadingHTTPServer(("127.0.0.1", port), Handler)
    print(f"Serving webchat-gui test harness on http://127.0.0.1:{port}/test.html", flush=True)
    httpd.serve_forever()
PY

URL="http://127.0.0.1:${PORT}/test.html"

WEBCHAT_TEST_ROOT="${WWW_DIR}" WEBCHAT_TEST_PORT="${PORT}" python3 "${WORK_DIR}/server.py" &
SERVER_PID="$!"
trap 'kill "${SERVER_PID}" 2>/dev/null || true' EXIT

sleep 1
echo "webchat-gui version: ${VERSION}"
echo "skin: ${SKIN}"
if [ "${DEMO_LINKS}" -eq 1 ] || [ -n "${NAV_LINKS_JSON}" ] || [ "${#NAV_LINK_ARGS[@]}" -gt 0 ]; then
  echo "top-bar links: enabled"
fi
echo "url: ${URL}"
echo "Press Ctrl+C to stop the local test server."

if [ "${OPEN_BROWSER}" -eq 1 ]; then
  if command -v xdg-open >/dev/null 2>&1; then
    xdg-open "${URL}" >/dev/null 2>&1 || true
  elif command -v open >/dev/null 2>&1; then
    open "${URL}" >/dev/null 2>&1 || true
  else
    echo "No browser opener found; open ${URL} manually."
  fi
fi

wait "${SERVER_PID}"
