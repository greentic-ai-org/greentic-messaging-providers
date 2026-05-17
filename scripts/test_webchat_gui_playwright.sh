#!/usr/bin/env bash
# Build the webchat-gui pack inputs and run the Playwright E2E suite.

set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "${ROOT_DIR}"

BUILD=1
EXTRA_ARGS=()

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

npm run test:webchat-gui -- "${EXTRA_ARGS[@]}"
