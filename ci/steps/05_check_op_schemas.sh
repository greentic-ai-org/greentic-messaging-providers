#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "${ROOT_DIR}"

if [ -n "${COMPONENT_MANIFESTS_JSON:-}" ]; then
  python3 tools/check_op_schemas.py "${COMPONENT_MANIFESTS_JSON}"
else
  python3 tools/check_op_schemas.py
fi
