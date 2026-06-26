#!/usr/bin/env bash

set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"

usage() {
  echo "usage: scripts/package_answer_provider.sh <messaging-http|messaging-websocket>" >&2
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

"${ROOT_DIR}/scripts/generate_answer_provider.sh" "${provider}" >/dev/null

answer_path="${ROOT_DIR}/generated-providers/${provider}/build-answer.json"
generated_rel="$(jq -r '.source_layout.generated_pack_dir // empty' "${answer_path}")"
generated_dir="${ROOT_DIR}/${generated_rel}"
dist_dir="${ROOT_DIR}/dist/packs"
artifact="${dist_dir}/${provider}.gtpack"

mkdir -p "${dist_dir}"

(
  cd "${generated_dir}"
  greentic-pack build --in . --allow-pack-schema >/dev/null
)

candidate=""
if [ -f "${generated_dir}/${provider}.gtpack" ]; then
  candidate="${generated_dir}/${provider}.gtpack"
elif [ -f "${generated_dir}/dist/${provider}.pack.gtpack" ]; then
  candidate="${generated_dir}/dist/${provider}.pack.gtpack"
elif [ -f "${generated_dir}/dist/${provider}.gtpack" ]; then
  candidate="${generated_dir}/dist/${provider}.gtpack"
elif [ -f "${ROOT_DIR}/.tmp/packs/${provider}.gtpack" ]; then
  candidate="${ROOT_DIR}/.tmp/packs/${provider}.gtpack"
else
  candidate="$(find "${ROOT_DIR}/.tmp/packs" -maxdepth 1 -type f -name "${provider}*.gtpack" 2>/dev/null | sort | tail -n 1 || true)"
fi

if [ -z "${candidate}" ] || [ ! -f "${candidate}" ]; then
  echo "greentic-pack did not produce a ${provider} gtpack" >&2
  exit 1
fi

cp "${candidate}" "${artifact}"
echo "packaged ${artifact}"
