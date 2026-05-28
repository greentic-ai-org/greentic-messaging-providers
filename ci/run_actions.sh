#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "${ROOT_DIR}"

ACT_BIN="${ACT_BIN:-act}"
WORKFLOW="${WORKFLOW:-.github/workflows/provider-build-publish.yml}"
EVENT="${EVENT:-workflow_dispatch}"
JOB="${JOB:-}"
MATRIX="${MATRIX:-}"
SINGLE="${SINGLE:-}"
QUICK="${QUICK:-0}"
ACT_IMAGE="${ACT_IMAGE:-catthehacker/ubuntu:act-latest}"
PLATFORM="${ACT_PLATFORM:-ubuntu-latest=${ACT_IMAGE}}"
SECRETS_FILE="${ACT_SECRETS_FILE:-.secrets.act}"
ENV_FILE="${ACT_ENV_FILE:-.env.act}"
ACT_VERSION="${ACT_VERSION:-v0.2.82}"
ACT_BIND="${ACT_BIND:-1}"
ACT_CAPTURE_MODE="${ACT_CAPTURE_MODE:-auto}"
ACT_TMPROOT="${ACT_TMPROOT:-${ACT_TMPDIR:-${ROOT_DIR}/.tmp/act-tmp}}"
ACT_ARTIFACT_PATH="${ACT_ARTIFACT_PATH:-${ROOT_DIR}/.tmp/act-artifacts}"
RUN_REPORT_DIR="${RUN_REPORT_DIR:-${ROOT_DIR}/.tmp/run-actions}"
LIST_JOBS="${LIST_JOBS:-0}"
QUICK_TARGET_ROOT="${QUICK_TARGET_ROOT:-${ROOT_DIR}/.tmp/quick-target}"
QUICK_PACKS_DIR="${QUICK_PACKS_DIR:-${ROOT_DIR}/.tmp/quick-packs}"
QUICK_TARGET_ROOT_REL="${QUICK_TARGET_ROOT_REL:-.tmp/quick-target}"
QUICK_PACKS_DIR_REL="${QUICK_PACKS_DIR_REL:-.tmp/quick-packs}"

usage() {
  cat <<'EOF'
Usage:
  ./ci/run_actions.sh [event] [-- ACT_ARGS...]

Defaults:
  event: workflow_dispatch
  workflow: .github/workflows/provider-build-publish.yml
  runner image: catthehacker/ubuntu:act-latest

Environment overrides:
  ACT_BIN=act
  WORKFLOW=.github/workflows/provider-build-publish.yml
  EVENT=workflow_dispatch
  JOB=package-provider
  MATRIX=component:messaging-provider-dummy
  SINGLE=messaging-provider-dummy
  QUICK=1
  ACT_IMAGE=catthehacker/ubuntu:act-latest
  ACT_PLATFORM=ubuntu-latest=catthehacker/ubuntu:act-latest
  ACT_SECRETS_FILE=.secrets.act
  ACT_ENV_FILE=.env.act
  ACT_VERSION=v0.2.82
  ACT_BIND=1
  ACT_CAPTURE_MODE=auto
  ACT_TMPROOT=.tmp/act-tmp
  ACT_ARTIFACT_PATH=.tmp/act-artifacts
  LIST_JOBS=1
  QUICK_TARGET_ROOT=.tmp/quick-target
  QUICK_PACKS_DIR=.tmp/quick-packs
  QUICK_TARGET_ROOT_REL=.tmp/quick-target
  QUICK_PACKS_DIR_REL=.tmp/quick-packs

Examples:
  ./ci/run_actions.sh -- --input provider=dummy --input publish=false
  JOB=package-provider ./ci/run_actions.sh -- --input provider=dummy --input publish=false
  JOB=lint ./ci/run_actions.sh -- --input provider=dummy
  JOB=build-components MATRIX=component:messaging-provider-dummy ./ci/run_actions.sh -- --input provider=dummy
  QUICK=1 ./ci/run_actions.sh
  QUICK=1 JOB=build-components SINGLE=provision ./ci/run_actions.sh
  JOB=cargo-test QUICK=1 ./ci/run_actions.sh
  QUICK=1 JOB=validate-pack-inputs SINGLE=messaging-dummy ./ci/run_actions.sh
  LIST_JOBS=1 ./ci/run_actions.sh
EOF
}

