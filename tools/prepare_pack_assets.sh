#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
PACKS_DIR="${ROOT_DIR}/packs"

pack_selected() {
  local pack_name="$1"
  local raw="${PACK_FILTER:-}"
  if [ -z "${raw}" ]; then
    return 0
  fi
  python3 - <<'PY' "${raw}" "${pack_name}"
import sys

raw = sys.argv[1]
name = sys.argv[2]
items = [part.strip() for chunk in raw.split(",") for part in chunk.split() if part.strip()]
raise SystemExit(0 if name in items else 1)
PY
}

if [ -x "${ROOT_DIR}/tools/import_webchat_gui_assets.sh" ] && pack_selected "messaging-webchat-gui"; then
  "${ROOT_DIR}/tools/import_webchat_gui_assets.sh"
fi

# messaging-webchat-ui is the SPA with no provider, for bundles that already have
# a webchat backend and no browser tier. It carries the SAME assets as
# messaging-webchat-gui, mirrored here rather than committed twice so the two
# cannot drift. Runs whenever the UI pack is selected — including when the GUI
# pack is not, in which case the source is the committed copy rather than a
# fresh upstream import.
if pack_selected "messaging-webchat-ui"; then
  gui_assets="${PACKS_DIR}/messaging-webchat-gui/assets/webchat-gui"
  ui_assets="${PACKS_DIR}/messaging-webchat-ui/assets/webchat-gui"
  if [ -d "${gui_assets}" ]; then
    mkdir -p "${ui_assets}"
    rsync -a --delete "${gui_assets}/" "${ui_assets}/"
  else
    echo "messaging-webchat-ui: no SPA assets at ${gui_assets} to mirror" >&2
    exit 1
  fi
fi

# messaging-3aigent-gui is the WebChat GUI shipped as the 3AIgent product. It
# carries the SAME SPA as messaging-webchat-gui, mirrored here rather than
# committed twice so the two cannot drift. config/tenants/default.json is
# excluded: that file is owned by the 3aigent pack and carries its skin and
# OAuth provider declarations.
if pack_selected "messaging-3aigent-gui"; then
  gui_assets="${PACKS_DIR}/messaging-webchat-gui/assets/webchat-gui"
  aigent_assets="${PACKS_DIR}/messaging-3aigent-gui/assets/webchat-gui"
  if [ -d "${gui_assets}" ]; then
    mkdir -p "${aigent_assets}"
    rsync -a --delete \
      --exclude 'config/tenants/default.json' \
      "${gui_assets}/" "${aigent_assets}/"
  else
    echo "messaging-3aigent-gui: no SPA assets at ${gui_assets} to mirror" >&2
    exit 1
  fi
fi

for dir in "${PACKS_DIR}"/messaging-*; do
  [ -d "${dir}" ] || continue
  secrets_out="${dir}/.secret_requirements.json"
  python3 "${ROOT_DIR}/tools/generate_pack_metadata.py" \
    --pack-dir "${dir}" \
    --components-dir "${ROOT_DIR}/components" \
    --secrets-out "${secrets_out}" >/dev/null

  dest_root="${dir}/secret-requirements.json"
  rm -f "${dir}/assets/secret-requirements.json"
  if [ -f "${secrets_out}" ]; then
    cp "${secrets_out}" "${dest_root}"
  else
    printf '%s\n' "[]" > "${dest_root}"
  fi

  pack_yaml="${dir}/pack.yaml"
  if [ -f "${pack_yaml}" ]; then
    python3 - "${pack_yaml}" <<'PY'
from pathlib import Path
import sys

path = Path(sys.argv[1])
lines = path.read_text().splitlines()
asset_line = "- path: secret-requirements.json"
if any(line.strip() == asset_line for line in lines):
    raise SystemExit(0)

insert_at = None
for idx, line in enumerate(lines):
    if line.startswith("assets:"):
        insert_at = idx + 1
        if line.strip() == "assets: []":
            lines[idx] = "assets:"
        break

if insert_at is None:
    if lines and lines[-1].strip():
        lines.append("")
    lines.extend(["assets:", asset_line])
else:
    lines.insert(insert_at, asset_line)

path.write_text("\n".join(lines) + "\n")
PY
  fi
done
