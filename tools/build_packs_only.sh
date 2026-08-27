#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
# PACK_VERSION is an explicit override. Left unset, each pack resolves its own
# version from ci/provider-matrix.json - never the workspace version.
# See docs/release-policy.md and tools/resolve_pack_version.py.
PACK_VERSION="${PACK_VERSION:-}"
PACK_VERSION="${PACK_VERSION#v}"
PACKC_BUILD_FLAGS="${PACKC_BUILD_FLAGS:-}"
DRY_RUN="${DRY_RUN:-1}"
PACK_FILTER="${PACK_FILTER:-}"

cd "${ROOT_DIR}"
DRY_RUN="${DRY_RUN}" PACK_VERSION="${PACK_VERSION}" PACKC_BUILD_FLAGS="${PACKC_BUILD_FLAGS}" PACK_FILTER="${PACK_FILTER}" ./tools/publish_packs_oci.sh
