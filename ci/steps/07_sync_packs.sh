#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "${ROOT_DIR}"

if [ -f "${ROOT_DIR}/.env" ]; then
  set -a
  # shellcheck disable=SC1091
  source "${ROOT_DIR}/.env"
  set +a
fi

# Keep authentication identity (GHCR_USERNAME) decoupled from namespace
# selection. Namespace resolution is handled in tools/sync_packs.sh via
# TEMPLATES_NAMESPACE/GHCR_NAMESPACE/OCI_ORG.

# PACK_VERSION is an explicit override. Left unset, each pack resolves its own
# version from ci/provider-matrix.json - never the workspace version.
# See docs/release-policy.md and tools/resolve_pack_version.py.
PACK_VERSION="${PACK_VERSION:-}"
export PACK_VERSION

"${ROOT_DIR}/ci/lib/stage_local_components.sh"

./tools/sync_packs.sh
