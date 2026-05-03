#!/usr/bin/env bash
set -u

SCRIPT_DIR="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" >/dev/null 2>&1 && pwd)"
REPO_ROOT="$(cd -- "${SCRIPT_DIR}/../.." >/dev/null 2>&1 && pwd)"
LOG_PATH="${AEGISAI_VERIFY_LOG:-${REPO_ROOT}/docs/verification_log.md}"
RUN_ID="${AEGISAI_TCB_RUN_ID:-$(date -u +%Y%m%dT%H%M%SZ)}"
ARTIFACT_DIR="${AEGISAI_TCB_ARTIFACT_DIR:-${REPO_ROOT}/.cache/aegisai/tool_call_booster/${RUN_ID}}"
BASE_TOOL_CALL_ID="${AEGISAI_TCB_TOOL_CALL_ID:-tc-real-001}"
EXECUTOR_CPU_MS="${AEGISAI_TCB_EXECUTOR_CPU_MS:-1800}"
WORKER_CPU_MS="${AEGISAI_TCB_WORKER_CPU_MS:-2600}"
WORKER_IO_KB="${AEGISAI_TCB_WORKER_IO_KB:-256}"
WORKER_START_DELAY_MS="${AEGISAI_TCB_WORKER_START_DELAY_MS:-50}"
DAEMON_START_DELAY="${AEGISAI_TCB_DAEMON_START_DELAY:-0.5}"
DAEMON_POLL_TIMEOUT_MS="${AEGISAI_TCB_DAEMON_POLL_TIMEOUT_MS:-100}"
DAEMON_BATCH_SIZE="${AEGISAI_TCB_DAEMON_BATCH_SIZE:-16}"
DAEMON_MAX_EVENTS="${AEGISAI_TCB_DAEMON_MAX_EVENTS:-64}"
DAEMON_DRAIN_MS="${AEGISAI_TCB_DAEMON_DRAIN_MS:-1200}"
DAEMON_TICK_MS="${AEGISAI_TCB_DAEMON_TICK_MS:-100}"
ACTUATOR_BACKEND="${AEGISAI_TCB_ACTUATOR_BACKEND:-noop}"
RUN_DRY_RUN="${AEGISAI_TCB_RUN_DRY_RUN:-1}"

mkdir -p "${ARTIFACT_DIR}" "$(dirname -- "${LOG_PATH}")"
touch "${LOG_PATH}"

CONFIG_ROOT="${ARTIFACT_DIR}/repo-config"
SUMMARY_CSV="${ARTIFACT_DIR}/tool_call_booster_summary.csv"
RUN_ENV="${ARTIFACT_DIR}/run.env"
DAEMON_BIN="${REPO_ROOT}/target/debug/aegisai-runtime-daemon"
BUILD_STDOUT="${ARTIFACT_DIR}/cargo-build.stdout"
BUILD_STDERR="${ARTIFACT_DIR}/cargo-build.stderr"

executor_pid=""
daemon_pid=""
overall_status=0

usage() {
  cat <<'USAGE'
Usage: bash bench/scripts/tool_call_booster_real_executor_harness.sh

Runs a real local tool executor process tree and observes it with the runtime
daemon linux/procfs source. The default actuator backend is noop; set
AEGISAI_TCB_RUN_DRY_RUN=1 to also run a second linux-command-dry-run pass.

Common overrides:
  AEGISAI_TCB_EXECUTOR_CPU_MS=1800
  AEGISAI_TCB_WORKER_CPU_MS=2600
  AEGISAI_TCB_WORKER_IO_KB=256
  AEGISAI_TCB_ACTUATOR_BACKEND=noop
  AEGISAI_TCB_ARTIFACT_DIR=/path/to/results
USAGE
}

if [[ "${1:-}" == "--help" || "${1:-}" == "-h" ]]; then
  usage
  exit 0
fi

append() {
  printf '%s\n' "$*" >>"${LOG_PATH}"
}

append_block() {
  local file="$1"
  append '```text'
  if [[ -s "${file}" ]]; then
    cat "${file}" >>"${LOG_PATH}"
  else
    printf '%s\n' "_empty_" >>"${LOG_PATH}"
  fi
  append '```'
}

has_command() {
  command -v "$1" >/dev/null 2>&1
}

is_uint() {
  [[ "${1:-}" =~ ^[0-9]+$ ]]
}

