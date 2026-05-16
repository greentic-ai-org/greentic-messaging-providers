#!/usr/bin/env bash
# Publish a single provider fast-path end-to-end.
#
# Optionally updates the provider version, builds that provider locally, runs
# targeted local checks, then dispatches the `publish-provider.yml` workflow on
# the current branch and tails the run to completion.
#
# Usage:
#   scripts/publish_provider.sh <provider> [version] [--skip-build] [--skip-local-check] [--dry-run]
#
# Providers (from ci/provider-matrix.json):
#   dummy, email, slack, teams, telegram, webchat, webchat-gui, webex, whatsapp
#
# Requirements: gh CLI authenticated, Rust toolchain matching repo.

set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "${ROOT_DIR}"

PROVIDER="${1:-}"
VERSION=""
SKIP_BUILD=0
SKIP_LOCAL_CHECK=0
DRY_RUN=false
PUBLISH_LATEST=false

if [ "${PROVIDER:-}" = "-h" ] || [ "${PROVIDER:-}" = "--help" ]; then
  sed -n '2,14p' "$0" >&2
  exit 0
fi

shift 2>/dev/null || true
while [ $# -gt 0 ]; do
  case "$1" in
    --skip-build|--no-build) SKIP_BUILD=1 ;;
    --skip-local-check) SKIP_LOCAL_CHECK=1 ;;
    --dry-run) DRY_RUN=true ;;
    --publish-latest) PUBLISH_LATEST=true ;;
    -*)
      echo "unknown argument: $1" >&2
      exit 2
      ;;
    *)
      if [ -n "${VERSION}" ]; then
        echo "unexpected extra argument: $1" >&2
        exit 2
      fi
      VERSION="$1"
      ;;
  esac
  shift
done

if [ -z "${PROVIDER}" ]; then
  echo "Usage: $0 <provider> [version] [--skip-build] [--skip-local-check] [--dry-run] [--publish-latest]" >&2
  echo "Providers:" >&2
  python3 -c "
import json
with open('ci/provider-matrix.json') as h:
    providers = json.load(h).get('providers', {})
for name in providers:
    print(f'  {name}')
" >&2
  exit 2
fi

# Resolve the provider into pack + components + manifests via the shared
# matrix script. This fails fast if the name is unknown.
RESOLVED_JSON="$(python3 ci/provider_matrix.py resolve-provider "${PROVIDER}")"
RESOLVED_PROVIDER="$(echo "${RESOLVED_JSON}" | python3 -c 'import json,sys; print(json.load(sys.stdin)["provider"])')"
PACK="$(echo "${RESOLVED_JSON}" | python3 -c 'import json,sys; print(json.load(sys.stdin)["pack"])')"
COMPONENTS_CSV="$(echo "${RESOLVED_JSON}" | python3 -c 'import json,sys; print(",".join(json.load(sys.stdin)["components"]))')"
MANIFESTS_JSON="$(echo "${RESOLVED_JSON}" | python3 -c 'import json,sys; print(json.dumps(json.load(sys.stdin)["manifests"]))')"
CURRENT_VERSION="$(echo "${RESOLVED_JSON}" | python3 -c 'import json,sys; print(json.load(sys.stdin)["version"])')"

echo "== provider fast-path =="
echo "  provider   : ${RESOLVED_PROVIDER}"
echo "  pack       : ${PACK}"
echo "  version    : ${VERSION:-${CURRENT_VERSION}}"
echo "  components : ${COMPONENTS_CSV}"
echo "  build      : $([ "${SKIP_BUILD}" -eq 1 ] && echo skipped || echo enabled)"
echo "  dry_run    : ${DRY_RUN}"
echo

if [ -n "${VERSION}" ]; then
  if [ "${SKIP_BUILD}" -eq 1 ]; then
    scripts/change_provider_version.sh --no-build "${RESOLVED_PROVIDER}" "${VERSION}"
  else
    scripts/change_provider_version.sh "${RESOLVED_PROVIDER}" "${VERSION}"
  fi
  RESOLVED_JSON="$(python3 ci/provider_matrix.py resolve-provider "${RESOLVED_PROVIDER}")"
  MANIFESTS_JSON="$(echo "${RESOLVED_JSON}" | python3 -c 'import json,sys; print(json.dumps(json.load(sys.stdin)["manifests"]))')"
else
  python3 tools/provider_versions.py validate --provider "${RESOLVED_PROVIDER}"
  if [ "${SKIP_BUILD}" -eq 1 ]; then
    echo "-- provider build: SKIPPED (--skip-build) --"
  else
    echo "-- provider build --"
    scripts/build_providers.sh "${RESOLVED_PROVIDER}"
  fi
fi

if [ "${SKIP_LOCAL_CHECK}" -ne 1 ]; then
  # Run the same targeted fmt + clippy scripts CI uses, scoped to this
  # provider. Same manifests list the workflow consumes.
  echo "-- local check: fmt --"
  COMPONENT_MANIFESTS_JSON="${MANIFESTS_JSON}" ./ci/steps/01_fmt.sh

  echo "-- local check: clippy --"
  COMPONENT_MANIFESTS_JSON="${MANIFESTS_JSON}" ./ci/steps/02_clippy.sh

  # Native unit tests for the component crates. Per-crate, not workspace,
  # so a pure webchat push doesn't wait on whatsapp tests.
  echo "-- local check: cargo test (per component) --"
  IFS=',' read -ra COMPONENTS <<< "${COMPONENTS_CSV}"
  for component in "${COMPONENTS[@]}"; do
    echo ">> cargo test -p ${component} --lib"
    cargo test -p "${component}" --lib
  done
else
  echo "-- local check: SKIPPED (--skip-local-check) --"
fi

# Workflow dispatches need the branch name. Default to current branch;
# that matches how Maarten would use it from a feature branch or main.
BRANCH="$(git rev-parse --abbrev-ref HEAD)"

if ! git diff --quiet || ! git diff --cached --quiet; then
  echo "warning: working tree has uncommitted changes; GitHub Actions will use the pushed ${BRANCH} branch, not local-only edits." >&2
fi

echo
echo "-- dispatching publish-provider.yml --"
echo "  ref=${BRANCH} provider=${RESOLVED_PROVIDER} dry_run=${DRY_RUN} publish_latest=${PUBLISH_LATEST}"
gh workflow run publish-provider.yml \
  --ref "${BRANCH}" \
  -f "provider=${RESOLVED_PROVIDER}" \
  -f "dry_run=${DRY_RUN}" \
  -f "publish_latest=${PUBLISH_LATEST}"

# Poll for the run that was just created — `gh workflow run` doesn't print
# a run ID, so we grab the latest queued run for this workflow on this
# branch. A short sleep lets GitHub register the dispatch before we query.
sleep 5
RUN_ID="$(gh run list \
  --workflow publish-provider.yml \
  --branch "${BRANCH}" \
  --limit 1 \
  --json databaseId \
  --jq '.[0].databaseId')"

if [ -z "${RUN_ID}" ]; then
  echo "warning: could not locate the run just dispatched; check https://github.com/$(gh repo view --json nameWithOwner -q .nameWithOwner)/actions" >&2
  exit 0
fi

echo "-- watching run ${RUN_ID} --"
echo "  https://github.com/$(gh repo view --json nameWithOwner -q .nameWithOwner)/actions/runs/${RUN_ID}"
gh run watch "${RUN_ID}" --exit-status
