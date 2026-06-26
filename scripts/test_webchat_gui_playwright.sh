#!/usr/bin/env bash
# Build the webchat-gui pack inputs and run the Playwright E2E suite,
# including the mocked sample Adaptive Card coverage.

set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "${ROOT_DIR}"

BUILD=1
EXTRA_ARGS=()

if [ -z "${WEBCHAT_GUI_TEST_PORT:-}" ]; then
  WEBCHAT_GUI_TEST_PORT="$((8700 + ($$ % 1000)))"
  export WEBCHAT_GUI_TEST_PORT
fi

while [ $# -gt 0 ]; do
  case "$1" in
    --no-build)
      BUILD=0
      ;;
    --headed)
      EXTRA_ARGS+=(--headed)
      ;;
    --update-snapshots)
      EXTRA_ARGS+=(--update-snapshots --project=chromium)
      ;;
    --)
      shift
      EXTRA_ARGS+=("$@")
      break
      ;;
    *)
      EXTRA_ARGS+=("$1")
      ;;
  esac
  shift
done

if [ "${BUILD}" -eq 1 ]; then
  scripts/build_providers.sh webchat-gui
fi

if [ "${#EXTRA_ARGS[@]}" -gt 0 ]; then
  npm run test:webchat-gui -- "${EXTRA_ARGS[@]}"
else
  npm run test:webchat-gui
fi
