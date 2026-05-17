#!/usr/bin/env bash
# Build/extract the current webchat-gui pack and open a local browser harness
# with a mocked Direct Line backend and sample Adaptive Card activity.
#
# Usage:
#   scripts/test_webchat_gui.sh [skin] [--embedded] [--login] [--no-text-input] [--no-build] [--no-open] [--port <port>] [--nav-link <spec>] [--nav-links-json <json|@file>] [--demo-links]
#
# Examples:
#   scripts/test_webchat_gui.sh
#   scripts/test_webchat_gui.sh 3aigent
#   scripts/test_webchat_gui.sh 3aigent --no-build --port 8765
#   scripts/test_webchat_gui.sh 3aigent --demo-links
#   scripts/test_webchat_gui.sh 3aigent --embedded
#   scripts/test_webchat_gui.sh 3aigent --login
#   scripts/test_webchat_gui.sh 3aigent --embedded --no-text-input
#   scripts/test_webchat_gui.sh 3aigent --nav-link 'M1|Playground|https://example.com'

set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "${ROOT_DIR}"

SKIN="default"
BUILD=1
OPEN_BROWSER=1
PORT="${PORT:-8787}"
DEMO_LINKS=0
EMBEDDED=0
LOGIN_REQUIRED=0
TEXT_INPUT=1
NAV_LINKS_JSON=""
NAV_LINK_ARGS=()

while [ $# -gt 0 ]; do
  case "$1" in
    -h|--help)
      sed -n '2,15p' "$0" >&2
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
    --embedded)
      EMBEDDED=1
      ;;
    --login-required|--login)
      LOGIN_REQUIRED=1
      ;;
    --no-text-input|--disable-text-input)
      TEXT_INPUT=0
      ;;
    --text-input)
      TEXT_INPUT=1
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

python3 - "${ASSET_DIR}" "${SKIN}" "${DEMO_LINKS}" "${TEXT_INPUT}" "${LOGIN_REQUIRED}" "${NAV_LINKS_JSON}" "${NAV_LINK_ARGS[@]}" <<'PY'
from __future__ import annotations

import json
import sys
from pathlib import Path


asset_dir = Path(sys.argv[1])
skin = sys.argv[2]
demo_links = sys.argv[3] == "1"
text_input_enabled = sys.argv[4] == "1"
login_required = sys.argv[5] == "1"
nav_links_json = sys.argv[6]
nav_specs = sys.argv[7:]
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

if tenant_path.exists():
    config = json.loads(tenant_path.read_text(encoding="utf-8"))
else:
    config = {
        "tenant_id": skin,
        "skin": skin,
        "legacy_skin": skin,
        "branding": {"company_name": skin},
        "webchat": {"directline": {}, "locale": "en-US"},
    }

config["tenant_id"] = config.get("tenant_id") or skin
config["skin"] = skin
config["legacy_skin"] = skin
if login_required:
    auth = config.setdefault("auth", {})
    providers = auth.get("providers")
    if not isinstance(providers, list) or not providers:
        auth["providers"] = [{"id": "guest", "label": "Continue as Guest", "type": "dummy", "enabled": True}]
else:
    config.pop("auth", None)
webchat = config.setdefault("webchat", {})
style_options = webchat.setdefault("style_options", {})
if text_input_enabled:
    style_options.pop("hideSendBox", None)
else:
    style_options["hideSendBox"] = True
if nav_links:
    config["nav_links"] = nav_links
tenant_path.parent.mkdir(parents=True, exist_ok=True)
tenant_path.write_text(json.dumps(config, indent=2, ensure_ascii=False) + "\n", encoding="utf-8")
print(f"wrote tenant test config to {tenant_path}")
PY

ln -s "${ASSET_DIR}" "${WWW_DIR}/v1/web/webchat/${SKIN}"
ln -s "${ASSET_DIR}/skins" "${WWW_DIR}/skins"

TEXT_INPUT_VALUE="true"
TEXT_INPUT_QUERY=""
if [ "${TEXT_INPUT}" -eq 0 ]; then
  TEXT_INPUT_VALUE="false"
  TEXT_INPUT_QUERY="&textInput=false"
