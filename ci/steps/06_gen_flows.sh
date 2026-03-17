#!/usr/bin/env bash
set -euo pipefail

# v0.6.0: Packgen flow generation disabled - components now implement qa-spec/apply-answers directly
# Legacy flows (setup_default, update, remove, requirements) were removed in v0.6.0 refactor

echo "Skipping flow generation - v0.6.0 uses component-based QA contract"
echo "Flows are now manually maintained in packs/*/flows/"
exit 0
