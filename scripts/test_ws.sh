#!/usr/bin/env bash
# Start a local WebSocket provider tester UI.
#
# Usage:
#   scripts/test_ws.sh [--port <port>] [--no-check] [--no-open]

set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "${ROOT_DIR}"

PORT="${PORT:-8798}"
RUN_CHECK=1
OPEN_BROWSER=1

while [ $# -gt 0 ]; do
  case "$1" in
    -h|--help)
      sed -n '2,6p' "$0" >&2
      exit 0
      ;;
    --no-check|--no-build)
      RUN_CHECK=0
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

if [ "${RUN_CHECK}" -eq 1 ]; then
  scripts/check_answer_provider.sh messaging-websocket
fi

export GREENTIC_ROOT="${ROOT_DIR}"
export GREENTIC_WS_TEST_PORT="${PORT}"
export GREENTIC_WS_TEST_OPEN="${OPEN_BROWSER}"

python3 scripts/websocket_tester_ui.py
