#!/usr/bin/env bash
set -euo pipefail

# v0.6.0: Packgen flow generation disabled - components now implement qa-spec/apply-answers directly
# Legacy flows (setup_default, update, remove, requirements) were removed in v0.6.0 refactor
#
# Previously this script ran greentic-messaging-packgen to generate:
# - setup_default.ygtc
# - update.ygtc
# - remove.ygtc
# - requirements.ygtc
#
# Now flows are manually maintained in packs/*/flows/ and only include:
# - default.ygtc (message routing)
# - diagnostics.ygtc
# - verify_webhooks.ygtc (for providers with webhooks)
# - sync_subscriptions.ygtc (for providers with subscriptions)
# - rotate_credentials.ygtc (for OAuth providers)

echo "Skipping flow generation - v0.6.0 uses component-based QA contract"
echo "Flows are now manually maintained in packs/*/flows/"
exit 0
