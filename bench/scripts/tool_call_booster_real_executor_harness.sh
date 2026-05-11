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
EXECUTOR_WORK_UNITS="${AEGISAI_TCB_EXECUTOR_WORK_UNITS:-0}"
WORKER_WORK_UNITS="${AEGISAI_TCB_WORKER_WORK_UNITS:-0}"
WORKER_IO_KB="${AEGISAI_TCB_WORKER_IO_KB:-256}"
WORKER_START_DELAY_MS="${AEGISAI_TCB_WORKER_START_DELAY_MS:-50}"
EXECUTOR_AFFINITY_CPUS="${AEGISAI_TCB_EXECUTOR_AFFINITY_CPUS:-}"
BACKGROUND_AFFINITY_CPUS="${AEGISAI_TCB_BACKGROUND_AFFINITY_CPUS:-}"
BASELINE_EXECUTOR_AFFINITY_CPUS="${AEGISAI_TCB_BASELINE_EXECUTOR_AFFINITY_CPUS:-${EXECUTOR_AFFINITY_CPUS}}"
BASELINE_BACKGROUND_AFFINITY_CPUS="${AEGISAI_TCB_BASELINE_BACKGROUND_AFFINITY_CPUS:-${BACKGROUND_AFFINITY_CPUS}}"
DAEMON_START_DELAY="${AEGISAI_TCB_DAEMON_START_DELAY:-0.5}"
DAEMON_POLL_TIMEOUT_MS="${AEGISAI_TCB_DAEMON_POLL_TIMEOUT_MS:-100}"
DAEMON_BATCH_SIZE="${AEGISAI_TCB_DAEMON_BATCH_SIZE:-16}"
DAEMON_MAX_EVENTS="${AEGISAI_TCB_DAEMON_MAX_EVENTS:-64}"
DAEMON_DRAIN_MS="${AEGISAI_TCB_DAEMON_DRAIN_MS:-1200}"
DAEMON_TICK_MS="${AEGISAI_TCB_DAEMON_TICK_MS:-100}"
LIVE_PID_DERIVE_TIMEOUT_MS="${AEGISAI_TCB_LIVE_PID_DERIVE_TIMEOUT_MS:-1000}"
ACTUATOR_BACKEND="${AEGISAI_TCB_ACTUATOR_BACKEND:-noop}"
LIVE_CONFIRM="${AEGISAI_CONFIRM_LIVE_ACTUATOR:-0}"
LIVE_PID_ALLOWLIST="${AEGISAI_LIVE_PID_ALLOWLIST:-}"
LIVE_ENABLE_AFFINITY="${AEGISAI_ENABLE_LIVE_AFFINITY:-}"
WARMUP_EXECUTOR_COMMAND="${AEGISAI_TCB_WARMUP_EXECUTOR_COMMAND:-}"
WARMUP_EXECUTOR_ARGS="${AEGISAI_TCB_WARMUP_EXECUTOR_ARGS:-}"
WARMUP_EXECUTOR_TIMEOUT_MS="${AEGISAI_TCB_WARMUP_EXECUTOR_TIMEOUT_MS:-250}"
RUN_DRY_RUN="${AEGISAI_TCB_RUN_DRY_RUN:-1}"
ROUNDS="${AEGISAI_TCB_ROUNDS:-3}"
if [[ -n "${AEGISAI_TCB_MODES:-}" ]]; then
  MODES="${AEGISAI_TCB_MODES}"
elif [[ "${RUN_DRY_RUN}" == "1" ]]; then
  MODES="baseline,noop,dry_run"
else
  MODES="baseline,noop"
fi
MIN_BENEFIT_PCT="${AEGISAI_TCB_MIN_BENEFIT_PCT:-5}"
REQUIRE_BENEFIT="${AEGISAI_TCB_REQUIRE_BENEFIT:-0}"

