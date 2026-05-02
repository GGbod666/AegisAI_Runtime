#!/usr/bin/env bash
set -u

SCRIPT_DIR="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" >/dev/null 2>&1 && pwd)"
REPO_ROOT="$(cd -- "${SCRIPT_DIR}/../.." >/dev/null 2>&1 && pwd)"
LOG_PATH="${AEGISAI_VERIFY_LOG:-${REPO_ROOT}/docs/verification_log.md}"
RUN_ID="${AEGISAI_PHASE2R2_RUN_ID:-$(date -u +%Y%m%dT%H%M%SZ)}"
ARTIFACT_DIR="${AEGISAI_PHASE2R2_ARTIFACT_DIR:-${REPO_ROOT}/.cache/aegisai/inference_tail_guard_phase2r2/${RUN_ID}}"
ROUNDS="${AEGISAI_PHASE2R2_NICE_ROUNDS:-3}"
RUN_AFFINITY="${AEGISAI_PHASE2R2_RUN_AFFINITY:-1}"

mkdir -p "$(dirname -- "${LOG_PATH}")" "${ARTIFACT_DIR}"
touch "${LOG_PATH}"

SUMMARY_CSV="${ARTIFACT_DIR}/phase2r2_actuator_quality.csv"

append_log() {
  printf '%s\n' "$*" >>"${LOG_PATH}"
}

is_positive_uint() {
  [[ "${1:-}" =~ ^[0-9]+$ ]] && [[ "$1" -gt 0 ]]
}

is_pid_allowlist() {
  local raw="${1//,/ }"
  local pid count=0

  for pid in ${raw}; do
    if ! is_positive_uint "${pid}"; then
      return 1
    fi
    count=$((count + 1))
  done

  [[ "${count}" -gt 0 ]]
}

csv_field() {
  local file="$1"
  local column="$2"
  python3 - "$file" "$column" <<'PY'
import csv
import sys

path, column = sys.argv[1:3]
try:
    with open(path, newline="", encoding="utf-8") as handle:
        row = next(csv.DictReader(handle), {})
except (FileNotFoundError, StopIteration):
    row = {}
print(row.get(column, "missing"))
PY
}

run_live_round() {
  local phase="$1"
  local round="$2"
  local affinity="$3"
  local run_dir="${ARTIFACT_DIR}/${phase}/round_${round}"
  local status
  local mode_contract_file
  local counts_file
  local action_errors mode_contract actuator_quality live_nice live_affinity cpuset_disabled rollbacks triggers processed
  local expected_scope_contract

  mkdir -p "${run_dir}"

  append_log ""
  append_log "#### Phase 2R-2 ${phase} round ${round}"
  append_log ""
  append_log "- Artifact directory: \`${run_dir}\`"
  append_log "- Live affinity enabled: \`${affinity}\`"

  AEGISAI_AB_RUN_ID="${RUN_ID}_${phase}_${round}" \
    AEGISAI_AB_ARTIFACT_DIR="${run_dir}" \
    AEGISAI_ACCEPTANCE_PHASE="2R-2" \
    AEGISAI_ACCEPTANCE_GOAL="actuator_quality_convergence" \
    AEGISAI_AB_MODES=live_guarded \
    AEGISAI_ENABLE_LIVE_AFFINITY="${affinity}" \
    AEGISAI_VERIFY_LOG="${LOG_PATH}" \
    bash "${SCRIPT_DIR}/inference_tail_guard_ollama_smoke.sh" \
    >"${run_dir}/harness.stdout" 2>"${run_dir}/harness.stderr"
  status=$?

  mode_contract_file="${run_dir}/mode_contract.csv"
  counts_file="${run_dir}/mode_counts.csv"
  action_errors="$(csv_field "${counts_file}" action_error_count)"
  processed="$(csv_field "${counts_file}" processed_events)"
  triggers="$(csv_field "${counts_file}" trigger_count)"
  rollbacks="$(csv_field "${counts_file}" rollback_count)"
  mode_contract="$(csv_field "${mode_contract_file}" mode_contract)"
  actuator_quality="$(csv_field "${mode_contract_file}" actuator_quality_contract)"
  live_nice="$(csv_field "${mode_contract_file}" live_nice_only_contract)"
  live_affinity="$(csv_field "${mode_contract_file}" live_affinity_contract)"
  cpuset_disabled="$(csv_field "${mode_contract_file}" live_cpuset_disabled_contract)"
  if [[ "${affinity}" == "1" ]]; then
    expected_scope_contract="${live_affinity}"
  else
    expected_scope_contract="${live_nice}"
  fi

  printf '%s,%s,%s,%s,%s,%s,%s,%s,%s,%s,%s,%s,%s\n' \
    "${phase}" \
    "${round}" \
    "${affinity}" \
    "${status}" \
    "${mode_contract}" \
    "${actuator_quality}" \
    "${live_nice}" \
    "${live_affinity}" \
    "${cpuset_disabled}" \
    "${processed}" \
    "${triggers}" \
    "${rollbacks}" \
    "${action_errors}" \
    >>"${SUMMARY_CSV}"

  append_log "- Harness exit status: \`${status}\`"
  append_log "- Mode contract: \`${mode_contract}\`"
  append_log "- Actuator quality contract: \`${actuator_quality}\`"
  append_log "- Live nice-only contract: \`${live_nice}\`"
  append_log "- Live affinity contract: \`${live_affinity}\`"
  append_log "- Live cpuset-disabled contract: \`${cpuset_disabled}\`"
  append_log "- Action audit errors: \`${action_errors}\`"
  append_log "- Harness stdout: \`${run_dir}/harness.stdout\`"
  append_log "- Harness stderr: \`${run_dir}/harness.stderr\`"

  [[ "${status}" -eq 0 && "${mode_contract}" == "PASS" && "${actuator_quality}" == "PASS" && "${cpuset_disabled}" == "PASS" && "${expected_scope_contract}" == "PASS" && "${action_errors}" == "0" ]]
}

