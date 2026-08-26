#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
SKINS_DIR="${ROOT_DIR}/packs/messaging-webchat-gui/assets/webchat-gui/skins"

NAME="${1:-}"
if [ -z "${NAME}" ]; then
  echo "Usage: $0 <skin-name>" >&2
  echo "Creates ${SKINS_DIR}/<skin-name>/ from the _template skin." >&2
  exit 2
fi

case "${NAME}" in
  _*|*/*|*' '*)
    echo "invalid skin name: ${NAME} (no leading underscore, slashes, or spaces)" >&2
    exit 2
    ;;
esac

DEST="${SKINS_DIR}/${NAME}"
if [ -e "${DEST}" ]; then
  echo "skin already exists: ${DEST}" >&2
  exit 1
fi

cp -r "${SKINS_DIR}/_template" "${DEST}"

python3 - "${DEST}/skin.json" "${NAME}" <<'PY'
import json
import sys
from pathlib import Path

path = Path(sys.argv[1])
name = sys.argv[2]
data = json.loads(path.read_text(encoding="utf-8"))
data["tenant"] = name
path.write_text(json.dumps(data, indent=2) + "\n", encoding="utf-8")
PY

echo "Created ${DEST}"

if git -C "${ROOT_DIR}" add -A "${DEST}" 2>/dev/null; then
  echo "Staged ${DEST} (git add -A) so the asset importer's prune allowlist covers it."
else
  echo "WARNING: could not 'git add -A ${DEST}' — stage it yourself before running" >&2
  echo "         tools/import_webchat_gui_assets.sh, or it will be deleted as untracked." >&2
fi

echo
echo "Next:"
echo "  1. Replace ${DEST}/assets/logo.svg and favicon.ico with your artwork."
echo "  2. Set brand.name and brand.primary in ${DEST}/skin.json."
echo "  3. Adjust ${DEST}/webchat/styleOptions.json and hostconfig.json."
echo "  4. Edit ${DEST}/fullpage/index.html — the skin name is not substituted there."
echo "  5. Run: python3 tools/validate_skins.py"