fail() {
  printf 'ERROR: %s\n' "$*" >&2
  overall_status=1
}

cleanup() {
  local pid
  for pid in "${daemon_pid:-}" "${executor_pid:-}"; do
    if [[ -n "${pid}" ]] && kill -0 "${pid}" >/dev/null 2>&1; then
      kill "${pid}" >/dev/null 2>&1 || true
      wait "${pid}" >/dev/null 2>&1 || true
    fi
  done
}
trap cleanup EXIT

require_command() {
  if ! has_command "$1"; then
    fail "required command missing: $1"
  fi
}

validate_uint_env() {
  local name="$1"
  local value="$2"
  if ! is_uint "${value}"; then
    fail "${name} must be an unsigned integer"
  fi
}

copy_config() {
  mkdir -p \
    "${CONFIG_ROOT}/configs/runtime" \
    "${CONFIG_ROOT}/configs/classifier" \
    "${CONFIG_ROOT}/configs/scenarios" \
    "${CONFIG_ROOT}/configs/safety"

  cp "${REPO_ROOT}/configs/classifier/process_rules.example.toml" \
    "${CONFIG_ROOT}/configs/classifier/process_rules.example.toml"
  cp "${REPO_ROOT}/configs/scenarios/ai_workload_awareness.example.toml" \
    "${CONFIG_ROOT}/configs/scenarios/ai_workload_awareness.example.toml"
  cp "${REPO_ROOT}/configs/safety/default.toml" \
    "${CONFIG_ROOT}/configs/safety/default.toml"

  cat >"${CONFIG_ROOT}/configs/runtime/runtime.example.toml" <<'EOF_RUNTIME'
[target]
deployment_target = "linux"
kernel_min = "5.15"
cgroup_version = "v2"

[runtime]
primary_runtime = "tool-executor"
fallback_runtime = "python"

[selection]
mode = "process_name"
process_names = ["python", "python3"]
pid_allowlist = []

[collection]
focus_signals = [
  "run_queue_delay",
  "cpu_migration",
  "major_page_fault",
  "subprocess_start_delay",
  "queue_wait",
  "io_latency"
]

[metrics]
track = [
  "tool_call_latency",
  "boost_hit_rate",
  "rollback_count",
  "side_effect_rate"
]
EOF_RUNTIME

  cat >"${CONFIG_ROOT}/configs/scenarios/inference_tail_guard.example.toml" <<'EOF_INFERENCE'
[policy]
active_scenarios = []
evaluation_window_ms = 500
cooldown_ms = 100
max_boost_duration_ms = 500
EOF_INFERENCE

  cat >"${CONFIG_ROOT}/configs/scenarios/tool_call_booster.example.toml" <<'EOF_TOOL'
[policy]
active_scenarios = ["tool_call_booster"]
evaluation_window_ms = 500
cooldown_ms = 100
max_boost_duration_ms = 500

[triggers.tool_call_booster]
run_queue_delay_us = 1
cpu_migrations_per_sec = 1
major_page_faults_per_sec = 1
subprocess_start_delay_us = 1500
queue_wait_us = 2000
optional_io_latency_us = 4000

[actions.tool_call_booster]
raise_nice = -3
pin_strategy = "prefer_low_contention_cores"
warmup_executor = true
EOF_TOOL
}

write_run_env() {
  {
    printf 'run_id=%s\n' "${RUN_ID}"
    printf 'tool_call_id_base=%s\n' "${BASE_TOOL_CALL_ID}"
    printf 'artifact_dir=%s\n' "${ARTIFACT_DIR}"
    printf 'executor_cpu_ms=%s\n' "${EXECUTOR_CPU_MS}"
    printf 'worker_cpu_ms=%s\n' "${WORKER_CPU_MS}"
    printf 'worker_io_kb=%s\n' "${WORKER_IO_KB}"
    printf 'worker_start_delay_ms=%s\n' "${WORKER_START_DELAY_MS}"
    printf 'daemon_poll_timeout_ms=%s\n' "${DAEMON_POLL_TIMEOUT_MS}"
    printf 'daemon_max_events=%s\n' "${DAEMON_MAX_EVENTS}"
    printf 'actuator_backend=%s\n' "${ACTUATOR_BACKEND}"
    printf 'run_dry_run=%s\n' "${RUN_DRY_RUN}"
  } >"${RUN_ENV}"
}