prepare_act_runtime() {
  mkdir -p "${ACT_TMPROOT}" "${ACT_ARTIFACT_PATH}"
  ACT_TMPDIR="$(mktemp -d "${ACT_TMPROOT}/run.XXXXXX")"
  export TMPDIR="${ACT_TMPDIR}"
}

prepare_run_report_dir() {
  mkdir -p "${RUN_REPORT_DIR}"
}

strip_ansi_to_file() {
  local src="$1"
  local dest="$2"
  perl -pe 's/\e\[[0-9;?]*[ -\/]*[@-~]//g' "${src}" > "${dest}"
}

run_command_with_tty_log() {
  local runner_path="$1"
  shift
  {
    printf '#!/usr/bin/env bash\n'
    printf 'set -euo pipefail\n'
    printf 'cd %q\n' "${ROOT_DIR}"
    printf 'exec'
    printf ' %q' "$@"
    printf '\n'
  } > "${runner_path}"
  chmod +x "${runner_path}"
  script -qefc "${runner_path}" "${RUN_LOG_PATH}"
}

should_capture_run() {
  local mode="$1"

  if [ "${ACT_CAPTURE_MODE}" = "1" ] || [ "${ACT_CAPTURE_MODE}" = "always" ]; then
    return 0
  fi

  if [ "${ACT_CAPTURE_MODE}" = "0" ] || [ "${ACT_CAPTURE_MODE}" = "never" ]; then
    return 1
  fi

  # In auto mode, keep QUICK runs captured, but let act run directly so its
  # native terminal UI stays intact.
  [ "${mode}" != "act" ]
}

append_matches() {
  local title="$1"
  local pattern="$2"
  local source="$3"
  local limit="${4:-80}"
  {
    echo "## ${title}"
    if ! rg -N -m "${limit}" "${pattern}" "${source}"; then
      echo "(none)"
    fi
    echo
  } >> "${RUN_SUMMARY_PATH}"
}

append_step_rerun_hints() {
  local source="$1"
  local -a step_numbers
  local found=0

  mapfile -t step_numbers < <(grep -oE 'Step [0-9]{2}' "${source}" | awk '{print $2}' | sort -u)

  {
    echo "## Rerun Hints"
    if [ "${#step_numbers[@]}" -eq 0 ]; then
      echo "(no numbered CI steps detected in the log)"
      echo
      return
    fi

    for step in "${step_numbers[@]}"; do
      local matches=()
      mapfile -t matches < <(compgen -G "${ROOT_DIR}/ci/steps/${step}_*.sh" || true)
      if [ "${#matches[@]}" -gt 0 ]; then
        local rel_path="${matches[0]#${ROOT_DIR}/}"
        echo "./${rel_path}"
        found=1
      fi
    done

    if [ "${found}" -eq 0 ]; then
      echo "(no matching ci/steps scripts found for detected step numbers)"
    fi
    echo
  } >> "${RUN_SUMMARY_PATH}"
}

append_artifact_listing() {
  {
    echo "## Artifact Files"
    if [ -d "${ACT_ARTIFACT_PATH}" ]; then
      if ! find "${ACT_ARTIFACT_PATH}" -type f | sort; then
        echo "(artifact listing failed)"
      fi
    else
      echo "(artifact directory not present)"
    fi
    echo
  } >> "${RUN_SUMMARY_PATH}"
}

