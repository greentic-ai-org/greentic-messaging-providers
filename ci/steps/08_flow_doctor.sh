#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "${ROOT_DIR}"

if ! command -v greentic-flow >/dev/null 2>&1; then
  echo "greentic-flow is required for flow validation" >&2
  exit 1
fi
pack_selected() {
  local pack_name="$1"
  if [ -z "${PACK_FILTER:-}" ]; then
    return 0
  fi
  python3 - <<'PY' "${PACK_FILTER}" "${pack_name}"
import sys

raw = sys.argv[1]
name = sys.argv[2]
items = [part.strip() for chunk in raw.split(",") for part in chunk.split() if part.strip()]
raise SystemExit(0 if name in items else 1)
PY
}

if compgen -G "packs/*/flows/*.ygtc" >/dev/null; then
  for f in packs/*/flows/*.ygtc; do
    pack_name="$(basename "$(dirname "$(dirname "${f}")")")"
    pack_selected "${pack_name}" || continue
    greentic-flow doctor "$f"
  done
fi