start_executor() {
  local tool_call_id="$1"
  local stdout="$2"
  local stderr="$3"

  python3 "${REPO_ROOT}/bench/tool_call_booster/real_tool_executor.py" \
    tool-executor \
    --tool-call-id "${tool_call_id}" \
    --output-dir "${ARTIFACT_DIR}/executor-work" \
    --executor-cpu-ms "${EXECUTOR_CPU_MS}" \
    --worker-cpu-ms "${WORKER_CPU_MS}" \
    --worker-io-kb "${WORKER_IO_KB}" \
    --worker-start-delay-ms "${WORKER_START_DELAY_MS}" \
    >"${stdout}" 2>"${stderr}" &
  executor_pid="$!"
}

run_daemon() {
  local backend="$1"
  local stdout="$2"
  local stderr="$3"

  "${DAEMON_BIN}" \
    --repo-root "${CONFIG_ROOT}" \
    --source linux \
    --metadata procfs \
    --actuator-backend "${backend}" \
    --allow-partial-probes \
    --probe-poll-timeout-ms "${DAEMON_POLL_TIMEOUT_MS}" \
    --batch-size "${DAEMON_BATCH_SIZE}" \
    --max-events "${DAEMON_MAX_EVENTS}" \
    --tick-ms "${DAEMON_TICK_MS}" \
    --drain-ms "${DAEMON_DRAIN_MS}" \
    >"${stdout}" 2>"${stderr}" &
  daemon_pid="$!"
}

build_daemon() {
  cargo build -p aegisai-runtime-daemon >"${BUILD_STDOUT}" 2>"${BUILD_STDERR}"
  local status=$?
  if [[ "${status}" -ne 0 ]]; then
    fail "cargo build -p aegisai-runtime-daemon exited with status ${status}"
  fi
}

wait_for_executor() {
  local status=0
  wait "${executor_pid}" || status=$?
  executor_pid=""
  if [[ "${status}" -ne 0 ]]; then
    fail "tool executor exited with status ${status}"
  fi
}

wait_for_daemon() {
  local status=0
  wait "${daemon_pid}" || status=$?
  daemon_pid=""
  if [[ "${status}" -ne 0 ]]; then
    fail "runtime daemon exited with status ${status}"
  fi
}

field_value() {
  local file="$1"
  local key="$2"
  awk -F': ' -v key="${key}" '$1 == key { print $2; exit }' "${file}"
}

lifecycle_line() {
  local file="$1"
  local tool_call_id="$2"
  grep -F "  ${tool_call_id}:" "${file}" | head -n 1
}

tool_call_trigger_count() {
  local file="$1"
  awk '/^[[:space:]]+tool_call_booster: / { print $2; exit }' "${file}"
}

count_executor_roles() {
  grep -h '"role":' "${ARTIFACT_DIR}"/executor.*.stdout 2>/dev/null | wc -l | tr -d '[:space:]'
}

summarize_pass() {
  local mode="$1"
  local stdout="$2"
  local tool_call_id="$3"
  local executor_stdout="$4"
  local processed applied inline_rollbacks tick_rollbacks total_rollbacks triggered lifecycle stages role_count pass
  processed="$(field_value "${stdout}" "processed_events")"
  applied="$(field_value "${stdout}" "applied_actions")"
  inline_rollbacks="$(field_value "${stdout}" "inline_rollbacks")"
  tick_rollbacks="$(field_value "${stdout}" "tick_rollbacks")"
  if is_uint "${inline_rollbacks}" && is_uint "${tick_rollbacks}"; then
    total_rollbacks=$((inline_rollbacks + tick_rollbacks))
  else
    total_rollbacks=0
  fi
  triggered="$(tool_call_trigger_count "${stdout}")"
  lifecycle="$(lifecycle_line "${stdout}" "${tool_call_id}")"
  stages="$(printf '%s\n' "${lifecycle}" | sed -n 's/.*stages=\([^ ]*\) boosted_actions.*/\1/p')"
  role_count="$(grep -c '"role":' "${executor_stdout}" 2>/dev/null || true)"
  pass="FAIL"

  if is_uint "${processed}" && [[ "${processed}" -gt 0 ]] \
    && is_uint "${applied}" && [[ "${applied}" -gt 0 ]] \
    && is_uint "${triggered}" && [[ "${triggered}" -gt 0 ]] \
    && [[ "${total_rollbacks}" -gt 0 ]] \
    && is_uint "${role_count}" && [[ "${role_count}" -ge 4 ]] \
    && [[ -n "${lifecycle}" ]] \
    && [[ "${stages}" == *"executor:"* ]] \
    && [[ "${stages}" == *"retrieval:"* ]] \
    && [[ "${stages}" == *"rerank:"* ]]; then
    pass="PASS"
  fi

  printf '%s,%s,%s,%s,%s,%s,%s,%s,"%s"\n' \
    "${mode}" "${pass}" "${tool_call_id}" "${processed:-0}" "${applied:-0}" \
    "${total_rollbacks:-0}" "${triggered:-0}" "${role_count:-0}" "${stages:-none}" \
    >>"${SUMMARY_CSV}"

  if [[ "${pass}" != "PASS" ]]; then
    fail "${mode} did not observe a triggered real tool-call lifecycle"
  fi
}

