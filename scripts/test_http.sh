#!/usr/bin/env bash
# Run the answer-owned HTTP provider check.
#
# Usage:
#   scripts/test_http.sh

set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "${ROOT_DIR}"

if [ $# -gt 0 ]; then
  case "$1" in
    -h|--help)
      sed -n '2,5p' "$0" >&2
      exit 0
      ;;
    *)
      echo "unexpected argument: $1" >&2
      exit 2
      ;;
  esac
fi

scripts/check_answer_provider.sh messaging-http