if ! is_positive_uint "${ROUNDS}" || [[ "${ROUNDS}" -lt 3 ]]; then
  printf 'AEGISAI_PHASE2R2_NICE_ROUNDS must be at least 3.\n' >&2
  exit 1
fi
if [[ "${RUN_AFFINITY}" != "0" && "${RUN_AFFINITY}" != "1" ]]; then
  printf 'AEGISAI_PHASE2R2_RUN_AFFINITY must be 0 or 1.\n' >&2
  exit 1
fi
if [[ "${AEGISAI_CONFIRM_LIVE_ACTUATOR:-0}" != "1" ]]; then
  printf 'Phase 2R-2 requires AEGISAI_CONFIRM_LIVE_ACTUATOR=1.\n' >&2
  exit 1
fi
if ! is_pid_allowlist "${AEGISAI_LIVE_PID_ALLOWLIST:-}"; then
  printf 'Phase 2R-2 requires AEGISAI_LIVE_PID_ALLOWLIST with one or more positive PIDs.\n' >&2
  exit 1
fi

if ! command -v python3 >/dev/null 2>&1; then
  printf 'Phase 2R-2 requires python3.\n' >&2
  exit 1
fi

timestamp="$(date -Iseconds)"
append_log ""
append_log "### ${timestamp} - Phase 2R-2 actuator quality convergence"
append_log ""
append_log "- Scope: nice-only first reaches at least three clean live rounds; affinity runs only after that gate passes. cpuset remains disabled."
append_log "- Working directory: \`${REPO_ROOT}\`"
append_log "- Artifact directory: \`${ARTIFACT_DIR}\`"
append_log "- Nice-only rounds: \`${ROUNDS}\`"
append_log "- Affinity after nice gate: \`${RUN_AFFINITY}\`"
append_log "- Live PID allowlist: \`${AEGISAI_LIVE_PID_ALLOWLIST}\`"

printf 'phase,round,affinity_enabled,harness_status,mode_contract,actuator_quality_contract,live_nice_only_contract,live_affinity_contract,live_cpuset_disabled_contract,processed_events,trigger_count,rollback_count,action_error_count\n' >"${SUMMARY_CSV}"

overall_status=0
nice_pass_count=0
for ((round = 1; round <= ROUNDS; round += 1)); do
  if run_live_round "nice_only" "${round}" "0"; then
    nice_pass_count=$((nice_pass_count + 1))
  else
    overall_status=1
  fi
done

append_log ""
append_log "#### Phase 2R-2 nice-only gate"
append_log ""
append_log "- Nice-only clean rounds: \`${nice_pass_count}/${ROUNDS}\`"

if [[ "${nice_pass_count}" -ge 3 && "${RUN_AFFINITY}" == "1" ]]; then
  if ! run_live_round "affinity" "1" "1"; then
    overall_status=1
  fi
elif [[ "${RUN_AFFINITY}" == "1" ]]; then
  overall_status=1
  append_log "- Affinity round: `SKIPPED`"
  append_log "- Reason: nice-only gate did not reach 3 clean rounds."
fi

append_log ""
append_log "#### Phase 2R-2 summary"
append_log ""
append_log "- Summary CSV: \`${SUMMARY_CSV}\`"
append_log "- Overall result: \`$([[ "${overall_status}" -eq 0 ]] && printf PASS || printf FAIL)\`"

printf '%s\n' "Phase 2R-2 actuator quality summary:"
cat "${SUMMARY_CSV}"
printf '%s\n' "Artifacts: ${ARTIFACT_DIR}"
printf '%s\n' "Verification log: ${LOG_PATH}"

exit "${overall_status}"
