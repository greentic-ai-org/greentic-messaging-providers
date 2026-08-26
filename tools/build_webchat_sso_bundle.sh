#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
OUT="${ROOT_DIR}/packs/messaging-webchat-gui/assets/webchat-gui/greentic-sso.js"

npx --no-install esbuild "${ROOT_DIR}/tools/webchat-sso/entry.js" \
  --bundle \
  --format=iife \
  --global-name=GreenticSso \
  --target=es2017 \
  --legal-comments=none \
  --outfile="${OUT}"

echo "built ${OUT}"
