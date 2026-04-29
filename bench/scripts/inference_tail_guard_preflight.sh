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

optional_failure_note() {
  local label="$1"
  local note="$2"

  append ""
  append "#### ${label} optional failure"
  append ""
  append "- Requirement: optional"
  append "- Status: \`NON_BLOCKING\`"
  append "- Note: ${note}"
}

note_and_log() {
  local label="$1"
  local note="$2"

  append ""
  append "#### ${label}"
  append ""
  append "- Requirement: informational"
  append "- Note: ${note}"
}

has_command() {
  command -v "$1" >/dev/null 2>&1
}

find_llama_cpp_binary() {
  local candidate
  for candidate in llama-cli llama-server llama-main; do
    if has_command "${candidate}"; then
      printf '%s\n' "${candidate}"
      return 0
    fi
  done
  return 1
}

timestamp="$(date -Iseconds)"
overall_status=0

append ""
append "### ${timestamp} - Inference Tail Guard preflight"
append ""
append "- Scope: Linux VM/demo readiness for \`AI Workload Awareness -> Inference Tail Guard\`."
append "- Working directory: \`${REPO_ROOT}\`"
append "- Log path: \`${LOG_PATH}\`"
append "- Required checks: Linux procfs/cgroup visibility and mock/noop runtime daemon smoke test."
append "- Optional inventory: \`ollama\`, common \`llama.cpp\` binaries, \`stress-ng\`, and \`taskset\`."
append "- Ollama/model installation: \`outside this preflight stage\`"
append "- Model download: \`not performed\`"
append "- Load generation: \`not started\`"

run_and_log required "Host kernel" uname -a || overall_status=1
run_and_log required "Kernel release" uname -r || overall_status=1
run_and_log required "Current cgroup membership" cat /proc/self/cgroup || overall_status=1
run_and_log required "Current cpuset" cat /proc/self/cpuset || overall_status=1
run_and_log required "Allowed CPU list" grep '^Cpus_allowed_list:' /proc/self/status || overall_status=1

if has_command cargo; then
  run_and_log required "Mock runtime daemon smoke test" \
    cargo run -p aegisai-runtime-daemon -- \
      --repo-root . \
      --source mock \
      --metadata demo \
      --actuator-backend noop || overall_status=1
else
  append ""
  append "#### Mock runtime daemon smoke test"
  append ""
  append "- Requirement: required"
  append "- Status: \`FAIL\`"
  append "- Reason: \`cargo\` is not installed or is not on PATH."
  overall_status=1
fi

if has_command ollama; then
  run_and_log optional "ollama version" ollama --version ||
    optional_failure_note "ollama version" "\`ollama\` exists on PATH, but \`ollama --version\` failed. This does not block pre-Ollama readiness."
  note_and_log "ollama model execution" "Skipped by design. This preflight does not run \`ollama run\` or pull a model."
else
  skip_and_log "ollama version" "\`ollama\` is not installed or is not on PATH."
fi

llama_cpp_binary="$(find_llama_cpp_binary || true)"
if [[ -n "${llama_cpp_binary}" ]]; then
  run_and_log optional "llama.cpp binary check" "${llama_cpp_binary}" --help ||
    optional_failure_note "llama.cpp binary check" "\`${llama_cpp_binary} --help\` failed. This does not block pre-Ollama readiness."
  note_and_log "llama.cpp model execution" "Skipped by design. This preflight does not require a local GGUF model."
else
  skip_and_log "llama.cpp binary check" "No common llama.cpp binary was found on PATH: \`llama-cli\`, \`llama-server\`, or \`llama-main\`."
fi

if has_command stress-ng; then
  run_and_log optional "stress-ng version" stress-ng --version ||
    optional_failure_note "stress-ng version" "\`stress-ng\` exists on PATH, but \`stress-ng --version\` failed. This does not block pre-Ollama readiness."
  note_and_log "stress-ng load generation" "Skipped by design. This preflight records availability without starting CPU or I/O pressure."
else
  skip_and_log "stress-ng version" "\`stress-ng\` is not installed or is not on PATH."
fi

if has_command taskset; then
  run_and_log optional "taskset version" taskset --version ||
    optional_failure_note "taskset version" "\`taskset\` exists on PATH, but \`taskset --version\` failed. This does not block pre-Ollama readiness."
else
  skip_and_log "taskset version" "\`taskset\` is not installed or is not on PATH."
fi

append ""
if [[ "${overall_status}" -eq 0 ]]; then
  append "- Overall result: \`PASS\`"
else
  append "- Overall result: \`FAIL\`"
fi

exit "${overall_status}"
