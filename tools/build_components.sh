#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
# TODO(0.5): telegram-webhook and webex-webhook excluded pending rewrite to new
# greentic:component@0.6.0 node ABI (describe + CBOR invoke). The 0.4 `component_entrypoint!`
# macro + get-manifest/on-start/on-stop/invoke(ctx, op, json) shape was removed in 0.5.
PACKAGES=("questions" "secrets-probe" "slack" "teams" "telegram" "webchat" "webex" "whatsapp" "messaging-ingress-slack" "messaging-ingress-teams" "messaging-ingress-telegram" "messaging-ingress-whatsapp" "messaging-provider-dummy" "messaging-provider-telegram" "messaging-provider-teams" "messaging-provider-email" "messaging-provider-slack" "messaging-provider-webex" "messaging-provider-whatsapp" "messaging-provider-webchat" "messaging-provider-webchat-gui")

bash "${ROOT_DIR}/tools/sync_wit_deps_from_greentic_interfaces.sh"

for package in "${PACKAGES[@]}"; do
  bash "${ROOT_DIR}/tools/build_components/${package}.sh"
done

# Note: do not delete nested target triples here. This script is invoked from
# multiple test binaries in parallel, and deleting shared target directories can
# race with active builds (leading to missing .fingerprint files).