mkdir -p "${ARTIFACT_DIR}" "$(dirname -- "${LOG_PATH}")"
touch "${LOG_PATH}"

CONFIG_ROOT="${ARTIFACT_DIR}/repo-config"
DETAIL_CSV="${ARTIFACT_DIR}/tool_call_booster_detail.csv"
SUMMARY_CSV="${ARTIFACT_DIR}/tool_call_booster_summary.csv"
STAGE_EFFECTIVENESS_CSV="${ARTIFACT_DIR}/tool_call_booster_stage_effectiveness.csv"
REPORT_MD="${ARTIFACT_DIR}/tool_call_booster_benefit_report.md"
RUN_ENV="${ARTIFACT_DIR}/run.env"
DAEMON_BIN="${REPO_ROOT}/target/debug/aegisai-runtime-daemon"
BUILD_STDOUT="${ARTIFACT_DIR}/cargo-build.stdout"
BUILD_STDERR="${ARTIFACT_DIR}/cargo-build.stderr"
REPORT_STDOUT="${ARTIFACT_DIR}/report.stdout"
REPORT_STDERR="${ARTIFACT_DIR}/report.stderr"

executor_pid=""
daemon_pid=""
overall_status=0

usage() {
  cat <<'USAGE'
Usage: bash bench/scripts/tool_call_booster_real_executor_harness.sh

Runs a real local tool executor process tree and observes it with the runtime
daemon linux/procfs source. By default it runs repeated
baseline/noop/dry_run A/B rounds and writes latency deltas plus a benefit
verdict. Include live_guarded in AEGISAI_TCB_MODES only for an explicitly
approved live actuator experiment.

Common overrides:
  AEGISAI_TCB_ROUNDS=3
  AEGISAI_TCB_MODES=baseline,noop,dry_run
  AEGISAI_TCB_EXECUTOR_CPU_MS=1800
  AEGISAI_TCB_WORKER_CPU_MS=2600
  AEGISAI_TCB_EXECUTOR_WORK_UNITS=0
  AEGISAI_TCB_WORKER_WORK_UNITS=0
  AEGISAI_TCB_WORKER_IO_KB=256
  AEGISAI_TCB_EXECUTOR_AFFINITY_CPUS=
  AEGISAI_TCB_BACKGROUND_AFFINITY_CPUS=
  AEGISAI_TCB_BASELINE_EXECUTOR_AFFINITY_CPUS=
  AEGISAI_TCB_BASELINE_BACKGROUND_AFFINITY_CPUS=
  AEGISAI_TCB_ACTUATOR_BACKEND=noop
  AEGISAI_TCB_MIN_BENEFIT_PCT=5
  AEGISAI_TCB_REQUIRE_BENEFIT=0
  AEGISAI_CONFIRM_LIVE_ACTUATOR=1
  AEGISAI_LIVE_PID_ALLOWLIST=1234  # optional; derived per round when omitted
  AEGISAI_TCB_WARMUP_EXECUTOR_COMMAND=/path/to/prime-cache
  AEGISAI_TCB_WARMUP_EXECUTOR_ARGS='--cache /tmp/cache'
  AEGISAI_TCB_WARMUP_EXECUTOR_TIMEOUT_MS=250
  AEGISAI_TCB_ARTIFACT_DIR=/path/to/results
  AEGISAI_TCB_LIVE_PID_DERIVE_TIMEOUT_MS=1000
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

validate_number_env() {
  local name="$1"
  local value="$2"
  if ! [[ "${value}" =~ ^[0-9]+([.][0-9]+)?$ ]]; then
    fail "${name} must be a non-negative number"
  fi
}

is_pid_allowlist() {
  [[ "${1:-}" =~ ^[0-9]+(,[0-9]+)*$ ]]
}

is_live_mode() {
  case "$1" in
    guarded|live_guarded|linux_command|linux-command)
      return 0
      ;;
    *)
      return 1
      ;;
  esac
}

