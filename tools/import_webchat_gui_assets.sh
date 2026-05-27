#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
SRC_DIR="${GREENTIC_WEBCHAT_SITE_DIR:-/projects/ai/greentic-ng/greentic-webchat/site/app}"
DEST_DIR="${ROOT_DIR}/packs/messaging-webchat-gui/assets/webchat-gui"

if [ ! -d "${SRC_DIR}" ]; then
  echo "Skipping WebChat GUI asset import; source not found: ${SRC_DIR}" >&2
  exit 0
fi

mkdir -p "${DEST_DIR}" "${DEST_DIR}/config" "${DEST_DIR}/i18n" "${DEST_DIR}/js" "${DEST_DIR}/skins"

# Import SPA build artifacts (JS/CSS bundles, config, i18n, js)
rsync -a --delete "${SRC_DIR}/assets/" "${DEST_DIR}/assets/"
rsync -a --delete "${SRC_DIR}/config/" "${DEST_DIR}/config/"
rsync -a --delete "${SRC_DIR}/i18n/" "${DEST_DIR}/i18n/"
rsync -a --delete "${SRC_DIR}/js/" "${DEST_DIR}/js/"

# Import skins from SPA without --delete to preserve pack-specific customizations.
rsync -a "${SRC_DIR}/skins/" "${DEST_DIR}/skins/"

# The provider pack intentionally exposes only the production Greentic default
# skin and the 3AIgent skin. The upstream SPA still carries template/demo skins
# for development; prune them after import so they cannot be selected by URL.
find "${DEST_DIR}/skins" -mindepth 1 -maxdepth 1 -type d \
  ! -name default \
  ! -name 3aigent \
  -exec rm -rf {} +

# Keep only tenant configs that are valid entry points for this pack.
find "${DEST_DIR}/config/tenants" -mindepth 1 -maxdepth 1 -type f \
  ! -name greentic.json \
  ! -name default.json \
  -exec rm -f {} +

python3 - "${DEST_DIR}/config/tenants/greentic.json" <<'PY'
from __future__ import annotations

import json
import sys
from pathlib import Path

path = Path(sys.argv[1])
if not path.exists():
    raise SystemExit(0)
data = json.loads(path.read_text(encoding="utf-8"))
data["skin"] = "default"
data["legacy_skin"] = "default"
branding = data.setdefault("branding", {})
if isinstance(branding, dict):
    branding["logo"] = "/skins/default/assets/logo.svg"
path.write_text(json.dumps(data, indent=2, ensure_ascii=False) + "\n", encoding="utf-8")
PY

js_bundle="$(basename "$(find "${DEST_DIR}/assets" -maxdepth 1 -type f -name 'index-*.js' | sort | head -n 1)")"
css_bundle="$(basename "$(find "${DEST_DIR}/assets" -maxdepth 1 -type f -name 'index-*.css' | sort | head -n 1)")"

if [ -z "${js_bundle}" ] || [ -z "${css_bundle}" ]; then
  echo "Unable to locate greentic-webchat app bundles in ${DEST_DIR}/assets" >&2
  exit 1
fi

# Generate index.html with correct bundle filenames.
# runtime-bootstrap.js and embed.js are maintained in-repo and NOT overwritten here.
cat > "${DEST_DIR}/index.html" <<EOF
<!doctype html>
<html lang="en">
  <head>
    <meta charset="UTF-8" />
    <meta name="viewport" content="width=device-width, initial-scale=1.0" />
    <title>Greentic WebChat</title>
    <script src="./runtime-bootstrap.js"></script>
    <script type="module" crossorigin src="./assets/${js_bundle}"></script>
    <link rel="stylesheet" crossorigin href="./assets/${css_bundle}">
  </head>
  <body>
    <div id="root"></div>
  </body>
</html>
EOF

cp "${DEST_DIR}/index.html" "${DEST_DIR}/404.html"
