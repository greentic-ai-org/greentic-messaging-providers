#!/usr/bin/env bash
# Validate and build a provider pack from its checked-in build-answer.json.
#
# The first migration phase validates that build-answer.json is the durable
# source of pack intent. Existing build scripts still perform component and pack
# compilation; this helper is the stable entry point for answer-owned providers.

set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "${ROOT_DIR}"

if [ $# -ne 1 ]; then
  echo "Usage: $0 <provider>" >&2
  exit 2
fi

provider="$1"

python3 tools/provider_build_answers.py --check "${provider}"
scripts/build_providers.sh "${provider}"