fi

if [ "${EMBEDDED}" -eq 1 ]; then
  cat > "${WWW_DIR}/test.html" <<EOF
<!doctype html>
<html lang="en">
  <head>
    <meta charset="utf-8">
    <meta name="viewport" content="width=device-width, initial-scale=1">
    <title>Embedded WebChat GUI Test ${VERSION}</title>
    <script type="module" src="/v1/web/webchat/${SKIN}/embed.js"></script>
    <style>
      :root { color-scheme: light; font-family: Inter, ui-sans-serif, system-ui, -apple-system, BlinkMacSystemFont, "Segoe UI", sans-serif; }
      * { box-sizing: border-box; }
      body {
        min-height: 100vh;
        margin: 0;
        background: #eef1f5;
        color: #111827;
      }
      body > main {
        width: min(1180px, calc(100vw - 32px));
        min-height: 100vh;
        margin: 0 auto;
        padding: 14px 0 24px;
        display: grid;
        align-content: start;
        gap: 12px;
      }
      .page-head h1 {
        margin: 0;
        font-size: 28px;
        line-height: 1.12;
      }
      .page-head p {
        max-width: 640px;
        margin: 0;
        color: #4b5563;
        line-height: 1.35;
      }
      .page-head {
        display: grid;
        gap: 6px;
      }
      .actions {
        display: flex;
        align-items: center;
        gap: 10px;
        flex-wrap: wrap;
      }
      .actions button,
      .popup__close,
      .button {
        border: 1px solid transparent;
        border-radius: 8px;
        min-height: 42px;
        padding: 0 14px;
        font: inherit;
        font-weight: 650;
        cursor: pointer;
        text-decoration: none;
        display: inline-flex;
        align-items: center;
        justify-content: center;
        gap: 8px;
        transition: transform 0.12s ease, box-shadow 0.12s ease, border-color 0.12s ease;
      }
      .actions button:hover,
      .popup__close:hover,
      .button:hover {
        transform: translateY(-1px);
      }
      .primary {
        background: #0f766e;
        color: white;
        box-shadow: 0 10px 22px rgba(15, 118, 110, 0.20);
      }
      .secondary {
        background: #f8fafc;
        color: #111827;
        border-color: #cbd5e1;
        box-shadow: 0 8px 18px rgba(15, 23, 42, 0.06);
      }
      code {
        border-radius: 6px;
        padding: 1px 5px;
        background: rgba(15, 23, 42, 0.07);
        font-size: 0.9em;
      }
      .mode-bar {
        display: grid;
        grid-template-columns: minmax(0, 1fr) auto;
        gap: 12px;
        align-items: center;
        padding: 12px 14px;
        border: 1px solid #d7dde7;
        border-radius: 10px;
        background: #ffffff;
        box-shadow: 0 10px 24px rgba(15, 23, 42, 0.06);
      }
      .mode-bar h2 {
        margin: 0;
        font-size: 16px;
        line-height: 1.3;
      }
      .mode-bar p {
        margin-top: 4px;
        max-width: 720px;
        font-size: 14px;
        color: #64748b;
      }
      .demo-grid {
        display: grid;
        grid-template-columns: minmax(0, 1fr) minmax(0, 1fr);
        gap: 16px;
        align-items: stretch;
      }
      .demo-panel {
        min-width: 0;
        display: grid;
        grid-template-rows: auto minmax(0, 1fr);
        gap: 9px;
      }
      .demo-panel h2 {
        margin: 0;
        font-size: 14px;
        line-height: 1.3;
        color: #334155;
      }
      .inline-demo {
        height: clamp(320px, calc(100vh - 270px), 620px);
        min-height: 0;
        overflow: hidden;
        border: 1px solid #cfd7e3;
        border-radius: 10px;
        background: white;
        box-shadow: 0 14px 34px rgba(15, 23, 42, 0.08);
      }
      .inline-demo greentic-webchat {
        display: block;
        width: 100%;
        height: 100%;
        min-height: 0;
      }
      .modal {
        position: fixed;
        inset: 0;
        z-index: 100;
        display: none;
        place-items: center;
        padding: 24px;
        background: rgba(17, 24, 39, 0.58);
      }
      .modal[data-open="true"] {
        display: grid;
      }
      .popup {
        width: min(460px, calc(100vw - 32px));
        height: min(720px, calc(100vh - 48px));
        position: relative;
        display: block;
        overflow: hidden;
        border-radius: 12px;
        background: white;
        box-shadow: 0 26px 80px rgba(0, 0, 0, 0.28);
      }
      .popup__body {
        width: 100%;
        height: 100%;
      }
      .popup__body greentic-webchat {
        display: block;
        width: 100%;
        height: 100%;
        min-height: 0;
      }
      .popup__close {
        position: absolute;
        top: 10px;
        right: 10px;
        z-index: 3;
        width: 34px;
        height: 34px;
        border-radius: 999px;
        border: 1px solid rgba(17, 24, 39, 0.12);
        background: rgba(255, 255, 255, 0.9);
        color: #111827;
        box-shadow: 0 8px 22px rgba(17, 24, 39, 0.16);
      }
      @media (max-width: 560px) {
        .modal {
          padding: 0;
        }
        .popup {
          width: 100vw;
          height: 100vh;
          border-radius: 0;
        }
        .demo-grid {
          grid-template-columns: 1fr;
        }
        .mode-bar {
          grid-template-columns: 1fr;
        }
        .inline-demo {
          height: calc(100vh - 260px);
          min-height: 300px;
        }
      }
    </style>
  </head>
  <body>
    <main>
      <header class="page-head">
        <h1>Embedded webchat-gui ${VERSION}</h1>
        <p>
          A host-page preview for the <code>${SKIN}</code> skin. Native mode should feel like part of the page;
          iframe mode stays isolated for safer drop-in installs. The mocked backend sends a sample Adaptive Card.
        </p>
      </header>
      <section class="mode-bar" aria-label="Other modes">
        <div>
          <h2>Other modes</h2>
          <p>Open the popup widget or launch the full-page native app in a new tab.</p>
        </div>
        <div class="actions">
          <button id="open-chat" class="primary" type="button">Open popup</button>
          <a class="button secondary" href="/login-required.html" target="_blank" rel="noreferrer">Open login test</a>
          <a class="button secondary" href="/v1/web/webchat/${SKIN}/?tenant=${SKIN}${TEXT_INPUT_QUERY}" target="_blank" rel="noreferrer">Open full-page app</a>
        </div>
      </section>
      <div class="demo-grid">
        <section class="demo-panel" aria-label="Inline web component">
          <h2>Web component: <code>mode="inline"</code> <code>render="iframe"</code></h2>
          <div id="inline-demo" class="inline-demo"></div>
        </section>
        <section class="demo-panel" aria-label="Native inline web component">
          <h2>Web component: <code>mode="inline"</code> <code>render="native"</code></h2>
          <div id="native-demo" class="inline-demo"></div>
        </section>
      </div>
    </main>

    <div id="modal" class="modal" role="dialog" aria-modal="true" aria-label="Embedded ${SKIN} WebChat">
      <section class="popup">
        <button id="close-chat" class="popup__close" type="button" aria-label="Close embedded chat">x</button>
        <div id="popup-body" class="popup__body"></div>
      </section>
    </div>

    <script>
      const modal = document.getElementById('modal');
      const popupBody = document.getElementById('popup-body');
      const inlineDemo = document.getElementById('inline-demo');
      const nativeDemo = document.getElementById('native-demo');
      const openButton = document.getElementById('open-chat');
      const closeButton = document.getElementById('close-chat');
      const createChat = (id, title, mode, render) => {
        const chat = document.createElement('greentic-webchat');
        chat.id = id;
        chat.setAttribute('tenant', '${SKIN}');
        chat.setAttribute('public-base-url', 'http://127.0.0.1:${PORT}');
        chat.setAttribute('mode', mode);
        chat.setAttribute('render', render);
        chat.setAttribute('text-input', '${TEXT_INPUT_VALUE}');
        chat.setAttribute('title', title);
        return chat;
      };
      let inlineChat = null;
      let nativeChat = null;
      let popupChat = null;
      const mountInlineChat = () => {
        if (inlineChat) return;
        inlineChat = createChat('inline-chat', 'Inline ${SKIN} WebChat', 'inline', 'iframe');
        inlineDemo.append(inlineChat);
      };
      const mountNativeChat = () => {
        if (nativeChat) return;
        nativeChat = createChat('native-chat', 'Native inline ${SKIN} WebChat', 'inline', 'native');
        nativeDemo.append(nativeChat);
      };
      const mountPopupChat = () => {
        if (popupChat) return;
        popupChat = createChat('popup-chat', 'Embedded ${SKIN} WebChat', 'popup', 'iframe');
        popupBody.append(popupChat);
      };
      const afterLayout = (callback) => {
        requestAnimationFrame(() => requestAnimationFrame(callback));
      };
      const setOpen = (open) => {
        modal.dataset.open = String(open);
        if (open) {
          afterLayout(() => {
            mountPopupChat();
            closeButton.focus();
          });
        } else {
          openButton.focus();
        }
      };
      openButton.addEventListener('click', () => setOpen(true));
      closeButton.addEventListener('click', () => setOpen(false));
      modal.addEventListener('click', (event) => {
        if (event.target === modal) setOpen(false);
      });
      window.addEventListener('keydown', (event) => {
        if (event.key === 'Escape') setOpen(false);
      });
      window.addEventListener('load', () => {
        afterLayout(() => {
          mountInlineChat();
          mountNativeChat();
        });
      }, { once: true });
    </script>
  </body>
