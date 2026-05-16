#!/usr/bin/env bash
# Build provider component WASMs and provider packs locally.
#
# Usage:
#   scripts/build_providers.sh [provider]
#
# Examples:
#   scripts/build_providers.sh
#   scripts/build_providers.sh webchat-gui
#   scripts/build_providers.sh messaging-webchat-gui
#
# The optional provider name is resolved through ci/provider_matrix.py, so it
# accepts the same names as scripts/publish_provider.sh.

set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "${ROOT_DIR}"

PROVIDER_FILTER="${1:-}"

if [ "${PROVIDER_FILTER:-}" = "-h" ] || [ "${PROVIDER_FILTER:-}" = "--help" ]; then
  sed -n '2,14p' "$0" >&2
  exit 0
fi

if [ $# -gt 1 ]; then
  echo "Usage: $0 [provider]" >&2
  exit 2
fi

if [ -n "${PROVIDER_FILTER}" ]; then
  mapfile -t PROVIDERS < <(python3 ci/provider_matrix.py resolve-provider "${PROVIDER_FILTER}" | python3 -c 'import json,sys; print(json.load(sys.stdin)["provider"])')
else
  mapfile -t PROVIDERS < <(python3 - <<'PY'
import json
from pathlib import Path

matrix = json.loads(Path("ci/provider-matrix.json").read_text())
for provider in sorted(matrix["providers"]):
    print(provider)
PY
)
fi

declare -A BUILT_COMPONENTS=()

for provider in "${PROVIDERS[@]}"; do
  resolved_json="$(python3 ci/provider_matrix.py resolve-provider "${provider}")"
  pack="$(printf '%s' "${resolved_json}" | python3 -c 'import json,sys; print(json.load(sys.stdin)["pack"])')"
  version="$(printf '%s' "${resolved_json}" | python3 -c 'import json,sys; print(json.load(sys.stdin)["version"])')"
  mapfile -t components < <(printf '%s' "${resolved_json}" | python3 -c 'import json,sys; print("\n".join(json.load(sys.stdin)["components"]))')

  echo "== provider: ${provider} =="
  echo "  pack      : ${pack}"
  echo "  version   : ${version}"
  echo "  components: ${components[*]}"

  for component in "${components[@]}"; do
    if [ -n "${BUILT_COMPONENTS[${component}]:-}" ]; then
      echo "-- component already built: ${component}"
      continue
    fi
    if [ ! -f "tools/build_components/${component}.sh" ]; then
      echo "missing component build script: tools/build_components/${component}.sh" >&2
      exit 1
    fi
    echo "-- build component: ${component}"
    bash "tools/build_components/${component}.sh"
    if [ -f "components/${component}/component.manifest.json" ]; then
      cp "components/${component}/component.manifest.json" "target/components/${component}.manifest.json"
    fi
    BUILT_COMPONENTS["${component}"]=1
  done

  echo "-- stage pack inputs: ${pack}"
  python3 - "${pack}" "${components[@]}" <<'PY'
import json
import shutil
import sys
from pathlib import Path

pack = sys.argv[1]
built = set(sys.argv[2:])
built_normalized = {name.replace("-", "_"): name for name in built}
root = Path(".")
pack_dir = root / "packs" / pack
manifest_path = pack_dir / "pack.manifest.json"
target_components = root / "target" / "components"
target_components.mkdir(parents=True, exist_ok=True)

manifest = json.loads(manifest_path.read_text(encoding="utf-8"))
component_sources = manifest.get("component_sources") or manifest.get("components") or []

for item in component_sources:
    if isinstance(item, str):
        comp_id = item
        wasm_rel = f"components/{item}.wasm"
        manifest_rel = ""
    else:
        comp_id = item.get("id") or ""
        wasm_rel = item.get("wasm") or f"components/{comp_id}.wasm"
        manifest_rel = item.get("manifest") or ""

    wasm_name = Path(wasm_rel).name
    target_name = f"{comp_id}.wasm" if wasm_name == "component.wasm" else wasm_name
    target_wasm = target_components / target_name

    source_component = None
    if comp_id in built:
        source_component = comp_id
    elif comp_id.replace("_", "-") in built:
        source_component = comp_id.replace("_", "-")
    elif Path(wasm_name).stem in built:
        source_component = Path(wasm_name).stem
    elif Path(wasm_name).stem.replace("_", "-") in built:
        source_component = Path(wasm_name).stem.replace("_", "-")

    if source_component:
        built_wasm = target_components / f"{source_component}.wasm"
        if built_wasm.exists() and built_wasm != target_wasm:
            shutil.copy2(built_wasm, target_wasm)
        built_manifest = root / "components" / source_component / "component.manifest.json"
        if built_manifest.exists():
            shutil.copy2(built_manifest, target_components / f"{comp_id}.manifest.json")
        continue

    pack_wasm = pack_dir / wasm_rel
    if not target_wasm.exists() and pack_wasm.exists():
        shutil.copy2(pack_wasm, target_wasm)

    if manifest_rel:
        pack_manifest = pack_dir / manifest_rel
        if pack_manifest.exists():
            shutil.copy2(pack_manifest, target_components / f"{comp_id}.manifest.json")

PY

  echo "-- build pack: ${pack}"
  if [ "${pack}" = "messaging-webchat-gui" ] && [ -z "${GREENTIC_WEBCHAT_SITE_DIR+x}" ]; then
    GREENTIC_WEBCHAT_SITE_DIR="${ROOT_DIR}/.tmp/no-webchat-site-import" PACK_FILTER="${pack}" PACK_VERSION="${version}" ./ci/steps/11_build_packs.sh
  else
    PACK_FILTER="${pack}" PACK_VERSION="${version}" ./ci/steps/11_build_packs.sh
  fi
  echo
done

echo "Built provider pack artifacts:"
for provider in "${PROVIDERS[@]}"; do
  pack="$(python3 ci/provider_matrix.py resolve-provider "${provider}" | python3 -c 'import json,sys; print(json.load(sys.stdin)["pack"])')"
  echo "  dist/packs/${pack}.gtpack"
done
