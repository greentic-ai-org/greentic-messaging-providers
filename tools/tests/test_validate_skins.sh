#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "${ROOT_DIR}"

SKINS="packs/messaging-webchat-gui/assets/webchat-gui/skins"
BROKEN="${SKINS}/_validator_probe"

cleanup() { rm -rf "${BROKEN}"; }
trap cleanup EXIT

mkdir -p "${BROKEN}"
printf '%s\n' '{"tenant":"_validator_probe","mode":"fullpage"}' > "${BROKEN}/skin.json"

if python3 tools/validate_skins.py >/dev/null 2>&1; then
  echo "FAIL: validator accepted a skin missing brand/webchat/fullpage" >&2
  exit 1
fi
echo "PASS: validator rejects an incomplete skin"
