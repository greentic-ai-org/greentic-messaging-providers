#!/usr/bin/env bash
# Regression test for the release defect behind #390 / #391 / #392.
#
# `provider_versions.py set-provider` moved the version in the matrix, the pack
# files and the component Cargo.toml but not in Cargo.lock. The release PR was
# green (nothing on the PR path passed `--locked`), merged, and then
# auto-publish-on-version-bump.yml died in ci/steps/02_clippy.sh with
# "cannot update the lock file ... because --locked was passed". Nothing
# published, and the only tell was the run's ~30s duration.
#
# Two assertions:
#   1. set-provider moves Cargo.lock, and `validate` passes afterwards.
#   2. a planted Cargo.toml/Cargo.lock mismatch makes `validate` FAIL.
#
# Assertion 2 is the one that would have caught both incidents.

set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "${ROOT_DIR}"

# The `dummy` provider is the simplest in the matrix: one component manifest,
# one pack, and nothing else depends on its version.
PROVIDER="dummy"
CRATE="messaging-provider-dummy"
PROBE_VERSION="0.5.1-lockprobe.1"

TOUCHED=(
  "Cargo.lock"
  "ci/provider-matrix.json"
  "components/messaging-provider-dummy/Cargo.toml"
  "packs/messaging-dummy/pack.yaml"
  "packs/messaging-dummy/pack.manifest.json"
)

# This test mutates tracked files and restores them with `git checkout --`.
# Refuse to run over uncommitted work in those paths rather than destroying it.
if ! git diff --quiet -- "${TOUCHED[@]}" || ! git diff --cached --quiet -- "${TOUCHED[@]}"; then
  echo "SKIP: uncommitted changes in the paths this test rewrites:" >&2
  git status --short -- "${TOUCHED[@]}" >&2
  echo "Commit or revert them and re-run." >&2
  exit 2
fi

ORIGINAL_VERSION="$(python3 tools/provider_versions.py provider "${PROVIDER}")"

cleanup() { git checkout -- "${TOUCHED[@]}" 2>/dev/null || true; }
trap cleanup EXIT

fail() {
  echo "FAIL: $*" >&2
  exit 1
}

# ---------------------------------------------------------------------------
# 1. set-provider updates Cargo.lock, and validate passes afterwards.
# ---------------------------------------------------------------------------
python3 tools/provider_versions.py set-provider "${PROVIDER}" "${PROBE_VERSION}" >/dev/null

if git diff --quiet -- Cargo.lock; then
  fail "set-provider ${PROVIDER} ${PROBE_VERSION} left Cargo.lock untouched"
fi

if ! git diff -- Cargo.lock | grep -q "^+version = \"${PROBE_VERSION}\""; then
  fail "Cargo.lock changed but does not carry ${PROBE_VERSION}"
fi

if ! python3 tools/provider_versions.py validate --provider "${PROVIDER}" >/dev/null; then
  fail "validate rejected a tree that set-provider had just written"
fi

# The failure mode this whole change exists to prevent, asserted directly.
if command -v cargo >/dev/null 2>&1; then
  if ! cargo metadata --locked --format-version 1 >/dev/null 2>&1; then
    fail "tree does not satisfy 'cargo metadata --locked' after set-provider"
  fi
else
  echo "NOTE: cargo not on PATH; skipped the 'cargo metadata --locked' assertion" >&2
fi

echo "PASS: set-provider moves Cargo.lock and the result validates"

# ---------------------------------------------------------------------------
# 2. A Cargo.toml/Cargo.lock mismatch makes validate FAIL.
#
# Restoring only Cargo.lock recreates the exact state PR #390 merged: every
# other declaration on the new version, the lock still on the old one.
# ---------------------------------------------------------------------------
git checkout -- Cargo.lock

if grep -q "\"${PROBE_VERSION}\"" Cargo.lock; then
  fail "restoring Cargo.lock did not remove ${PROBE_VERSION}; the mismatch was not planted"
fi

set +e
output="$(python3 tools/provider_versions.py validate --provider "${PROVIDER}" 2>&1)"
status=$?
set -e

if [ "${status}" -eq 0 ]; then
  fail "validate accepted a tree whose Cargo.lock still pins ${CRATE} at ${ORIGINAL_VERSION} while its Cargo.toml declares ${PROBE_VERSION}"
fi

if ! printf '%s' "${output}" | grep -q "Cargo.lock"; then
  echo "${output}" >&2
  fail "validate exited non-zero but not for the Cargo.lock mismatch"
fi

echo "PASS: validate rejects a Cargo.toml/Cargo.lock version mismatch"
