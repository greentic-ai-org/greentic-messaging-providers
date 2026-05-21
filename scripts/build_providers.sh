#!/usr/bin/env bash
# Build provider component WASMs and provider packs locally.
#
# Requires bash 4+ (uses `mapfile` and `declare -A`). macOS ships bash 3.2,
# so install a modern bash first: `brew install bash`. The shebang above
# resolves `bash` via PATH, so Homebrew's bash is picked up automatically.
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

# Re-execute with bash if not already running in bash
if [ -z "${BASH_VERSION}" ]; then
  exec bash "$0" "$@"
fi

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

if [ $# -gt 1 ]; then
  echo "Usage: $0 [provider]" >&2
  exit 2
fi

# Use Python to handle all array operations (bash 3.2 compatible)
python3 - "${PROVIDER_FILTER}" <<'PYTHON_EOF'
import json
import subprocess
import sys
import os
from pathlib import Path

provider_filter = sys.argv[1]
root_dir = Path(".")

# Get providers list
if provider_filter:
    try:
        resolved = subprocess.run(
            ["python3", "ci/provider_matrix.py", "resolve-provider", provider_filter],
            capture_output=True, text=True, check=True
        )
        provider_name = json.loads(resolved.stdout)["provider"]
        providers = [provider_name]
    except Exception as e:
        print(f"Error resolving provider: {e}", file=sys.stderr)
        sys.exit(1)
else:
    matrix = json.loads((root_dir / "ci/provider-matrix.json").read_text())
    providers = sorted(matrix["providers"].keys())

built_components = set()

for provider in providers:
    resolved = subprocess.run(
        ["python3", "ci/provider_matrix.py", "resolve-provider", provider],
        capture_output=True, text=True, check=True
    )
    data = json.loads(resolved.stdout)
    pack = data["pack"]
    version = data["version"]
    components = data["components"]

    print(f"== provider: {provider} ==")
    print(f"  pack      : {pack}")
    print(f"  version   : {version}")
    print(f"  components: {' '.join(components)}")

    for component in components:
        if component in built_components:
            print(f"-- component already built: {component}")
            continue

        build_script = root_dir / "tools" / "build_components" / f"{component}.sh"
        if not build_script.exists():
            print(f"missing component build script: {build_script}", file=sys.stderr)
            sys.exit(1)

        print(f"-- build component: {component}")
        result = subprocess.run(["bash", str(build_script)], check=False)
        if result.returncode != 0:
            sys.exit(result.returncode)

        manifest_src = root_dir / "components" / component / "component.manifest.json"
        if manifest_src.exists():
            manifest_dst = root_dir / "target" / "components" / f"{component}.manifest.json"
            manifest_dst.parent.mkdir(parents=True, exist_ok=True)
            import shutil
            shutil.copy2(manifest_src, manifest_dst)

        built_components.add(component)

    # Stage pack inputs
    print(f"-- stage pack inputs: {pack}")
    stage_result = subprocess.run(
        ["python3", "-", pack] + components,
        input='''
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
''',
        text=True,
        check=False
    )
    if stage_result.returncode != 0:
        sys.exit(stage_result.returncode)

    # Build pack
    print(f"-- build pack: {pack}")
    env = dict(os.environ)
    if pack == "messaging-webchat-gui" and "GREENTIC_WEBCHAT_SITE_DIR" not in env:
        env["GREENTIC_WEBCHAT_SITE_DIR"] = str(root_dir / ".tmp" / "no-webchat-site-import")
    env["PACK_FILTER"] = pack
    env["PACK_VERSION"] = version

    import os
    result = subprocess.run(
        ["bash", "ci/steps/11_build_packs.sh"],
        env=env,
        check=False
    )
    if result.returncode != 0:
        sys.exit(result.returncode)
    print()

print("Built provider pack artifacts:")
for provider in providers:
    resolved = subprocess.run(
        ["python3", "ci/provider_matrix.py", "resolve-provider", provider],
        capture_output=True, text=True, check=True
    )
    pack = json.loads(resolved.stdout)["pack"]
    print(f"  dist/packs/{pack}.gtpack")
PYTHON_EOF
