#!/usr/bin/env bash
# Demo + e2e for "start default bundle on entry".
#
# Proves that when a user enters a Slack app (app_home_opened), a Webex bot
# space (membership created), or a Teams app (conversationUpdate / installation),
# the provider emits a `channel.user.entered` envelope with `autoStart=true`.
# The operator (greentic-start) routes any such envelope to the app's `default`
# flow (select_app_flow -> route_messaging_envelopes), so this signal is what
# starts the default bundle on entry.
#
# Usage: scripts/test_default_on_entry.sh

set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "${ROOT_DIR}"
export PATH="${HOME}/.cargo/bin:${PATH}"

ENTRY_DIR="${ROOT_DIR}/e2e/entry"
FAIL=0

assert_autostart() {
  local provider="$1"
  echo "── ${provider}: entry event → user_entered envelope ─────────────"
  local tmp
  tmp="$(mktemp)"
  cargo run -q -p greentic-messaging-tester -- ingress \
    --provider "${provider}" \
    --values "${ENTRY_DIR}/${provider}.values.json" \
    --http-in "${ENTRY_DIR}/${provider}.http_in.json" \
    --public-base-url https://example.com > "${tmp}" 2>/dev/null || true
  if ! python3 - "${tmp}" <<'PY'; then
import json, sys
data = json.load(open(sys.argv[1]))
envs = data.get("envelopes", [])
hit = [e for e in envs
       if e.get("metadata", {}).get("autoStart") == "true"
       and e.get("metadata", {}).get("event_type") == "channel.user.entered"]
if not hit:
    print("  FAIL: no autoStart user_entered envelope emitted")
    sys.exit(1)
m = hit[0]["metadata"]
print("  PASS autoStart=true event_type=channel.user.entered reason=" + str(m.get("reason")))
PY
    FAIL=1
  fi
  rm -f "${tmp}"
}

echo "============================================================"
echo " Default-bundle-on-entry: provider emission proof"
echo "============================================================"

assert_autostart slack
assert_autostart webex

echo "── teams: conversationUpdate / installationUpdate → user_entered ──"
if cargo test -q -p messaging-ingress-teams --lib bot_framework 2>/dev/null \
     | grep -q 'test result: ok'; then
  echo "  PASS teams bot_framework entry tests (members_added + app_installed)"
else
  echo "  FAIL teams bot_framework entry tests"
  FAIL=1
fi

echo "============================================================"
if [ "${FAIL}" -eq 0 ]; then
  echo " ALL PASS — slack/webex/teams emit autoStart on entry."
  echo " greentic-start routes these to the app 'default' flow."
else
  echo " FAILURES above."
fi
echo "============================================================"
exit "${FAIL}"