run_pass() {
  local mode="$1"
  local backend="$2"
  local tool_call_id="${BASE_TOOL_CALL_ID}-${mode}"
  local executor_stdout="${ARTIFACT_DIR}/executor.${mode}.stdout"
  local executor_stderr="${ARTIFACT_DIR}/executor.${mode}.stderr"
  local daemon_stdout="${ARTIFACT_DIR}/daemon.${mode}.stdout"
  local daemon_stderr="${ARTIFACT_DIR}/daemon.${mode}.stderr"

  start_executor "${tool_call_id}" "${executor_stdout}" "${executor_stderr}"
  sleep "${DAEMON_START_DELAY}"
  run_daemon "${backend}" "${daemon_stdout}" "${daemon_stderr}"
  wait_for_executor
  wait_for_daemon
  summarize_pass "${mode}" "${daemon_stdout}" "${tool_call_id}" "${executor_stdout}"
}

require_command cargo
require_command python3
validate_uint_env AEGISAI_TCB_EXECUTOR_CPU_MS "${EXECUTOR_CPU_MS}"
validate_uint_env AEGISAI_TCB_WORKER_CPU_MS "${WORKER_CPU_MS}"
validate_uint_env AEGISAI_TCB_WORKER_IO_KB "${WORKER_IO_KB}"
validate_uint_env AEGISAI_TCB_WORKER_START_DELAY_MS "${WORKER_START_DELAY_MS}"
validate_uint_env AEGISAI_TCB_DAEMON_MAX_EVENTS "${DAEMON_MAX_EVENTS}"

if [[ "${overall_status}" -ne 0 ]]; then
  exit "${overall_status}"
fi

copy_config
write_run_env
printf 'mode,contract,tool_call_id,processed_events,applied_actions,total_rollbacks,tool_call_booster_triggers,executor_roles,stages\n' >"${SUMMARY_CSV}"
build_daemon
if [[ "${overall_status}" -ne 0 ]]; then
  append
  append "### $(date -u +%Y-%m-%dT%H:%M:%SZ) - Tool Call Booster real executor harness"
  append
  append "- Run ID: \`${RUN_ID}\`"
  append "- Artifact dir: \`${ARTIFACT_DIR}\`"
  append "- Build failed:"
  append_block "${BUILD_STDERR}"
  exit "${overall_status}"
fi

run_pass "noop" "${ACTUATOR_BACKEND}"
if [[ "${RUN_DRY_RUN}" == "1" ]]; then
  run_pass "dry_run" "linux-command-dry-run"
fi

append
append "### $(date -u +%Y-%m-%dT%H:%M:%SZ) - Tool Call Booster real executor harness"
append
append "- Run ID: \`${RUN_ID}\`"
append "- Artifact dir: \`${ARTIFACT_DIR}\`"
append "- Tool call id base: \`${BASE_TOOL_CALL_ID}\`"
append "- Executor roles observed: \`$(count_executor_roles)\`"
append "- Summary:"
append_block "${SUMMARY_CSV}"
append "- Executor stdout files: \`executor.noop.stdout\`, \`executor.dry_run.stdout\`"

if [[ "${overall_status}" -eq 0 ]]; then
  printf 'Tool Call Booster real executor harness PASS\n'
  printf 'Artifacts: %s\n' "${ARTIFACT_DIR}"
else
  printf 'Tool Call Booster real executor harness FAIL\n' >&2
fi

exit "${overall_status}"
