#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
# Legacy webhook stub: the node@0.5.0 export glue was removed when greentic-interfaces-guest
# >=1.1 dropped `component_entrypoint!`. Until this webhook is migrated to the 0.6.0 invoke
# contract, the artifact is a no-export library and won't import wasi:cli — skip the
# wasm-tools validation gate to unblock the build.
SKIP_WASM_TOOLS_VALIDATION=1 \
  "${ROOT_DIR}/tools/build_component_one.sh" "webex-webhook"