mode_backend() {
  local mode="$1"
  case "${mode}" in
    baseline)
      printf 'none\n'
      ;;
    noop|noop_observation)
      printf '%s\n' "${ACTUATOR_BACKEND}"
      ;;
    dry_run)
      printf 'linux-command-dry-run\n'
      ;;
    guarded|live_guarded|linux_command|linux-command)
      printf 'linux-command\n'
      ;;
    *)
      printf 'unknown Tool Call Booster mode: %s\n' "${mode}" >&2
      return 1
      ;;
  esac
}

report_arg() {
  if [[ "${REQUIRE_BENEFIT}" == "1" ]]; then
    printf '%s\n' "--require-benefit"
  fi
}

has_live_mode() {
  local raw_mode mode
  IFS=',' read -r -a modes <<<"${MODES}"
  for raw_mode in "${modes[@]}"; do
    mode="$(printf '%s' "${raw_mode}" | xargs)"
    if is_live_mode "${mode}"; then
      return 0
    fi
  done
  return 1
}

resolve_live_affinity_default() {
  if [[ -n "${LIVE_ENABLE_AFFINITY}" ]]; then
    return
  fi

  if has_live_mode; then
    LIVE_ENABLE_AFFINITY="1"
  else
    LIVE_ENABLE_AFFINITY="0"
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
    printf 'executor_work_units=%s\n' "${EXECUTOR_WORK_UNITS}"
    printf 'worker_work_units=%s\n' "${WORKER_WORK_UNITS}"
    printf 'worker_io_kb=%s\n' "${WORKER_IO_KB}"
    printf 'worker_start_delay_ms=%s\n' "${WORKER_START_DELAY_MS}"
    printf 'executor_affinity_cpus=%s\n' "${EXECUTOR_AFFINITY_CPUS}"
    printf 'background_affinity_cpus=%s\n' "${BACKGROUND_AFFINITY_CPUS}"
    printf 'baseline_executor_affinity_cpus=%s\n' "${BASELINE_EXECUTOR_AFFINITY_CPUS}"
    printf 'baseline_background_affinity_cpus=%s\n' "${BASELINE_BACKGROUND_AFFINITY_CPUS}"
    printf 'daemon_poll_timeout_ms=%s\n' "${DAEMON_POLL_TIMEOUT_MS}"
    printf 'daemon_max_events=%s\n' "${DAEMON_MAX_EVENTS}"
    printf 'live_pid_derive_timeout_ms=%s\n' "${LIVE_PID_DERIVE_TIMEOUT_MS}"
    printf 'actuator_backend=%s\n' "${ACTUATOR_BACKEND}"
    printf 'live_confirm=%s\n' "${LIVE_CONFIRM}"
    printf 'live_pid_allowlist=%s\n' "${LIVE_PID_ALLOWLIST}"
    printf 'live_enable_affinity=%s\n' "${LIVE_ENABLE_AFFINITY}"
    printf 'warmup_executor_command=%s\n' "${WARMUP_EXECUTOR_COMMAND}"
    printf 'warmup_executor_args=%s\n' "${WARMUP_EXECUTOR_ARGS}"
    printf 'warmup_executor_timeout_ms=%s\n' "${WARMUP_EXECUTOR_TIMEOUT_MS}"
    printf 'run_dry_run=%s\n' "${RUN_DRY_RUN}"
    printf 'rounds=%s\n' "${ROUNDS}"
    printf 'modes=%s\n' "${MODES}"
    printf 'min_benefit_pct=%s\n' "${MIN_BENEFIT_PCT}"
    printf 'require_benefit=%s\n' "${REQUIRE_BENEFIT}"
  } >"${RUN_ENV}"
}

