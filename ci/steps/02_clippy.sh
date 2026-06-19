#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "${ROOT_DIR}"

TARGET_DIR="${CARGO_TARGET_DIR:-${ROOT_DIR}/target}"

clear_greentic_interfaces_wasmtime_build_cache() {
  local path
  for path in \
    "${TARGET_DIR}/debug/build" \
    "${TARGET_DIR}/debug/.fingerprint" \
    "${TARGET_DIR}/debug/deps"
  do
    [ -d "${path}" ] || continue
    find "${path}" -maxdepth 1 -mindepth 1 \
      \( -name 'greentic-interfaces-wasmtime-*' -o -name 'libgreentic_interfaces_wasmtime*' \) \
      -exec rm -rf {} +
  done
}

# Clippy can restore stale generated bindgen output from the cache that points at
# registry-local WIT staging paths which do not exist yet on a fresh runner.
bash "${ROOT_DIR}/tools/sync_wit_deps_from_greentic_interfaces.sh"
clear_greentic_interfaces_wasmtime_build_cache

run_clippy() {
  if [ -n "${COMPONENT_MANIFESTS_JSON:-}" ]; then
    python3 - <<'PY' "${COMPONENT_MANIFESTS_JSON}"
import json
from pathlib import Path
import sys
for manifest in json.loads(sys.argv[1]):
    if Path(manifest).name == "Cargo.toml":
        print(manifest)
PY
    return
  fi
  echo "__workspace__"
}

clippy_failed=0
while IFS= read -r manifest; do
  saw_manifest=1
  [ -z "${manifest}" ] && continue
  if [ "${manifest}" = "__workspace__" ]; then
    cargo clippy --workspace --all-targets || clippy_failed=1
  else
    cargo clippy --manifest-path "${manifest}" --all-targets || clippy_failed=1
  fi
done < <(run_clippy)

if [ "${saw_manifest:-0}" -eq 0 ] && [ -n "${COMPONENT_MANIFESTS_JSON:-}" ]; then
  echo "No Rust manifests selected for cargo clippy; skipping."
  exit 0
fi

if [ "${clippy_failed}" -ne 0 ]; then
  if command -v rustup >/dev/null 2>&1; then
    toolchain="$(rustup show active-toolchain | awk '{print $1}')"
    rustup component add --toolchain "${toolchain}" clippy
    clear_greentic_interfaces_wasmtime_build_cache
    clippy_failed=0
    while IFS= read -r manifest; do
      [ -z "${manifest}" ] && continue
      if [ "${manifest}" = "__workspace__" ]; then
        cargo clippy --workspace --all-targets || clippy_failed=1
      else
        cargo clippy --manifest-path "${manifest}" --all-targets || clippy_failed=1
      fi
    done < <(run_clippy)
    [ "${clippy_failed}" -eq 0 ] || exit 1
  else
    exit 1
  fi
fi
