#!/usr/bin/env bash
# Start a local Microsoft Graph email tester UI.
#
# Usage:
#   scripts/test_ms_email.sh [--port <port>] [--no-build] [--no-open]

set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "${ROOT_DIR}"

PORT="${PORT:-8796}"
BUILD=1
OPEN_BROWSER=1

while [ $# -gt 0 ]; do
  case "$1" in
    -h|--help)
      sed -n '2,6p' "$0" >&2
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

if [ "${BUILD}" -eq 1 ]; then
  PACK_VERSION="$(python3 -c "import json; print(json.load(open('ci/provider-matrix.json'))['providers']['microsoft-email']['version'])")"
  bash tools/build_components/messaging-provider-email.sh
  PACK_FILTER=messaging-microsoft-email PACK_VERSION="${PACK_VERSION}" ./ci/steps/11_build_packs.sh
  cargo build -p greentic-messaging-tester
fi

TESTER_BIN="${ROOT_DIR}/target/debug/greentic-messaging-tester"
PROVIDER_WASM="${ROOT_DIR}/target/components/messaging-provider-email.wasm"
if [ ! -x "${TESTER_BIN}" ]; then
  echo "${TESTER_BIN} not found; run without --no-build first" >&2
  exit 1
fi
if [ ! -f "${PROVIDER_WASM}" ]; then
  echo "${PROVIDER_WASM} not found; run without --no-build first" >&2
  exit 1
fi

export GREENTIC_EMAIL_TEST_PROVIDER="microsoft-email"
export GREENTIC_EMAIL_TEST_TITLE="Microsoft Graph Email"
export GREENTIC_EMAIL_TEST_PORT="${PORT}"
export GREENTIC_EMAIL_TEST_OPEN="${OPEN_BROWSER}"
export GREENTIC_PROVIDER_WASM="${PROVIDER_WASM}"
export GREENTIC_ROOT="${ROOT_DIR}"
export GREENTIC_TESTER_BIN="${TESTER_BIN}"

python3 scripts/email_tester_ui.py
