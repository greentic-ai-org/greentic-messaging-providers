#!/usr/bin/env bash
# Build one provider pack locally.

set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "${ROOT_DIR}"

if [ $# -ne 1 ]; then
  echo "Usage: $0 <provider>" >&2
  exit 2
fi

scripts/build_providers.sh "$1"
