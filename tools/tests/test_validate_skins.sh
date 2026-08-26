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

set +e
output="$(python3 tools/validate_skins.py 2>&1)"
status=$?
set -e

if [ "${status}" -eq 0 ]; then
  echo "FAIL: validator accepted a skin missing brand/webchat/fullpage" >&2
  exit 1
fi

if ! printf '%s' "${output}" | grep -q "is a required property"; then
  echo "FAIL: validator exited non-zero but not for the expected reason:" >&2
  printf '%s\n' "${output}" >&2
  exit 1
fi

echo "PASS: validator rejects an incomplete skin for the right reason"