create_run_summary() {
  local mode="$1"
  local exit_code="$2"
  local raw_log="$3"
  local clean_log="$4"

  {
    echo "# ci/run_actions summary"
    echo
    echo "- Timestamp (UTC): $(date -u +"%Y-%m-%dT%H:%M:%SZ")"
    echo "- Mode: ${mode}"
    echo "- Exit code: ${exit_code}"
    echo "- Workflow: ${WORKFLOW}"
    echo "- Event: ${EVENT}"
    echo "- Job filter: ${JOB:-<all>}"
    echo "- Matrix filter: ${MATRIX:-<none>}"
    echo "- Single filter: ${SINGLE:-<none>}"
    echo "- Full log: ${raw_log}"
    echo "- Clean log: ${clean_log}"
    echo "- Artifact path: ${ACT_ARTIFACT_PATH}"
    echo
    echo "## Reproduction"
    echo "cd ${ROOT_DIR}"
    echo "JOB='${JOB}' MATRIX='${MATRIX}' SINGLE='${SINGLE}' QUICK='${QUICK}' ./ci/run_actions.sh ${EVENT}"
    echo
  } > "${RUN_SUMMARY_PATH}"

  append_matches "Failed Job And Step Lines" '^\[[^]]+\].*(Failure -|failed|Error:|exitcode|panic|panicked)' "${clean_log}" 200
  append_matches "Rust And Test Errors" '(^error(\[[A-Z0-9]+\])?:|^Error:|^thread .+ panicked|^failures:$|^failures:|^---- .+ ----|assertion .* failed|test result: FAILED|^FAIL \[)' "${clean_log}" 200
  append_matches "Recent Log Tail" '.' <(tail -n 120 "${clean_log}") 120
  append_step_rerun_hints "${clean_log}"
  append_artifact_listing
}

print_failure_digest() {
  echo >&2
  echo "================ ci/run_actions failure digest ================" >&2
  echo "Failure summary: ${RUN_SUMMARY_PATH}" >&2
  echo "Full log: ${RUN_LOG_PATH}" >&2
  echo "Artifact path: ${ACT_ARTIFACT_PATH}" >&2
  echo >&2
  sed -n '1,160p' "${RUN_SUMMARY_PATH}" >&2
  echo "===============================================================" >&2
}

run_with_reporting() {
  local mode="$1"
  shift
  local timestamp
  local exit_code
  local runner_path

  prepare_run_report_dir

  if ! should_capture_run "${mode}"; then
    "$@"
    return $?
  fi

  timestamp="$(date -u +"%Y%m%dT%H%M%SZ")"
  RUN_LOG_PATH="${RUN_REPORT_DIR}/${timestamp}-${mode}.log"
  RUN_CLEAN_LOG_PATH="${RUN_REPORT_DIR}/${timestamp}-${mode}.clean.log"
  RUN_SUMMARY_PATH="${RUN_REPORT_DIR}/${timestamp}-${mode}.summary.md"

  set +e
  if [ "${mode}" = "act" ] && [ -t 1 ] && command -v script >/dev/null 2>&1; then
    runner_path="${RUN_REPORT_DIR}/${timestamp}-${mode}.runner.sh"
    run_command_with_tty_log "${runner_path}" "$@"
    exit_code=$?
  else
    "$@" 2>&1 | tee "${RUN_LOG_PATH}"
    exit_code=${PIPESTATUS[0]}
  fi
  set -e

  strip_ansi_to_file "${RUN_LOG_PATH}" "${RUN_CLEAN_LOG_PATH}"
  create_run_summary "${mode}" "${exit_code}" "${RUN_LOG_PATH}" "${RUN_CLEAN_LOG_PATH}"

  echo "Run summary: ${RUN_SUMMARY_PATH}"
  echo "Run log: ${RUN_LOG_PATH}"

  if [ "${exit_code}" -ne 0 ]; then
    print_failure_digest
  fi

  return "${exit_code}"
}

prepare_quick_runtime() {
  mkdir -p \
    "${QUICK_TARGET_ROOT}/components" \
    "${QUICK_TARGET_ROOT}/cargo-component/wasm32-wasip2" \
    "${QUICK_TARGET_ROOT}/cargo-test"
}

prepare_quick_packs_copy() {
  rm -rf "${QUICK_PACKS_DIR}"
  mkdir -p "${QUICK_PACKS_DIR}"
  cp -a "${ROOT_DIR}/packs/." "${QUICK_PACKS_DIR}/"
}

install_act() {
  local gobin

  if ! command -v go >/dev/null 2>&1; then
    echo "Missing 'go'; cannot automatically install act via go install." >&2
    return 1
  fi

  gobin="${GOBIN:-$(go env GOPATH)/bin}"
  echo "Installing act ${ACT_VERSION} via go install"
  GOBIN="${gobin}" go install "github.com/nektos/act@${ACT_VERSION}"
  export PATH="${gobin}:${PATH}"
}

