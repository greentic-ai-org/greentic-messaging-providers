#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "${ROOT_DIR}"

if ! command -v greentic-component >/dev/null 2>&1; then
  echo "greentic-component is required for component validation" >&2
  exit 1
fi
emit_manifests() {
  if [ -n "${COMPONENT_MANIFESTS_JSON:-}" ]; then
    python3 - <<'PY' "${COMPONENT_MANIFESTS_JSON}"
import json
import sys

for manifest in json.loads(sys.argv[1]):
    print(manifest)
PY
    return
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
  if compgen -G "packs/*/components/*.manifest.json" >/dev/null; then
    for c in packs/*/components/*.manifest.json; do
      pack_name="$(basename "$(dirname "$(dirname "${c}")")")"
      pack_selected "${pack_name}" || continue
      printf '%s\n' "${c}"
    done
  fi
}

while IFS= read -r c; do
  [ -n "${c}" ] || continue
    greentic-component doctor "$c"
done < <(emit_manifests)