start_executor() {
  local tool_call_id="$1"
  local stdout="$2"
  local stderr="$3"
  local executor_affinity_cpus="$4"
  local background_affinity_cpus="$5"

  python3 "${REPO_ROOT}/bench/tool_call_booster/real_tool_executor.py" \
    tool-executor \
    --tool-call-id "${tool_call_id}" \
    --output-dir "${ARTIFACT_DIR}/executor-work" \
    --executor-cpu-ms "${EXECUTOR_CPU_MS}" \
    --worker-cpu-ms "${WORKER_CPU_MS}" \
    --executor-work-units "${EXECUTOR_WORK_UNITS}" \
    --worker-work-units "${WORKER_WORK_UNITS}" \
    --worker-io-kb "${WORKER_IO_KB}" \
    --worker-start-delay-ms "${WORKER_START_DELAY_MS}" \
    --executor-affinity-cpus "${executor_affinity_cpus}" \
    --background-affinity-cpus "${background_affinity_cpus}" \
    >"${stdout}" 2>"${stderr}" &
  executor_pid="$!"
}

run_daemon() {
  local backend="$1"
  local live_pid_allowlist="$2"
  local stdout="$3"
  local stderr="$4"
  local -a live_args=()
  local -a warmup_args=()

  if [[ "${backend}" == "linux-command" ]]; then
    live_args=(--confirm-live-actuator --live-pid-allowlist "${live_pid_allowlist}")
    if [[ "${LIVE_ENABLE_AFFINITY}" == "1" ]]; then
      live_args+=(--enable-live-affinity)
    fi
  fi
  if [[ "${backend}" == "linux-command" || "${backend}" == "linux-command-dry-run" ]]; then
    if [[ -n "${WARMUP_EXECUTOR_COMMAND}" ]]; then
      warmup_args=(
        --warmup-executor-command "${WARMUP_EXECUTOR_COMMAND}"
        --warmup-executor-timeout-ms "${WARMUP_EXECUTOR_TIMEOUT_MS}"
      )
      if [[ -n "${WARMUP_EXECUTOR_ARGS}" ]]; then
        # shellcheck disable=SC2206
        local split_warmup_args=(${WARMUP_EXECUTOR_ARGS})
        local warmup_arg
        for warmup_arg in "${split_warmup_args[@]}"; do
          warmup_args+=(--warmup-executor-arg "${warmup_arg}")
        done
      fi
    fi
  fi

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
    "${live_args[@]}" \
    "${warmup_args[@]}" \
    >"${stdout}" 2>"${stderr}" &
  daemon_pid="$!"
}

