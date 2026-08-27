#!/usr/bin/env bash
set -euo pipefail

# Regenerates pack manifests, syncs schemas, bumps versions, and stages WASM artifacts from target/components.

die() {
  echo "ERROR: $*" >&2
  exit 1
}

trap 'die "sync_packs failed."' ERR

if [ -z "${BASH_VERSION:-}" ]; then
  die "This script requires bash. Run: bash tools/sync_packs.sh"
fi


ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
PACKS_DIR="${PACKS_DIR:-${ROOT_DIR}/packs}"
TARGET_COMPONENTS="${TARGET_COMPONENTS:-${ROOT_DIR}/target/components}"
VERSION="${PACK_VERSION:-}"
PACK_FILTER="${PACK_FILTER:-}"

if [ -f "${ROOT_DIR}/.env" ]; then
  set -a
  # shellcheck disable=SC1091
  source "${ROOT_DIR}/.env"
  set +a
fi

# VERSION is an explicit override only. When unset, each pack resolves its own
# version from ci/provider-matrix.json. The workspace version is never a release
# version - docs/release-policy.md.
if [ -n "${VERSION}" ]; then
  echo "Using version override: ${VERSION}"
else
  echo "Resolving each pack version from ci/provider-matrix.json"
fi

pack_selected() {
  local pack_name="$1"
  if [ -z "${PACK_FILTER}" ]; then
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

command -v jq >/dev/null 2>&1 || { echo "jq is required" >&2; exit 1; }
command -v python3 >/dev/null 2>&1 || { echo "python3 is required" >&2; exit 1; }

if [ -x "${ROOT_DIR}/tools/prepare_pack_assets.sh" ]; then
  "${ROOT_DIR}/tools/prepare_pack_assets.sh"
fi

# Default OCI location for the shared templates component used by many packs.
TEMPLATES_REGISTRY="${TEMPLATES_REGISTRY:-${OCI_REGISTRY:-ghcr.io}}"
TEMPLATES_NAMESPACE="${TEMPLATES_NAMESPACE:-${GHCR_NAMESPACE:-${OCI_ORG:-greenticai}}}"
DEFAULT_TEMPLATES_IMAGE="${TEMPLATES_IMAGE:-${TEMPLATES_REGISTRY}/${TEMPLATES_NAMESPACE}/components/templates:latest}"
DEFAULT_TEMPLATES_DIGEST=""
DEFAULT_TEMPLATES_ARTIFACT="component_templates.wasm"
DEFAULT_TEMPLATES_MANIFEST="component.publish.manifest.json"
ALLOW_REMOTE_COMPONENT_FETCH="${ALLOW_REMOTE_COMPONENT_FETCH:-1}"
echo "Using templates image: ${DEFAULT_TEMPLATES_IMAGE}"

if [ ! -d "${TARGET_COMPONENTS}" ]; then
  echo "Building components..."
  "${ROOT_DIR}/tools/build_components.sh"
fi

assert_no_pack_downgrade() {
  local dir="$1"
  local next="$2"
  local current
  current="$(python3 "${ROOT_DIR}/tools/resolve_pack_version.py" "${dir}" --root "${ROOT_DIR}" --source pack-yaml 2>/dev/null || true)"
  [ -n "${current}" ] || return 0
  [ "${current}" != "${next}" ] || return 0
  if [ "${ALLOW_PACK_DOWNGRADE:-0}" = "1" ]; then
    return 0
  fi
  # `if !` keeps set -e from killing the script before the message is printed.
  if ! python3 - "${current}" "${next}" <<'PYVER'
import sys

def parts(v):
    return [int(x) if x.isdigit() else x for x in v.replace("-", ".").split(".")]

current, nxt = sys.argv[1], sys.argv[2]
try:
    lower = parts(nxt) < parts(current)
except TypeError:
    lower = False
sys.exit(1 if lower else 0)
PYVER
  then
    echo "Refusing to stamp $(basename "${dir}") down from ${current} to ${next}." >&2
    echo "Set PACK_VERSION explicitly, or ALLOW_PACK_DOWNGRADE=1 if this is intended." >&2
    exit 1
  fi
}

update_pack_yaml_version() {
  local yaml_path="$1"
  [ -f "${yaml_path}" ] || return 0
  python3 - "$yaml_path" "$resolved_version" <<'PY'
from pathlib import Path
import sys

path = Path(sys.argv[1])
version = sys.argv[2]
lines = path.read_text().splitlines()

# Detect current root version so we can replace all matching occurrences.
old_version = None
for line in lines:
    stripped = line.lstrip()
    indent = len(line) - len(stripped)
    if indent == 0 and stripped.startswith("version:"):
        old_version = stripped.split(":", 1)[1].strip().strip("'\"")
        break

out = []
updated = False
for line in lines:
    stripped = line.lstrip()
    # Replace all version: fields that match the old pack version.
    if stripped.startswith("version:"):
        current = stripped.split(":", 1)[1].strip().strip("'\"")
        if current == old_version or current == version:
            prefix = line.split("version:")[0] + "version: "
            out.append(f"{prefix}{version}")
            updated = True
            continue
    out.append(line.replace("__PACK_VERSION__", version))

if not updated:
    out.append(f"version: {version}")
path.write_text("\n".join(out) + "\n")
PY
}

ensure_helper_components_in_pack_yaml() {
  # Legacy helper injection removed in v0.6.0 - components now implement qa-spec/apply-answers directly
  return 0
}

stamp_manifest_version() {
  local manifest_path="$1"
  local pack_yaml_path="${2:-}"
  local comp_id="${3:-}"
  [ -f "${manifest_path}" ] || return 0
  python3 - "$manifest_path" "$resolved_version" "$pack_yaml_path" "$comp_id" <<'PY'
from pathlib import Path
import json
import sys
import yaml

manifest_path = Path(sys.argv[1])
version = sys.argv[2]
pack_yaml_path = sys.argv[3] if len(sys.argv) > 3 else ""
comp_id = sys.argv[4] if len(sys.argv) > 4 else ""

data = json.loads(manifest_path.read_text())
data["version"] = version

# Extract component config from pack.yaml if available
if pack_yaml_path and comp_id and Path(pack_yaml_path).exists():
    pack_data = yaml.safe_load(Path(pack_yaml_path).read_text())
    for comp in pack_data.get("components", []):
        if comp.get("id") == comp_id:
            if "world" in comp:
                data["world"] = comp["world"]
            if "profiles" in comp:
                data["profiles"] = comp["profiles"]
            break

manifest_path.write_text(json.dumps(data, indent=2) + "\n")
PY
}

sync_pack_yaml_component_versions() {
  local pack_dir="$1"
  local yaml_path="${pack_dir}/pack.yaml"
  [ -f "${yaml_path}" ] || return 0
  python3 - "$pack_dir" "$yaml_path" <<'PY'
from pathlib import Path
import json
import sys
import yaml

pack_dir = Path(sys.argv[1])
yaml_path = Path(sys.argv[2])

manifest_versions = {}
for manifest_path in sorted((pack_dir / "components").rglob("component.manifest.json")):
    try:
        data = json.loads(manifest_path.read_text())
    except Exception:
        continue
    comp_id = data.get("id")
    version = data.get("version")
    if comp_id and version:
        manifest_versions[comp_id] = version

if not manifest_versions:
    raise SystemExit(0)

data = yaml.safe_load(yaml_path.read_text()) or {}
components = data.get("components")
if not isinstance(components, list):
    raise SystemExit(0)

updated = False
for component in components:
    if not isinstance(component, dict):
        continue
    comp_id = component.get("id")
    manifest_version = manifest_versions.get(comp_id)
    if manifest_version and component.get("version") != manifest_version:
        component["version"] = manifest_version
        updated = True

if updated:
    yaml_path.write_text(yaml.safe_dump(data, sort_keys=False))
PY
}

copy_schema() {
  local pack_dir="$1"
  local schema_path="$2"
  local src="${ROOT_DIR}/${schema_path}"
  local dest="${pack_dir}/${schema_path}"
  if [ -f "${src}" ]; then
    mkdir -p "$(dirname "${dest}")"
    cp "${src}" "${dest}"
  else
    echo "Warning: schema not found at ${src}" >&2
  fi
}

ensure_secret_requirements_asset() {
  local pack_dir="$1"
  local secrets_out="$2"
  local dest_root="${pack_dir}/secret-requirements.json"
  rm -f "${pack_dir}/assets/secret-requirements.json"
  if [ -f "${secrets_out}" ]; then
    cp "${secrets_out}" "${dest_root}"
  else
    printf '%s\n' "[]" > "${dest_root}"
  fi
}

ensure_secret_requirements_asset_entry() {
  local pack_dir="$1"
  local yaml_path="${pack_dir}/pack.yaml"
  [ -f "${yaml_path}" ] || return 0
  python3 - "${yaml_path}" <<'PY'
from pathlib import Path
import sys

path = Path(sys.argv[1])
lines = path.read_text().splitlines()
asset_line = "- path: secret-requirements.json"
if any(line.strip() == asset_line for line in lines):
    raise SystemExit(0)

insert_at = None
for idx, line in enumerate(lines):
    if line.startswith("assets:"):
        insert_at = idx + 1
        if line.strip() == "assets: []":
            lines[idx] = "assets:"
        break

if insert_at is None:
    if lines and lines[-1].strip():
        lines.append("")
    lines.extend(["assets:", asset_line])
else:
    lines.insert(insert_at, asset_line)

path.write_text("\n".join(lines) + "\n")
PY
}

fetch_oci_component() {
  local image="$1"
  local digest="$2"
  local artifact="$3"
  local dest_wasm="$4"
  local manifest_name="$5"
  local dest_manifest="$6"

  if [ "${ALLOW_REMOTE_COMPONENT_FETCH}" != "1" ]; then
    echo "Remote component fetch disabled (ALLOW_REMOTE_COMPONENT_FETCH=${ALLOW_REMOTE_COMPONENT_FETCH}) for ${image}" >&2
    exit 1
  fi

  local ref="${image}"
  if [ -n "${digest}" ]; then
    ref="${image}@${digest}"
  fi

  local tmpdir
  tmpdir="$(mktemp -d)"
  echo "Fetching OCI component ${ref}..."
  oras_pull "${ref}" "${tmpdir}"
  local src_path="${tmpdir}/${artifact}"
  if [ ! -f "${src_path}" ]; then
    echo "OCI component artifact ${artifact} not found in ${tmpdir}" >&2
    rm -rf "${tmpdir}"
    exit 1
  fi
  mkdir -p "$(dirname "${dest_wasm}")"
  cp "${src_path}" "${dest_wasm}"

  if [ -n "${manifest_name:-}" ] && [ -n "${dest_manifest:-}" ]; then
    local manifest_src="${tmpdir}/${manifest_name}"
    if [ -f "${manifest_src}" ]; then
      mkdir -p "$(dirname "${dest_manifest}")"
      cp "${manifest_src}" "${dest_manifest}"
    fi
  fi
  rm -rf "${tmpdir}"
}

OCI_CACHE_KEYS=()
OCI_CACHE_DIRS=()
OCI_CACHE_TMPDIRS=()

oci_cache_get() {
  local key="$1"
  local idx=0
  for existing in "${OCI_CACHE_KEYS[@]:-}"; do
    if [ "${existing}" = "${key}" ]; then
      echo "${OCI_CACHE_DIRS[$idx]}"
      return 0
    fi
    idx=$((idx + 1))
  done
  return 1
}

oci_cache_set() {
  local key="$1"
  local value="$2"
  OCI_CACHE_KEYS+=("${key}")
  OCI_CACHE_DIRS+=("${value}")
}

cleanup_oci_cache() {
  for dir in "${OCI_CACHE_TMPDIRS[@]:-}"; do
    rm -rf "${dir}"
  done
}

trap cleanup_oci_cache EXIT

fetch_locked_component() {
  local ref="$1"
  local digest="$2"
  local dest_wasm="$3"

  if [[ "${ref}" == file://* ]]; then
    local src_path="${ref#file://}"
    if [ ! -f "${src_path}" ]; then
      echo "Local component file not found for ${ref}" >&2
      exit 1
    fi
    mkdir -p "$(dirname "${dest_wasm}")"
    cp "${src_path}" "${dest_wasm}"
    return
  fi

  if [ "${ALLOW_REMOTE_COMPONENT_FETCH}" != "1" ]; then
    echo "Remote locked component fetch disabled (ALLOW_REMOTE_COMPONENT_FETCH=${ALLOW_REMOTE_COMPONENT_FETCH}) for ${ref}" >&2
    exit 1
  fi

  local ref_clean="${ref#oci://}"
  local cache_key="${digest:-${ref_clean}}"
  local tmpdir=""
  tmpdir="$(oci_cache_get "${cache_key}")" || tmpdir=""

  if [ -z "${tmpdir}" ]; then
    tmpdir="$(mktemp -d)"
    oci_cache_set "${cache_key}" "${tmpdir}"
    OCI_CACHE_TMPDIRS+=("${tmpdir}")
    echo "Fetching OCI component ${ref_clean}..."
    oras_pull "${ref_clean}" "${tmpdir}"
  fi

  local manifest="${tmpdir}/component.publish.manifest.json"
  local artifact=""
  if [ -f "${manifest}" ]; then
    artifact="$(jq -r '.artifacts.component_wasm // empty' "${manifest}")"
  fi
  if [ -z "${artifact}" ]; then
    artifact="$(ls "${tmpdir}"/*.wasm 2>/dev/null | head -n 1)"
    artifact="${artifact##*/}"
  fi
  if [ -z "${artifact}" ] || [ ! -f "${tmpdir}/${artifact}" ]; then
    echo "OCI component artifact not found for ${ref_clean}" >&2
    exit 1
  fi
  mkdir -p "$(dirname "${dest_wasm}")"
  cp "${tmpdir}/${artifact}" "${dest_wasm}"
}

oras_pull() {
  local ref="$1"
  local out_dir="$2"

  command -v oras >/dev/null 2>&1 || { echo "oras is required for fetching OCI components" >&2; exit 1; }

  if [ -n "${GHCR_TOKEN:-}" ]; then
    local ghcr_user="${GHCR_USERNAME:-${GHCR_USER:-${USER:-}}}"
    if [ -z "${ghcr_user}" ]; then
      die "GHCR_TOKEN is set but no username found. Set GHCR_USERNAME."
    fi
    printf '%s' "${GHCR_TOKEN}" | oras pull --output "${out_dir}" --username "${ghcr_user}" --password-stdin "${ref}"
  else
    oras pull --output "${out_dir}" "${ref}"
  fi
}

for dir in "${PACKS_DIR}"/*; do
  [ -d "${dir}" ] || continue
  pack_name="$(basename "${dir}")"
  if ! pack_selected "${pack_name}"; then
    echo "Skipping filtered-out pack ${pack_name}"
    continue
  fi
  if [ ! -f "${dir}/pack.manifest.json" ]; then
    echo "Skipping ${dir}: no pack.manifest.json"
    continue
  fi

  if ! resolved_version="$(python3 "${ROOT_DIR}/tools/resolve_pack_version.py" "${dir}" --root "${ROOT_DIR}" --override "${VERSION}")"; then
    exit 1
  fi
  assert_no_pack_downgrade "${dir}" "${resolved_version}"

  echo "Syncing ${pack_name} at ${resolved_version}..."
  update_pack_yaml_version "${dir}/pack.yaml"
  python3 "${ROOT_DIR}/tools/normalize_pack_components.py" "${dir}/pack.yaml"
  ensure_helper_components_in_pack_yaml "${dir}/pack.yaml"
  python3 "${ROOT_DIR}/tools/generate_pack_metadata.py" \
    --pack-dir "${dir}" \
    --components-dir "${ROOT_DIR}/components" \
    --version "${resolved_version}" \
    --secrets-out "${dir}/.secret_requirements.json" \
    --include-capabilities-cache
  ensure_secret_requirements_asset "${dir}" "${dir}/.secret_requirements.json"
  ensure_secret_requirements_asset_entry "${dir}"

  mkdir -p "${dir}/components"
  rm -f "${dir}/components/component.manifest.json"
  rm -f "${dir}/components/provision.wasm" "${dir}/components/qa.wasm"
  while IFS=$'\t' read -r comp wasm_path oci_image oci_digest oci_artifact manifest_rel oci_manifest; do
    [ -z "${comp}" ] && continue
    wasm_rel="${wasm_path:-components/${comp}.wasm}"
    wasm_file="$(basename "${wasm_rel}")"
    # When wasm_rel uses a subdirectory path like components/<id>/component.wasm,
    # the basename is just "component.wasm" which collides across all packs.
    # Resolve to the component-specific artifact name instead.
    if [ "${wasm_file}" = "component.wasm" ]; then
      wasm_file="${comp}.wasm"
    fi
    is_templates_component=0
    if [ "${comp}" = "templates" ] || [ "${comp}" = "ai.greentic.component-templates" ] || [ "${wasm_file}" = "templates.wasm" ]; then
      is_templates_component=1
    fi
    src="${TARGET_COMPONENTS}/${wasm_file}"
    dest="${dir}/${wasm_rel}"
    manifest_src=""
    manifest_dest=""
    root_manifest_src=""
    if [ -n "${manifest_rel}" ]; then
      manifest_dest="${dir}/${manifest_rel}"
      # Use component-specific filename to avoid collisions when multiple components
      # have manifests with the same basename (e.g., component.manifest.json)
      manifest_src="${TARGET_COMPONENTS}/${comp}.manifest.json"
      # Also check the root component directory as the canonical source
      root_manifest_src="${ROOT_DIR}/components/${comp}/component.manifest.json"
    fi
    # Fill in default OCI metadata for template components when missing.
    if [ "${is_templates_component}" -eq 1 ] && [ -z "${oci_image}" ]; then
      oci_image="${DEFAULT_TEMPLATES_IMAGE}"
    fi
    if [ "${is_templates_component}" -eq 1 ] && [ -z "${oci_digest}" ]; then
      oci_digest="${DEFAULT_TEMPLATES_DIGEST}"
    fi
    if [ "${is_templates_component}" -eq 1 ] && [ -z "${oci_artifact}" ]; then
      oci_artifact="${DEFAULT_TEMPLATES_ARTIFACT}"
    fi
    if [ "${is_templates_component}" -eq 1 ] && [ -z "${oci_manifest}" ]; then
      oci_manifest="${DEFAULT_TEMPLATES_MANIFEST}"
    fi
    mkdir -p "$(dirname "${dest}")"
    if [ ! -f "${src}" ] || { [ -n "${manifest_rel}" ] && [ ! -f "${manifest_src}" ]; }; then
      # Prefer OCI fetch when metadata is available.
      if [ -n "${oci_image}" ] && [ -n "${oci_artifact}" ]; then
        fetch_oci_component "${oci_image}" "${oci_digest}" "${oci_artifact}" "${src}" "${oci_manifest}" "${manifest_src}"
      # Fallback: reuse component artifacts already bundled under the pack directory.
      elif [ -f "${dir}/${wasm_rel}" ]; then
        mkdir -p "$(dirname "${src}")"
        cp "${dir}/${wasm_rel}" "${src}"
        if [ -n "${manifest_rel}" ] && [ -f "${dir}/${manifest_rel}" ]; then
          mkdir -p "$(dirname "${manifest_src}")"
          cp "${dir}/${manifest_rel}" "${manifest_src}"
        fi
      else
        echo "Missing component artifact: ${src}" >&2
        exit 1
      fi
    fi
    cp "${src}" "${dest}"
    # Copy manifest to pack directory, preferring root component manifest as canonical source
    if [ -n "${manifest_rel}" ]; then
      mkdir -p "$(dirname "${manifest_dest}")"
      if [ -n "${root_manifest_src}" ] && [ -f "${root_manifest_src}" ]; then
        # Use root component manifest as canonical source
        cp "${root_manifest_src}" "${manifest_dest}"
      elif [ -f "${manifest_src}" ]; then
        # Fallback to TARGET_COMPONENTS cache
        cp "${manifest_src}" "${manifest_dest}"
      fi
      stamp_manifest_version "${manifest_dest}" "${dir}/pack.yaml" "${comp}"
    fi
  done < <(jq -r '(.component_sources // .components // [])[] | if type=="string" then {id: ., wasm: ("components/" + . + ".wasm")} else {id: .id, wasm: (.wasm // ("components/" + .id + ".wasm")), manifest: (.manifest // ""), oci: (.oci // {})} end | [.id, .wasm, (.oci.image // ""), (.oci.digest // ""), (.oci.artifact // ""), (.manifest // ""), (.oci.manifest // "")] | @tsv' "${dir}/pack.manifest.json")

  while IFS= read -r comp; do
    [ -z "${comp}" ] && continue
    stamp_manifest_version "${dir}/components/${comp}/component.manifest.json" "${dir}/pack.yaml" "${comp}"
  done < <(jq -r '(.component_sources // .components // [])[] | if type=="string" then . else (.id // "") end' "${dir}/pack.manifest.json")
  python3 "${ROOT_DIR}/tools/stamp_pack_component_manifests.py" "${dir}" "${resolved_version}"

  while IFS= read -r schema; do
    [ -z "${schema}" ] && continue
    copy_schema "${dir}" "${schema}"
  done < <(jq -r '
    [
      (.extensions["greentic.provider-extension.v1"].inline.providers[]?.config_schema_ref // empty),
      (.config_schema.provider_config.path // empty)
    ] | flatten | .[]? ' "${dir}/pack.manifest.json")

  lock_file="${dir}/pack.lock.json"
  if [ -f "${lock_file}" ]; then
    while IFS=$'\t' read -r name ref digest; do
      [ -z "${name}" ] && continue
      wasm_rel="components/${name}.wasm"
      dest="${dir}/${wasm_rel}"
      if [ ! -f "${dest}" ]; then
        if [[ "${ref}" == *"components/questions"* ]] && [ -f "${ROOT_DIR}/components/questions/questions.wasm" ]; then
          cp "${ROOT_DIR}/components/questions/questions.wasm" "${dest}"
        else
          fetch_locked_component "${ref}" "${digest}" "${dest}"
        fi
      fi
    done < <(jq -r '.components[]? | [.name, .ref, .digest] | @tsv' "${lock_file}")
  fi

  sync_pack_yaml_component_versions "${dir}"
  python3 "${ROOT_DIR}/tools/normalize_pack_components.py" "${dir}/pack.yaml"
done

echo "Pack sync complete."
