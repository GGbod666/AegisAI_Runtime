#!/usr/bin/env bash
set -u

SCRIPT_DIR="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" >/dev/null 2>&1 && pwd)"
REPO_ROOT="$(cd -- "${SCRIPT_DIR}/../.." >/dev/null 2>&1 && pwd)"
LOG_PATH="${AEGISAI_VERIFY_LOG:-${REPO_ROOT}/docs/verification_log.md}"

mkdir -p "$(dirname -- "${LOG_PATH}")"
touch "${LOG_PATH}"

append() {
  printf '%s\n' "$*" >>"${LOG_PATH}"
}

append_block() {
  local file="$1"
  append '```text'
  cat "${file}" >>"${LOG_PATH}"
  append '```'
}

run_and_log() {
  local requirement="$1"
  local label="$2"
  shift 2

  local tmp
  tmp="$(mktemp)"

  append ""
  append "#### ${label}"
  append ""
  append "- Requirement: ${requirement}"
  append "- Command: \`$*\`"
  append "- Working directory: \`${REPO_ROOT}\`"

  (
    cd "${REPO_ROOT}" &&
      "$@"
  ) >"${tmp}" 2>&1
  local status=$?

  append "- Exit status: \`${status}\`"
  append_block "${tmp}"
  rm -f "${tmp}"

  return "${status}"
}

skip_and_log() {
  local label="$1"
  local reason="$2"

  append ""
  append "#### ${label}"
  append ""
  append "- Requirement: optional"
  append "- Status: \`SKIPPED\`"
  append "- Reason: ${reason}"
}

has_cargo_command() {
  cargo --list 2>/dev/null | awk '{print $1}' | grep -qx "$1"
}

timestamp="$(date -Iseconds)"
overall_status=0

append ""
append "### ${timestamp} - Workspace verification pass"
append ""
append "- Scope: post-change validation for runtime control loop and Linux preflight path."
append "- Working directory: \`${REPO_ROOT}\`"
append "- Log path: \`${LOG_PATH}\`"

run_and_log required "Host kernel" uname -a || overall_status=1
run_and_log required "Rust compiler version" rustc --version || overall_status=1
run_and_log required "Cargo version" cargo --version || overall_status=1
run_and_log required "Cargo check" cargo check --workspace || overall_status=1
run_and_log required "Cargo test" cargo test --workspace || overall_status=1

run_and_log required "Tool Call Booster report unit tests" \
  python3 -m unittest discover -s bench/tool_call_booster -p 'test_*.py' || overall_status=1

run_and_log required "Inference Tail Guard report unit tests" \
  python3 -m unittest discover -s bench/scripts -p 'test_*.py' || overall_status=1

if has_cargo_command fmt; then
  run_and_log required "Cargo fmt check" cargo fmt --all -- --check || overall_status=1
else
  skip_and_log "Cargo fmt check" "\`cargo fmt\` is not installed in this toolchain."
  overall_status=1
fi

if has_cargo_command clippy; then
  run_and_log required "Cargo clippy" cargo clippy --all-targets --all-features -- -D warnings || overall_status=1
else
  skip_and_log "Cargo clippy" "\`cargo clippy\` is not installed in this toolchain."
  overall_status=1
fi

run_and_log required "Mock daemon smoke test" \
  cargo run -p aegisai-runtime-daemon -- \
    --repo-root . \
    --source mock \
    --metadata demo \
    --actuator-backend noop || overall_status=1

run_and_log required "Linux source preflight smoke test" \
  cargo run -p aegisai-runtime-daemon -- \
    --repo-root . \
    --source linux \
    --metadata procfs \
    --actuator-backend linux-skeleton \
    --allow-partial-probes || overall_status=1

append ""
if [[ "${overall_status}" -eq 0 ]]; then
  append "- Overall result: \`PASS\`"
else
  append "- Overall result: \`FAIL\`"
fi

exit "${overall_status}"
