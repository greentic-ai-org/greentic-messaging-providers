#!/usr/bin/env bash

set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"

usage() {
  echo "usage: scripts/check_answer_provider.sh <messaging-http|messaging-websocket>" >&2
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
    echo "answer-owned provider checks currently support only messaging-http and messaging-websocket" >&2
    exit 2
    ;;
esac

provider_dir="${ROOT_DIR}/generated-providers/${provider}"
answer_path="${provider_dir}/build-answer.json"

if [ ! -f "${answer_path}" ]; then
  echo "missing answer file: ${answer_path}" >&2
  exit 1
fi

jq -e \
  --arg provider "${provider}" \
  '.provider_id == $provider and .pack_id == $provider and (.source_layout.generated_pack_dir == ("target/generated/" + $provider + ".pack"))' \
  "${answer_path}" >/dev/null

case "${provider}" in
  messaging-http)
    python3 "${provider_dir}/tests/test_http_provider.py"
    ;;
  messaging-websocket)
    python3 "${provider_dir}/tests/test_websocket_provider.py"
    ;;
esac

"${ROOT_DIR}/scripts/generate_answer_provider.sh" "${provider}" >/dev/null
"${ROOT_DIR}/scripts/package_answer_provider.sh" "${provider}" >/dev/null

artifact="${ROOT_DIR}/dist/packs/${provider}.gtpack"
if [ ! -f "${artifact}" ]; then
  echo "missing packaged artifact: ${artifact}" >&2
  exit 1
fi

echo "answer-owned provider check passed: ${provider}"
