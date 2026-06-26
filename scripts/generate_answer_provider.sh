#!/usr/bin/env bash

set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"

usage() {
  echo "usage: scripts/generate_answer_provider.sh <messaging-http|messaging-websocket>" >&2
}

provider="${1:-}"
if [ -z "${provider}" ]; then
  usage
  exit 2
fi

case "${provider}" in
  messaging-http|messaging-websocket)
    ;;
  *)
    echo "answer-owned provider generation currently supports only messaging-http and messaging-websocket" >&2
    exit 2
    ;;
esac

if ! command -v jq >/dev/null 2>&1; then
  echo "jq not found" >&2
  exit 127
fi

if ! command -v greentic-pack >/dev/null 2>&1; then
  echo "greentic-pack not found" >&2
  exit 127
fi

provider_dir="${ROOT_DIR}/generated-providers/${provider}"
answer_path="${provider_dir}/build-answer.json"

if [ ! -f "${answer_path}" ]; then
  echo "missing answer file: ${answer_path}" >&2
  exit 1
fi

answer_provider="$(jq -r '.provider_id // empty' "${answer_path}")"
answer_pack="$(jq -r '.pack_id // empty' "${answer_path}")"
generated_rel="$(jq -r '.source_layout.generated_pack_dir // empty' "${answer_path}")"

if [ "${answer_provider}" != "${provider}" ] || [ "${answer_pack}" != "${provider}" ]; then
  echo "answer file provider_id/pack_id must both equal ${provider}" >&2
  exit 1
fi

if [ -z "${generated_rel}" ] || [ "${generated_rel}" = "null" ]; then
  echo "missing source_layout.generated_pack_dir in ${answer_path}" >&2
  exit 1
fi

case "${generated_rel}" in
  target/generated/${provider}.pack)
    ;;
  *)
    echo "source_layout.generated_pack_dir must be target/generated/${provider}.pack" >&2
    exit 1
    ;;
esac

generated_dir="${ROOT_DIR}/${generated_rel}"
tmp_dir="${TMPDIR:-/tmp}/greentic-answer-provider-${provider}"

rm -rf "${tmp_dir}" "${generated_dir}"
mkdir -p "${tmp_dir}" "$(dirname "${generated_dir}")"

jq '.pack_create' "${answer_path}" > "${tmp_dir}/pack-create-answers.json"
(
  cd "${ROOT_DIR}"
  greentic-pack wizard apply --answers "${tmp_dir}/pack-create-answers.json" >/dev/null
)

python3 - "${provider_dir}" "${generated_dir}" <<'PY'
import json
import shutil
import sys
from pathlib import Path

provider_dir = Path(sys.argv[1])
generated_dir = Path(sys.argv[2])
answer = json.loads((provider_dir / "build-answer.json").read_text(encoding="utf-8"))

for top_level in ("src", "assets", "tests"):
    src = provider_dir / top_level
    if not src.exists():
        continue
    dest = generated_dir / top_level
    if dest.exists():
        shutil.rmtree(dest)
    shutil.copytree(src, dest)

pack_yaml = generated_dir / "pack.yaml"
if not pack_yaml.exists():
    raise SystemExit(f"generated pack.yaml missing: {pack_yaml}")

text = pack_yaml.read_text(encoding="utf-8")
pack_version = answer.get("pack_version")
if pack_version:
    text = text.replace("version: 0.1.0", f"version: {pack_version}", 1)
capability_ids = answer.get("capabilities") or []
if capability_ids:
    capabilities = {
        "schema_version": 1,
        "provider_id": answer["provider_id"],
        "capabilities": capability_ids,
    }
    capabilities_inline = json.dumps(capabilities, indent=6).replace("\n", "\n      ")
    if "\nextensions:\n" not in text:
        text = text.rstrip() + (
            "\nextensions:\n"
            "  greentic.answer-owned.capabilities.v1:\n"
            "    kind: greentic.answer-owned.capabilities.v1\n"
            f"    version: \"{answer.get('pack_version', '0.1.0')}\"\n"
            f"    inline: {capabilities_inline}\n"
        )
pack_yaml.write_text(text, encoding="utf-8")

metadata_path = generated_dir / "answer-owned-provider.json"
metadata = {
    "schema_id": "greentic.answer-owned-provider.v1",
    "provider_id": answer["provider_id"],
    "pack_id": answer["pack_id"],
    "pack_version": answer.get("pack_version", "0.1.0"),
    "source": str(provider_dir),
}
metadata_path.write_text(json.dumps(metadata, indent=2) + "\n", encoding="utf-8")
PY

echo "generated ${generated_dir}"