derive_matrix_from_single() {
  if [ -n "${SINGLE}" ] && [ -n "${MATRIX}" ]; then
    echo "Use either SINGLE or MATRIX, not both." >&2
    exit 1
  fi

  if [ -n "${SINGLE}" ]; then
    case "${JOB}" in
      build-components)
        MATRIX="component:${SINGLE}"
        ;;
      validate-pack-inputs|build-packs)
        MATRIX="pack:${SINGLE}"
        ;;
      *)
        echo "SINGLE is only supported for matrix jobs like build-components, validate-pack-inputs, or build-packs." >&2
        exit 1
        ;;
    esac
  fi
}

quick_component() {
  local component="${1:-provision}"
  echo "QUICK mode: building component '${component}' locally"
  TARGET_DIR="${QUICK_TARGET_ROOT}/components" \
    TARGET_COMPONENTS_DIR="${QUICK_TARGET_ROOT}/components" \
    TARGET_DIR_OVERRIDE="${QUICK_TARGET_ROOT}/cargo-component/wasm32-wasip2" \
    bash "./tools/build_components/${component}.sh"
}

quick_cargo_test() {
  echo "QUICK mode: staging local components and running cargo tests locally"
  mkdir -p "${ROOT_DIR}/components/provision" "${ROOT_DIR}/components/questions"
  if [ -f "${QUICK_TARGET_ROOT}/components/provision.wasm" ]; then
    cp -f "${QUICK_TARGET_ROOT}/components/provision.wasm" "${ROOT_DIR}/components/provision/provision.wasm"
  fi
  if [ -f "${QUICK_TARGET_ROOT}/components/questions.wasm" ]; then
    cp -f "${QUICK_TARGET_ROOT}/components/questions.wasm" "${ROOT_DIR}/components/questions/questions.wasm"
  fi
  CARGO_BUILD_JOBS="${CARGO_BUILD_JOBS:-1}" \
    CARGO_TARGET_DIR="${QUICK_TARGET_ROOT}/cargo-test" \
    TARGET_DIR="${QUICK_TARGET_ROOT}/components" \
    TARGET_DIR_OVERRIDE="${QUICK_TARGET_ROOT}/cargo-component/wasm32-wasip2" \
    TARGET_COMPONENTS_DIR="${QUICK_TARGET_ROOT}/components" \
    TARGET_COMPONENTS="${QUICK_TARGET_ROOT}/components" \
    RUSTFLAGS="${RUSTFLAGS:--C debuginfo=0}" \
    RUST_BACKTRACE="${RUST_BACKTRACE:-1}" \
    ./ci/steps/12_cargo_test.sh
}

quick_validate_pack_inputs() {
  local pack="${1:-messaging-dummy}"
  local lock_path="${ROOT_DIR}/packs.lock.json"
  local lock_backup=""
  echo "QUICK mode: preparing artifacts and validating pack '${pack}' locally"
  prepare_quick_packs_copy
  if [ -f "${lock_path}" ]; then
    lock_backup="$(mktemp)"
    cp -f "${lock_path}" "${lock_backup}"
  fi
  trap 'if [ -n "'"${lock_backup}"'" ] && [ -f "'"${lock_backup}"'" ]; then cp -f "'"${lock_backup}"'" "'"${lock_path}"'"; rm -f "'"${lock_backup}"'"; else rm -f "'"${lock_path}"'"; fi' RETURN
  TARGET_DIR="${QUICK_TARGET_ROOT}/components" \
    TARGET_COMPONENTS_DIR="${QUICK_TARGET_ROOT}/components" \
    TARGET_COMPONENTS="${QUICK_TARGET_ROOT}/components" \
    TARGET_DIR_OVERRIDE="${QUICK_TARGET_ROOT}/cargo-component/wasm32-wasip2" \
    CARGO_TARGET_DIR="${QUICK_TARGET_ROOT}/cargo-steps" \
    ./ci/steps/03_build_components.sh
  TARGET_COMPONENTS_DIR="${QUICK_TARGET_ROOT}/components" \
    CARGO_TARGET_DIR="${QUICK_TARGET_ROOT}/cargo-steps" \
    ./ci/steps/04_ensure_templates.sh
  TARGET_COMPONENTS_DIR="${QUICK_TARGET_ROOT}/components" \
    GENERATED_PROVIDERS_DIR="${QUICK_TARGET_ROOT}/generated/providers" \
    PACKS_DIR="${QUICK_PACKS_DIR_REL}" \
    CARGO_TARGET_DIR="${QUICK_TARGET_ROOT}/cargo-steps" \
    ./ci/steps/06_gen_flows.sh
  TARGET_COMPONENTS_DIR="${QUICK_TARGET_ROOT}/components" \
    TARGET_COMPONENTS="${QUICK_TARGET_ROOT}/components" \
    PACKS_DIR="${QUICK_PACKS_DIR_REL}" \
    CARGO_TARGET_DIR="${QUICK_TARGET_ROOT}/cargo-steps" \
    PACK_FILTER="${pack}" \
    ./ci/steps/07a_validate_pack_inputs.sh
}

