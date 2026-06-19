#!/usr/bin/env bash
# Build the Microsoft email provider pack and run its provider E2E smoke.
#
# Usage:
#   scripts/test_ms_email.sh [--no-build]

set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "${ROOT_DIR}"

BUILD=1

while [ $# -gt 0 ]; do
  case "$1" in
    -h|--help)
      sed -n '2,5p' "$0" >&2
      exit 0
      ;;
    --no-build)
      BUILD=0
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
  scripts/build_providers.sh microsoft-email
fi

python3 ci/e2e/provider_e2e.py run \
  --provider microsoft-email \
  --gtpack dist/packs/messaging-microsoft-email.gtpack \
  --gtpack-source local-build \
  --result-json e2e-result-microsoft-email.json
