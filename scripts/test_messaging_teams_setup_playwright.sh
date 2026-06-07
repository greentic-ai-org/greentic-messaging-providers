#!/usr/bin/env bash
# Run the deterministic Teams setup web-component E2E test against a fake backend.

set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "${ROOT_DIR}"

EXTRA_ARGS=()

while [ $# -gt 0 ]; do
  case "$1" in
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

if [ "${#EXTRA_ARGS[@]}" -gt 0 ]; then
  npm run test:messaging-teams-setup -- "${EXTRA_ARGS[@]}"
else
  npm run test:messaging-teams-setup
fi