run_quick_mode() {
  derive_matrix_from_single

  case "${JOB:-quick}" in
    quick)
      quick_component "${SINGLE:-provision}"
      quick_cargo_test
      quick_validate_pack_inputs "${QUICK_PACK:-messaging-dummy}"
      ;;
    build-components)
      quick_component "${SINGLE:-${MATRIX#component:}}"
      ;;
    cargo-test)
      quick_cargo_test
      ;;
    validate-pack-inputs)
      quick_validate_pack_inputs "${SINGLE:-${MATRIX#pack:}}"
      ;;
    *)
      echo "QUICK mode is supported for the default quick sequence, build-components, cargo-test, and validate-pack-inputs." >&2
      exit 1
      ;;
  esac
}

if [ "${1:-}" = "-h" ] || [ "${1:-}" = "--help" ]; then
  usage
  exit 0
fi

extra_args=()
if [ "${1:-}" = "--" ]; then
  shift
elif [ $# -gt 0 ] && [[ "${1}" != -* ]]; then
  EVENT="${1}"
  shift
fi

if [ "${1:-}" = "--" ]; then
  shift
fi

if [ $# -gt 0 ]; then
  extra_args=("$@")
fi

if [ "${QUICK}" = "1" ]; then
  prepare_quick_runtime
  run_with_reporting quick run_quick_mode
  exit 0
fi

if ! command -v "${ACT_BIN}" >/dev/null 2>&1; then
  echo "'${ACT_BIN}' not found; attempting automatic install."
  install_act
fi

if ! command -v "${ACT_BIN}" >/dev/null 2>&1; then
  echo "Failed to install '${ACT_BIN}'. Install nektos/act manually and retry." >&2
  exit 1
fi

if ! command -v docker >/dev/null 2>&1; then
  echo "Missing 'docker'. nektos/act requires a Docker-compatible runtime." >&2
  exit 1
fi

prepare_act_runtime

args=(
  "${EVENT}"
  --workflows "${WORKFLOW}"
  --platform "${PLATFORM}"
  --artifact-server-path "${ACT_ARTIFACT_PATH}"
)

if [ -n "${JOB}" ]; then
  args+=(--job "${JOB}")
fi

derive_matrix_from_single

if [ -n "${MATRIX}" ]; then
  args+=(--matrix "${MATRIX}")
fi

if [ "${ACT_BIND}" = "1" ]; then
  args+=(--bind)
fi

if [ -f "${SECRETS_FILE}" ]; then
  args+=(--secret-file "${SECRETS_FILE}")
fi

if [ -f "${ENV_FILE}" ]; then
  args+=(--env-file "${ENV_FILE}")
fi

args+=("${extra_args[@]}")

if [ "${LIST_JOBS}" = "1" ]; then
  args+=(--list)
fi

echo "Running ${ACT_BIN} ${args[*]}"
echo "Note: act mirrors the GitHub workflow file locally, but it is not a byte-for-byte GitHub runner."
echo "Using TMPDIR=${TMPDIR}"
echo "Using artifact path ${ACT_ARTIFACT_PATH}"
if should_capture_run act; then
  echo "Capture mode: enabled (logs and summary will be written under ${RUN_REPORT_DIR})"
else
  echo "Capture mode: disabled for native act UI. Set ACT_CAPTURE_MODE=1 to also write detailed failure summaries."
fi

run_with_reporting act "${ACT_BIN}" "${args[@]}"
