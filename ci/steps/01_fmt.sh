#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "${ROOT_DIR}"

run_fmt() {
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

fmt_failed=0
while IFS= read -r manifest; do
  saw_manifest=1
  [ -z "${manifest}" ] && continue
  if [ "${manifest}" = "__workspace__" ]; then
    cargo fmt --check || fmt_failed=1
  else
    cargo fmt --check --manifest-path "${manifest}" || fmt_failed=1
  fi
done < <(run_fmt)

if [ "${saw_manifest:-0}" -eq 0 ] && [ -n "${COMPONENT_MANIFESTS_JSON:-}" ]; then
  echo "No Rust manifests selected for cargo fmt; skipping."
  exit 0
fi

if [ "${fmt_failed}" -ne 0 ]; then
  if command -v rustup >/dev/null 2>&1; then
    toolchain="$(rustup show active-toolchain | awk '{print $1}')"
    rustup component add --toolchain "${toolchain}" rustfmt clippy
    fmt_failed=0
    while IFS= read -r manifest; do
      [ -z "${manifest}" ] && continue
      if [ "${manifest}" = "__workspace__" ]; then
        cargo fmt --check || fmt_failed=1
      else
        cargo fmt --check --manifest-path "${manifest}" || fmt_failed=1
      fi
    done < <(run_fmt)
    [ "${fmt_failed}" -eq 0 ] || exit 1
  else
    exit 1
  fi
fi
