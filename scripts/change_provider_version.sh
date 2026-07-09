#!/usr/bin/env bash
# Change one provider release version, validate metadata, and build that provider.
#
# Usage:
#   scripts/change_provider_version.sh [--no-build] <provider> <version>
#
# Examples:
#   scripts/change_provider_version.sh slack 1.1.1
#   scripts/change_provider_version.sh webchat-gui 1.1.1
#   scripts/change_provider_version.sh messaging-webchat-gui 1.1.1
#   scripts/change_provider_version.sh --no-build slack 1.1.1

set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "${ROOT_DIR}"

BUILD=1

while [ $# -gt 0 ]; do
  case "$1" in
    -h|--help)
      sed -n '2,11p' "$0" >&2
      exit 0
      ;;
    --no-build)
      BUILD=0
      shift
      ;;
    --build)
      BUILD=1
      shift
      ;;
    --)
      shift
      break
      ;;
    -*)
      echo "unknown argument: $1" >&2
      exit 2
      ;;
    *)
      break
      ;;
  esac
done

PROVIDER="${1:-}"
VERSION="${2:-}"

if [ -z "${PROVIDER}" ] || [ -z "${VERSION}" ] || [ $# -ne 2 ]; then
  echo "Usage: $0 [--no-build] <provider> <version>" >&2
  echo "Providers:" >&2
  python3 ci/provider_matrix.py list-providers --format text | awk '{print "  " $1}' >&2
  exit 2
fi

resolved_provider="$(
  python3 ci/provider_matrix.py resolve-provider "${PROVIDER}" |
    python3 -c 'import json,sys; print(json.load(sys.stdin)["provider"])'
)"

python3 tools/provider_versions.py set-provider "${resolved_provider}" "${VERSION}"
python3 tools/provider_versions.py validate --provider "${resolved_provider}"

echo "Changed provider version: ${resolved_provider} -> ${VERSION}"

# Keep Cargo.lock in sync with the bumped Cargo.toml versions. set-provider
# edits Cargo.toml only; without this the lock drifts and CI's `cargo --locked`
# clippy/build fails — this bites even with --no-build. --workspace limits the
# refresh to workspace members so registry deps aren't churned.
echo "Syncing Cargo.lock to the new version(s)..."
cargo update --workspace >/dev/null

if [ "${BUILD}" -eq 1 ]; then
  scripts/build_providers.sh "${resolved_provider}"
else
  echo "Skipped provider build (--no-build)."
fi