derive_live_pid_allowlist() {
  local deadline
  local child_count=0
  local child_pid
  local pids=()

  if [[ -n "${LIVE_PID_ALLOWLIST}" ]]; then
    printf '%s\n' "${LIVE_PID_ALLOWLIST}"
    return 0
  fi

  if [[ -z "${executor_pid}" ]] || ! kill -0 "${executor_pid}" >/dev/null 2>&1; then
    return 1
  fi

  pids+=("${executor_pid}")
  if ! has_command pgrep; then
    printf 'pgrep is required to derive live_guarded child PID allowlist\n' >&2
    return 1
  fi

  deadline=$(( $(date +%s%3N) + LIVE_PID_DERIVE_TIMEOUT_MS ))
  while kill -0 "${executor_pid}" >/dev/null 2>&1; do
    child_count="$(pgrep -P "${executor_pid}" 2>/dev/null | wc -l | tr -d '[:space:]')"
    if [[ "${child_count}" -ge 3 ]]; then
      break
    fi
    if [[ "$(date +%s%3N)" -ge "${deadline}" ]]; then
      break
    fi
    sleep 0.02
  done

  while IFS= read -r child_pid; do
    if [[ -n "${child_pid}" ]]; then
      pids+=("${child_pid}")
    fi
  done < <(pgrep -P "${executor_pid}" 2>/dev/null || true)

  printf '%s\n' "${pids[*]}" | tr ' ' ','
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

count_executor_roles() {
  grep -h '"role":' "${ARTIFACT_DIR}"/executor.*.stdout 2>/dev/null | wc -l | tr -d '[:space:]'
}

run_round_mode() {
  local round="$1"
  local mode="$2"
  local backend="$3"
  local prefix="round${round}.${mode}"
  local tool_call_id="${BASE_TOOL_CALL_ID}-r${round}-${mode}"
  local executor_stdout="${ARTIFACT_DIR}/executor.${prefix}.stdout"
  local executor_stderr="${ARTIFACT_DIR}/executor.${prefix}.stderr"
  local daemon_stdout="${ARTIFACT_DIR}/daemon.${prefix}.stdout"
  local daemon_stderr="${ARTIFACT_DIR}/daemon.${prefix}.stderr"
  local executor_affinity_cpus="${EXECUTOR_AFFINITY_CPUS}"
  local background_affinity_cpus="${BACKGROUND_AFFINITY_CPUS}"
  local live_pid_allowlist=""

  if [[ "${mode}" == "baseline" ]]; then
    executor_affinity_cpus="${BASELINE_EXECUTOR_AFFINITY_CPUS}"
    background_affinity_cpus="${BASELINE_BACKGROUND_AFFINITY_CPUS}"
  fi

  start_executor \
    "${tool_call_id}" \
    "${executor_stdout}" \
    "${executor_stderr}" \
    "${executor_affinity_cpus}" \
    "${background_affinity_cpus}"
  if [[ "${mode}" != "baseline" ]]; then
    sleep "${DAEMON_START_DELAY}"
    if is_live_mode "${mode}"; then
      if ! live_pid_allowlist="$(derive_live_pid_allowlist)"; then
        fail "could not derive live_guarded PID allowlist for ${prefix}"
      elif ! is_pid_allowlist "${live_pid_allowlist}"; then
        fail "derived invalid live_guarded PID allowlist for ${prefix}: ${live_pid_allowlist}"
      else
        printf '%s\n' "${live_pid_allowlist}" >"${ARTIFACT_DIR}/live_pid_allowlist.${prefix}.txt"
      fi
    fi
    if [[ "${overall_status}" -eq 0 ]]; then
      run_daemon "${backend}" "${live_pid_allowlist}" "${daemon_stdout}" "${daemon_stderr}"
    fi
  fi
  wait_for_executor
  if [[ "${mode}" != "baseline" && -n "${daemon_pid}" ]]; then
    wait_for_daemon
  fi
}

generate_report() {
  local require_arg
  require_arg="$(report_arg)"
  python3 "${REPO_ROOT}/bench/tool_call_booster/summarize_ab.py" \
    --artifact-dir "${ARTIFACT_DIR}" \
    --run-id "${RUN_ID}" \
    --modes "${MODES}" \
    --rounds "${ROUNDS}" \
    --min-benefit-pct "${MIN_BENEFIT_PCT}" \
    --detail-csv "${DETAIL_CSV}" \
    --summary-csv "${SUMMARY_CSV}" \
    --stage-effectiveness-csv "${STAGE_EFFECTIVENESS_CSV}" \
    --report-md "${REPORT_MD}" \
    ${require_arg} \
    >"${REPORT_STDOUT}" 2>"${REPORT_STDERR}"
  local status=$?
  if [[ "${status}" -ne 0 ]]; then
    fail "Tool Call Booster A/B report exited with status ${status}"
  fi
  if [[ ! -s "${STAGE_EFFECTIVENESS_CSV}" ]]; then
    fail "Tool Call Booster A/B report did not write ${STAGE_EFFECTIVENESS_CSV}"
  fi
}

require_command cargo
require_command python3
validate_uint_env AEGISAI_TCB_EXECUTOR_CPU_MS "${EXECUTOR_CPU_MS}"
validate_uint_env AEGISAI_TCB_WORKER_CPU_MS "${WORKER_CPU_MS}"
validate_uint_env AEGISAI_TCB_EXECUTOR_WORK_UNITS "${EXECUTOR_WORK_UNITS}"
validate_uint_env AEGISAI_TCB_WORKER_WORK_UNITS "${WORKER_WORK_UNITS}"
validate_uint_env AEGISAI_TCB_WORKER_IO_KB "${WORKER_IO_KB}"
validate_uint_env AEGISAI_TCB_WORKER_START_DELAY_MS "${WORKER_START_DELAY_MS}"
validate_uint_env AEGISAI_TCB_DAEMON_MAX_EVENTS "${DAEMON_MAX_EVENTS}"
validate_uint_env AEGISAI_TCB_LIVE_PID_DERIVE_TIMEOUT_MS "${LIVE_PID_DERIVE_TIMEOUT_MS}"
validate_uint_env AEGISAI_TCB_WARMUP_EXECUTOR_TIMEOUT_MS "${WARMUP_EXECUTOR_TIMEOUT_MS}"
validate_uint_env AEGISAI_TCB_ROUNDS "${ROUNDS}"
validate_number_env AEGISAI_TCB_MIN_BENEFIT_PCT "${MIN_BENEFIT_PCT}"
if [[ "${WARMUP_EXECUTOR_TIMEOUT_MS}" == "0" ]]; then
  fail "AEGISAI_TCB_WARMUP_EXECUTOR_TIMEOUT_MS must be positive"
fi
if has_live_mode; then
  if [[ "${LIVE_CONFIRM}" != "1" ]]; then
    fail "live_guarded requires AEGISAI_CONFIRM_LIVE_ACTUATOR=1"
  fi
  if [[ -n "${LIVE_PID_ALLOWLIST}" ]] && ! is_pid_allowlist "${LIVE_PID_ALLOWLIST}"; then
    fail "AEGISAI_LIVE_PID_ALLOWLIST must be a comma-separated list of positive PIDs when set"
  fi
fi
resolve_live_affinity_default

if [[ "${overall_status}" -ne 0 ]]; then
  exit "${overall_status}"
fi

copy_config
write_run_env
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

IFS=',' read -r -a SELECTED_MODES <<<"${MODES}"
for round in $(seq 1 "${ROUNDS}"); do
  for raw_mode in "${SELECTED_MODES[@]}"; do
    mode="$(printf '%s' "${raw_mode}" | xargs)"
    if [[ -z "${mode}" ]]; then
      continue
    fi
    if ! backend="$(mode_backend "${mode}" 2>&1)"; then
      fail "${backend}"
      continue
    fi
    run_round_mode "${round}" "${mode}" "${backend}"
  done
done

generate_report

append
append "### $(date -u +%Y-%m-%dT%H:%M:%SZ) - Tool Call Booster repeated A/B benefit harness"
append
append "- Run ID: \`${RUN_ID}\`"
append "- Artifact dir: \`${ARTIFACT_DIR}\`"
append "- Tool call id base: \`${BASE_TOOL_CALL_ID}\`"
append "- Rounds: \`${ROUNDS}\`"
append "- Modes: \`${MODES}\`"
append "- Executor roles observed: \`$(count_executor_roles)\`"
append "- Report verdict:"
append_block "${REPORT_STDOUT}"
append "- Aggregate summary:"
append_block "${SUMMARY_CSV}"
append "- Stage effectiveness:"
append_block "${STAGE_EFFECTIVENESS_CSV}"
append "- Detail:"
append_block "${DETAIL_CSV}"
append "- Report: \`${REPORT_MD}\`"

if [[ "${overall_status}" -eq 0 ]]; then
  printf 'Tool Call Booster repeated A/B benefit harness PASS\n'
  printf 'Artifacts: %s\n' "${ARTIFACT_DIR}"
else
  printf 'Tool Call Booster repeated A/B benefit harness FAIL\n' >&2
fi

exit "${overall_status}"