</html>
EOF
else
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
      code { background: #1f2937; padding: 2px 6px; border-radius: 4px; }
      main { min-height: calc(100vh - 49px); display: grid; place-items: center; padding: 24px; text-align: center; }
      .button { display: inline-flex; margin-top: 16px; padding: 10px 14px; border-radius: 8px; background: #2563eb; color: white; text-decoration: none; }
    </style>
    <meta http-equiv="refresh" content="0; url=/v1/web/webchat/${SKIN}/?tenant=${SKIN}">
  </head>
  <body>
    <header>
      <strong>webchat-gui ${VERSION}</strong>
      <span>skin: <code>${SKIN}</code></span>
      <a href="/v1/web/webchat/${SKIN}/embed.js" target="_blank" rel="noreferrer">embed.js</a>
      <span>Direct Line is mocked locally.</span>
    </header>
    <main>
      <div>
        <p>Opening the full-page WebChat app directly, without an iframe.</p>
        <a class="button" href="/v1/web/webchat/${SKIN}/?tenant=${SKIN}">Open full-page app</a>
      </div>
    </main>
  </body>
</html>
EOF
fi

cat > "${WWW_DIR}/login-required.html" <<EOF
<!doctype html>
<html lang="en">
  <head>
    <meta charset="utf-8">
    <meta name="viewport" content="width=device-width, initial-scale=1">
    <title>Login-required WebChat GUI Test ${VERSION}</title>
    <style>
      :root {
        color-scheme: light;
        font-family: Inter, ui-sans-serif, system-ui, -apple-system, BlinkMacSystemFont, "Segoe UI", sans-serif;
      }
      * {
        box-sizing: border-box;
      }
      body {
        min-height: 100vh;
        margin: 0;
        background: #eef1f5;
        color: #111827;
      }
      main {
        width: min(720px, calc(100vw - 32px));
        min-height: 100vh;
        margin: 0 auto;
        padding: 28px 0 36px;
        display: grid;
        place-items: center;
        gap: 18px;
        text-align: center;
      }
      .card {
        display: grid;
        gap: 14px;
        padding: 28px;
        border: 1px solid #d7dde7;
        border-radius: 10px;
        background: #ffffff;
        box-shadow: 0 14px 34px rgba(15, 23, 42, 0.08);
      }
      h1 {
        margin: 0;
        font-size: 28px;
        line-height: 1.12;
      }
      p {
        margin: 0;
        color: #4b5563;
        line-height: 1.55;
      }
      code {
        border-radius: 6px;
        padding: 1px 5px;
        background: rgba(15, 23, 42, 0.07);
        font-size: 0.9em;
      }
    </style>
  </head>
  <body>
    <main>
      <section class="card" aria-live="polite">
        <h1>Login-required webchat-gui ${VERSION}</h1>
        <p>
          Clearing local test auth state, then opening the real <code>${SKIN}</code> full-page login experience.
        </p>
      </section>
    </main>
    <script>
      try {
        localStorage.removeItem('webchat_auth_session');
        sessionStorage.clear();
      } catch (_) {
        // Storage may be unavailable in hardened browser modes.
      }
      const appUrl = '/v1/web/webchat/${SKIN}/?tenant=${SKIN}${TEXT_INPUT_QUERY}';
      window.location.replace(appUrl + (appUrl.includes('?') ? '&' : '?') + 'loginRequired=' + Date.now());
    </script>
  </body>
</html>
EOF

cat > "${WORK_DIR}/server.py" <<'PY'
from __future__ import annotations

import base64
import hashlib
import json
import os
import posixpath
import time
from http.server import SimpleHTTPRequestHandler, ThreadingHTTPServer
from pathlib import Path
from urllib.parse import unquote, urlparse


ROOT = Path(os.environ["WEBCHAT_TEST_ROOT"]).resolve()
SKIN = os.environ.get("WEBCHAT_TEST_SKIN", "default")
CONVERSATIONS: dict[str, list[dict]] = {}


def conversation_id_from_path(path: str) -> str | None:
    marker = "/v3/directline/conversations/"
    if marker not in path:
        return None
    tail = path.split(marker, 1)[1].strip("/")
    if not tail:
        return None
    return tail.split("/", 1)[0]


def sample_adaptive_card_activity() -> dict:
    return {
        "type": "message",
        "id": "sample-adaptive-card",
        "timestamp": "2026-01-01T00:00:00.500Z",
        "from": {"id": "greentic-test-bot", "name": "Greentic Test Bot"},
        "attachments": [
            {
                "contentType": "application/vnd.microsoft.card.adaptive",
                "content": {
                    "$schema": "http://adaptivecards.io/schemas/adaptive-card.json",
                    "type": "AdaptiveCard",
                    "version": "1.5",
                    "body": [
                        {
                            "type": "TextBlock",
                            "text": "Sample Adaptive Card",
                            "weight": "Bolder",
                            "size": "Medium",
                            "wrap": True,
                        },
                        {
                            "type": "TextBlock",
                            "text": "Rendered by the local WebChat GUI test harness.",
                            "isSubtle": True,
                            "wrap": True,
                        },
                        {
                            "type": "FactSet",
                            "facts": [
                                {"title": "Skin", "value": SKIN},
                                {"title": "Width", "value": "Default adaptive card width"},
                                {"title": "Backend", "value": "Local mock Direct Line"},
                            ],
                        },
                    ],
                    "actions": [
                        {
                            "type": "Action.OpenUrl",
                            "title": "Adaptive Cards docs",
                            "url": "https://adaptivecards.io/",
                        }
                    ],
                },
            }
        ],
    }


def initial_activities() -> list[dict]:
    return [
        {
            "type": "message",
            "id": "welcome",
            "timestamp": "2026-01-01T00:00:00.000Z",
            "from": {"id": "greentic-test-bot", "name": "Greentic Test Bot"},
            "text": "Local webchat-gui test backend is connected.",
        },
        sample_adaptive_card_activity(),
    ]


def json_bytes(value: dict) -> bytes:
    return json.dumps(value).encode("utf-8")


def websocket_frame(payload: dict) -> bytes:
    data = json_bytes(payload)
    length = len(data)
    if length < 126:
        header = bytes([0x81, length])
    elif length < 65536:
        header = bytes([0x81, 126, (length >> 8) & 0xFF, length & 0xFF])
    else:
        header = bytes([
            0x81,
            127,
            (length >> 56) & 0xFF,
            (length >> 48) & 0xFF,
            (length >> 40) & 0xFF,
            (length >> 32) & 0xFF,
            (length >> 24) & 0xFF,
            (length >> 16) & 0xFF,
            (length >> 8) & 0xFF,
            length & 0xFF,
        ])
    return header + data


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
            CONVERSATIONS.setdefault(conversation_id, initial_activities())
            host = self.headers.get("Host", "127.0.0.1")
            self.send_json({
                "conversationId": conversation_id,
                "token": "local-test-token",
                "streamUrl": f"ws://{host}/v1/messaging/webchat/{SKIN}/v3/directline/conversations/{conversation_id}/stream",
                "expires_in": 1800,
            })
            return

        conversation_id = conversation_id_from_path(path)
        if conversation_id and path.endswith("/activities"):
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

        if self.headers.get("Upgrade", "").lower() == "websocket":
            self.handle_websocket(path)
            return

        if path.endswith("/undefined"):
            self.send_response(204)
            self.end_headers()
            return

        if path == "/":
            self.send_response(302)
            self.send_header("Location", "/test.html")
            self.end_headers()
            return

        conversation_id = conversation_id_from_path(path)
        if conversation_id:
            activities = CONVERSATIONS.setdefault(conversation_id, initial_activities())
            self.send_json({"activities": activities, "watermark": str(len(activities))})
            return

        super().do_GET()

    def handle_websocket(self, path: str) -> None:
        conversation_id = conversation_id_from_path(path) or "local-test-conversation"
        key = self.headers.get("Sec-WebSocket-Key")
        if not key:
            self.send_error(400, "Missing Sec-WebSocket-Key")
            return
        accept = base64.b64encode(hashlib.sha1((key + "258EAFA5-E914-47DA-95CA-C5AB0DC85B11").encode("ascii")).digest()).decode("ascii")
        activities = CONVERSATIONS.setdefault(conversation_id, initial_activities())
        self.send_response(101, "Switching Protocols")
        self.send_header("Upgrade", "websocket")
        self.send_header("Connection", "Upgrade")
        self.send_header("Sec-WebSocket-Accept", accept)
        self.end_headers()
        self.wfile.write(websocket_frame({"activities": activities, "watermark": str(len(activities))}))
        self.wfile.flush()
        time.sleep(1)


if __name__ == "__main__":
    port = int(os.environ["WEBCHAT_TEST_PORT"])
    httpd = ThreadingHTTPServer(("127.0.0.1", port), Handler)
    print(f"Serving webchat-gui test harness on http://127.0.0.1:{port}/test.html", flush=True)
    httpd.serve_forever()
PY

if [ "${EMBEDDED}" -eq 1 ]; then
  URL="http://127.0.0.1:${PORT}/test.html"
elif [ "${LOGIN_REQUIRED}" -eq 1 ]; then
  URL="http://127.0.0.1:${PORT}/login-required.html"
else
  URL="http://127.0.0.1:${PORT}/v1/web/webchat/${SKIN}/?tenant=${SKIN}"
fi

WEBCHAT_TEST_ROOT="${WWW_DIR}" WEBCHAT_TEST_PORT="${PORT}" WEBCHAT_TEST_SKIN="${SKIN}" python3 "${WORK_DIR}/server.py" &
SERVER_PID="$!"
trap 'kill "${SERVER_PID}" 2>/dev/null || true' EXIT

sleep 1
echo "webchat-gui version: ${VERSION}"
echo "skin: ${SKIN}"
if [ "${DEMO_LINKS}" -eq 1 ] || [ -n "${NAV_LINKS_JSON}" ] || [ "${#NAV_LINK_ARGS[@]}" -gt 0 ]; then
  echo "top-bar links: enabled"
fi
if [ "${EMBEDDED}" -eq 1 ]; then
  echo "mode: embedded"
  echo "text input: ${TEXT_INPUT_VALUE}"
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
