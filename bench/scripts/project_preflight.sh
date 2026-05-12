#!/usr/bin/env bash
set -u

SCRIPT_DIR="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" >/dev/null 2>&1 && pwd)"
REPO_ROOT="$(cd -- "${SCRIPT_DIR}/../.." >/dev/null 2>&1 && pwd)"

usage() {
  cat <<'USAGE'
Usage: bash bench/scripts/project_preflight.sh [--check]

Print the AegisAI_Runtime project preflight checklist. With --check, run the
listed gates from the repository root.
USAGE
}

print_checklist() {
  cat <<'CHECKLIST'
Project Preflight Checklist:

[ ] Rust format: cargo fmt --all -- --check
[ ] Rust tests: cargo test --workspace
[ ] Rust lint: cargo clippy --all-targets --all-features -- -D warnings
[ ] Tool Call Booster Python tests: python3 -m unittest discover -s bench/tool_call_booster -p 'test_*.py'
[ ] Bench script Python tests: python3 -m unittest discover -s bench/scripts -p 'test_*.py'
[ ] Shell syntax: for f in bench/scripts/*.sh; do bash -n "$f" || exit 1; done
[ ] Workspace preflight: AEGISAI_VERIFY_LOG=/tmp/aegisai_project_preflight_verify_workspace.md bash bench/scripts/verify_workspace.sh
[ ] Toolchain preflight: AEGISAI_VERIFY_LOG=/tmp/aegisai_project_preflight_toolchain.md bash bench/scripts/toolchain_preflight.sh
[ ] Inference preflight: AEGISAI_VERIFY_LOG=/tmp/aegisai_project_preflight_inference.md bash bench/scripts/inference_tail_guard_preflight.sh

Note: upstream `bd preflight` in bd 1.0.3 prints Beads' own Go/Nix template.
That output is irrelevant to AegisAI_Runtime readiness; use this project
preflight path instead.

Run 'bash bench/scripts/project_preflight.sh --check' to validate automatically.
CHECKLIST
}

run_check() {
  local label="$1"
  shift

  printf '\n==> %s\n' "${label}"
  printf '+'
  printf ' %q' "$@"
  printf '\n'

  (
    cd "${REPO_ROOT}" &&
      "$@"
  )
}

run_shell_syntax_check() {
  printf '\n==> Shell syntax\n'
  printf '+ for f in bench/scripts/*.sh; do bash -n "$f" || exit 1; done\n'

  (
    cd "${REPO_ROOT}" &&
      for f in bench/scripts/*.sh; do
        bash -n "$f" || exit 1
      done
  )
}

run_all_checks() {
  local overall_status=0

  run_check "Rust format" cargo fmt --all -- --check || overall_status=1
  run_check "Rust tests" cargo test --workspace || overall_status=1
  run_check "Rust lint" cargo clippy --all-targets --all-features -- -D warnings || overall_status=1
  run_check "Tool Call Booster Python tests" \
    python3 -m unittest discover -s bench/tool_call_booster -p 'test_*.py' || overall_status=1
  run_check "Bench script Python tests" \
    python3 -m unittest discover -s bench/scripts -p 'test_*.py' || overall_status=1
  run_shell_syntax_check || overall_status=1
  run_check "Workspace preflight" \
    env AEGISAI_VERIFY_LOG=/tmp/aegisai_project_preflight_verify_workspace.md \
      bash bench/scripts/verify_workspace.sh || overall_status=1
  run_check "Toolchain preflight" \
    env AEGISAI_VERIFY_LOG=/tmp/aegisai_project_preflight_toolchain.md \
      bash bench/scripts/toolchain_preflight.sh || overall_status=1
  run_check "Inference preflight" \
    env AEGISAI_VERIFY_LOG=/tmp/aegisai_project_preflight_inference.md \
      bash bench/scripts/inference_tail_guard_preflight.sh || overall_status=1

  return "${overall_status}"
}

case "${1:-}" in
  "")
    print_checklist
    ;;
  "--check")
    run_all_checks
    ;;
  "-h" | "--help")
    usage
    ;;
  *)
    printf 'unknown argument: %s\n' "$1" >&2
    usage >&2
    exit 2
    ;;
esac
