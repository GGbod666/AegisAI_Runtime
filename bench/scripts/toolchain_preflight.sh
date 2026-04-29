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

check_command() {
  local requirement="$1"
  local command_name="$2"

  if command -v "${command_name}" >/dev/null 2>&1; then
    run_and_log "${requirement}" "command ${command_name}" command -v "${command_name}"
    return 0
  fi

  append ""
  append "#### command ${command_name}"
  append ""
  append "- Requirement: ${requirement}"
  append "- Status: \`MISSING\`"
  append "- Command: \`command -v ${command_name}\`"
  return 1
}

package_inventory() {
  if command -v rpm >/dev/null 2>&1; then
    rpm -q rustfmt clippy stress-ng bpftool clang llvm util-linux 2>&1 || true
    return 0
  fi

  if command -v dpkg-query >/dev/null 2>&1; then
    dpkg-query -W -f='${binary:Package}\t${Version}\n' \
      rustfmt clippy stress-ng bpftool clang llvm util-linux 2>&1 || true
    return 0
  fi

  printf '%s\n' "No supported package inventory command found: rpm or dpkg-query."
}

timestamp="$(date -Iseconds)"
overall_status=0

append ""
append "### ${timestamp} - Toolchain preflight"
append ""
append "- Scope: tool availability before Ollama installation and model download."
append "- Working directory: \`${REPO_ROOT}\`"
append "- Log path: \`${LOG_PATH}\`"
append "- Install action: \`not performed\`"

run_and_log required "OS release" cat /etc/os-release || overall_status=1
run_and_log required "Cargo command list" cargo --list || overall_status=1
run_and_log informational "Installed package inventory" package_inventory

check_command required rustc || overall_status=1
check_command required cargo || overall_status=1
check_command required bpftool || overall_status=1
check_command required clang || overall_status=1
check_command required llc || overall_status=1
check_command required taskset || overall_status=1

check_command optional rustfmt || true
check_command optional cargo-fmt || true
check_command optional clippy-driver || true
check_command optional cargo-clippy || true
check_command optional stress-ng || true

append ""
append "- Recommended required-tool install if approval is available: \`dnf install -y bpftool clang llvm util-linux\`"
append "- Recommended optional-tool install if approval is available: \`dnf install -y rustfmt clippy stress-ng\`"
append "- Debian/Ubuntu equivalent packages: \`apt-get install -y bpftool clang llvm util-linux rustfmt clippy stress-ng\`"
append "- Ollama/model installation: \`outside this stage\`"
if [[ "${overall_status}" -eq 0 ]]; then
  append "- Overall result: \`PASS\`"
else
  append "- Overall result: \`FAIL\`"
fi

exit "${overall_status}"
