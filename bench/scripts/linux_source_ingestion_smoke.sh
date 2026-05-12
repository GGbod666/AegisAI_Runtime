#!/usr/bin/env bash
set -u

SCRIPT_DIR="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" >/dev/null 2>&1 && pwd)"
REPO_ROOT="$(cd -- "${SCRIPT_DIR}/../.." >/dev/null 2>&1 && pwd)"
LOG_PATH="${AEGISAI_VERIFY_LOG:-${REPO_ROOT}/docs/verification_log.md}"

BACKEND="${AEGISAI_LINUX_SOURCE_SMOKE_BACKEND:-linux-skeleton}"
MAX_EVENTS="${AEGISAI_LINUX_SOURCE_SMOKE_MAX_EVENTS:-4}"
BATCH_SIZE="${AEGISAI_LINUX_SOURCE_SMOKE_BATCH_SIZE:-16}"
POLL_TIMEOUT_MS="${AEGISAI_LINUX_SOURCE_SMOKE_POLL_TIMEOUT_MS:-200}"
TICK_MS="${AEGISAI_LINUX_SOURCE_SMOKE_TICK_MS:-200}"
DRAIN_MS="${AEGISAI_LINUX_SOURCE_SMOKE_DRAIN_MS:-50}"
PRECHECK_SLEEP_SEC="${AEGISAI_LINUX_SOURCE_SMOKE_PRECHECK_SLEEP_SEC:-0.25}"
MAX_WORKERS="${AEGISAI_LINUX_SOURCE_SMOKE_MAX_WORKERS:-64}"

TMP_ROOT=""
DAEMON_STDOUT=""
DAEMON_STDERR=""
worker_pids=()

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

cleanup() {
  local pid

  for pid in "${worker_pids[@]}"; do
    kill "${pid}" >/dev/null 2>&1 || true
  done
  for pid in "${worker_pids[@]}"; do
    wait "${pid}" >/dev/null 2>&1 || true
  done

  if [[ -n "${TMP_ROOT}" && -d "${TMP_ROOT}" ]]; then
    rm -rf "${TMP_ROOT}"
  fi
  if [[ -n "${DAEMON_STDOUT}" ]]; then
    rm -f "${DAEMON_STDOUT}"
  fi
  if [[ -n "${DAEMON_STDERR}" ]]; then
    rm -f "${DAEMON_STDERR}"
  fi
}

trap cleanup EXIT

finish_skip() {
  local reason="$1"

  append ""
  append "- Status: \`SKIPPED\`"
  append "- Skip reason: ${reason}"
  append "- Overall result: \`SKIPPED\`"
  printf 'SKIPPED: %s\n' "${reason}"
  exit 77
}

finish_fail() {
  local reason="$1"

  append ""
  append "- Status: \`FAIL\`"
  append "- Failure reason: ${reason}"
  append "- Overall result: \`FAIL\`"
  printf 'FAIL: %s\n' "${reason}"
  exit 1
}

format_command() {
  printf '%q ' "$@"
}

has_command() {
  command -v "$1" >/dev/null 2>&1
}

validate_positive_integer() {
  local name="$1"
  local value="$2"

  if ! [[ "${value}" =~ ^[1-9][0-9]*$ ]]; then
    finish_fail "${name} must be a positive integer; got \`${value}\`."
  fi
}

detect_cpu_count() {
  local cpus

  if has_command nproc; then
    cpus="$(nproc 2>/dev/null || true)"
  else
    cpus="$(getconf _NPROCESSORS_ONLN 2>/dev/null || true)"
  fi

  if [[ "${cpus}" =~ ^[1-9][0-9]*$ ]]; then
    printf '%s\n' "${cpus}"
  else
    printf '1\n'
  fi
}

worker_count() {
  local cpus="$1"

  if [[ -n "${AEGISAI_LINUX_SOURCE_SMOKE_WORKERS:-}" ]]; then
    validate_positive_integer AEGISAI_LINUX_SOURCE_SMOKE_WORKERS "${AEGISAI_LINUX_SOURCE_SMOKE_WORKERS}"
    printf '%s\n' "${AEGISAI_LINUX_SOURCE_SMOKE_WORKERS}"
    return 0
  fi

  validate_positive_integer AEGISAI_LINUX_SOURCE_SMOKE_MAX_WORKERS "${MAX_WORKERS}"
  local workers=$((cpus + 1))
  if (( workers > MAX_WORKERS )); then
    workers="${MAX_WORKERS}"
  fi
  if (( workers < 2 )); then
    workers=2
  fi
  printf '%s\n' "${workers}"
}

spawn_workers() {
  local count="$1"
  local i

  for ((i = 0; i < count; i += 1)); do
    if has_command yes; then
      yes >/dev/null &
    else
      bash -c 'while :; do :; done' >/dev/null 2>&1 &
    fi
    worker_pids+=("$!")
  done
}

join_by_comma() {
  local IFS=,
  printf '%s' "$*"
}

read_procfs_totals() {
  local readable=0
  local run_queue_delay=0
  local cpu_migrations=0
  local major_faults=0
  local pid value

  for pid in "${worker_pids[@]}"; do
    if [[ -r "/proc/${pid}/schedstat" ]]; then
      value="$(awk 'NF >= 2 && $2 ~ /^[0-9]+$/ { print $2; exit }' "/proc/${pid}/schedstat" 2>/dev/null || true)"
      if [[ "${value}" =~ ^[0-9]+$ ]]; then
        readable=1
        run_queue_delay=$((run_queue_delay + value))
      fi
    fi

    if [[ -r "/proc/${pid}/sched" ]]; then
      value="$(awk -F: '$1 ~ /^[[:space:]]*se[.]nr_migrations[[:space:]]*$/ { gsub(/^[[:space:]]+|[[:space:]]+$/, "", $2); print $2; exit }' "/proc/${pid}/sched" 2>/dev/null || true)"
      if [[ "${value}" =~ ^[0-9]+$ ]]; then
        readable=1
        cpu_migrations=$((cpu_migrations + value))
      fi
    fi

    if [[ -r "/proc/${pid}/stat" ]]; then
      value="$(awk '{ close_field = 0; for (i = 1; i <= NF; i += 1) { if ($i ~ /[)]$/) { close_field = i; break } } if (close_field > 0 && $(close_field + 10) ~ /^[0-9]+$/) { print $(close_field + 10); exit } }' "/proc/${pid}/stat" 2>/dev/null || true)"
      if [[ "${value}" =~ ^[0-9]+$ ]]; then
        readable=1
        major_faults=$((major_faults + value))
      fi
    fi
  done

  printf '%s %s %s %s\n' "${readable}" "${run_queue_delay}" "${cpu_migrations}" "${major_faults}"
}

write_runtime_config() {
  local path="$1"
  local pid_csv="$2"
  local pid_array="${pid_csv//,/, }"

  cat >"${path}" <<EOF
[target]
deployment_target = "linux"
kernel_min = "5.15"
cgroup_version = "v2"

[runtime]
primary_runtime = "procfs-smoke"
fallback_runtime = "procfs-smoke"

[selection]
mode = "pid_allowlist"
process_names = []
pid_allowlist = [${pid_array}]

[collection]
focus_signals = [
  "run_queue_delay",
  "cpu_migration",
  "major_page_fault"
]

[metrics]
track = [
  "ttft",
  "p95_latency",
  "p99_latency",
  "jitter",
  "boost_hit_rate",
  "rollback_count",
  "side_effect_rate"
]
EOF
}

append_daemon_excerpt() {
  local stdout_path="$1"
  local stderr_path="$2"

  append ""
  append "#### Daemon summary excerpt"
  append_block <(
    grep -E '^(source|metadata|actuator_backend|processed_events|applied_actions|inline_rollbacks|tick_rollbacks|metric_records|trace_records|signal_observations|feature_window_maxima|triggered_scenarios|  (run_queue_delay|cpu_migration|major_page_fault|cpu_migrations_per_sec|major_page_faults_per_sec|run_queue_delay_us_max|inference_tail_guard):)' "${stdout_path}" || true
  )

  if [[ -s "${stderr_path}" ]]; then
    append ""
    append "#### Daemon stderr"
    append_block "${stderr_path}"
  fi
}

timestamp="$(date -Iseconds)"
append ""
append "### ${timestamp} - Controlled Linux source ingestion smoke"
append ""
append "- Scope: Linux procfs-derived event ingestion using short-lived PID-allowlisted CPU workers."
append "- Command: \`bash bench/scripts/linux_source_ingestion_smoke.sh\`"
append "- Working directory: \`${REPO_ROOT}\`"
append "- Log path: \`${LOG_PATH}\`"
append "- Actuator backend: \`${BACKEND}\`"
append "- Live scheduler state changes: \`none\`"
append "- Exit codes: \`0=PASS\`, \`1=FAIL\`, \`77=SKIPPED\`"

case "${BACKEND}" in
  linux-skeleton | linux-command-dry-run) ;;
  *)
    finish_fail "AEGISAI_LINUX_SOURCE_SMOKE_BACKEND must be \`linux-skeleton\` or \`linux-command-dry-run\`."
    ;;
esac

validate_positive_integer AEGISAI_LINUX_SOURCE_SMOKE_MAX_EVENTS "${MAX_EVENTS}"
validate_positive_integer AEGISAI_LINUX_SOURCE_SMOKE_BATCH_SIZE "${BATCH_SIZE}"
validate_positive_integer AEGISAI_LINUX_SOURCE_SMOKE_POLL_TIMEOUT_MS "${POLL_TIMEOUT_MS}"
validate_positive_integer AEGISAI_LINUX_SOURCE_SMOKE_DRAIN_MS "${DRAIN_MS}"

if [[ "$(uname -s 2>/dev/null || true)" != "Linux" ]]; then
  finish_skip "procfs-derived Linux signal ingestion requires a Linux host."
fi

if [[ ! -d /proc || ! -r /proc/self/status ]]; then
  finish_skip "procfs is not mounted or /proc/self/status is not readable."
fi

if ! has_command cargo; then
  finish_fail "\`cargo\` is required to run aegisai-runtime-daemon."
fi

cpu_count="$(detect_cpu_count)"
workers="$(worker_count "${cpu_count}")"
spawn_workers "${workers}"
sleep "${PRECHECK_SLEEP_SEC}"

live_workers=()
for pid in "${worker_pids[@]}"; do
  if kill -0 "${pid}" >/dev/null 2>&1; then
    live_workers+=("${pid}")
  fi
done
worker_pids=("${live_workers[@]}")
if [[ "${#worker_pids[@]}" -eq 0 ]]; then
  finish_fail "controlled procfs smoke workers exited before sampling."
fi

read -r readable_before run_before migrations_before faults_before < <(read_procfs_totals)
sleep "${PRECHECK_SLEEP_SEC}"
read -r readable_after run_after migrations_after faults_after < <(read_procfs_totals)

append "- CPU count used for sizing: \`${cpu_count}\`"
append "- Controlled worker count: \`${#worker_pids[@]}\`"
append "- PID allowlist: \`$(join_by_comma "${worker_pids[@]}")\`"
append "- Procfs precheck before: \`readable=${readable_before} run_queue_delay=${run_before} cpu_migration=${migrations_before} major_page_fault=${faults_before}\`"
append "- Procfs precheck after: \`readable=${readable_after} run_queue_delay=${run_after} cpu_migration=${migrations_after} major_page_fault=${faults_after}\`"

if [[ "${readable_before}" != "1" || "${readable_after}" != "1" ]]; then
  finish_skip "controlled worker procfs counters are not readable for schedstat, sched, or stat."
fi

if (( run_after <= run_before && migrations_after <= migrations_before && faults_after <= faults_before )); then
  finish_skip "controlled workers did not produce a positive procfs counter delta; increase AEGISAI_LINUX_SOURCE_SMOKE_WORKERS or run on a host that exposes procfs scheduler/fault deltas."
fi

TMP_ROOT="$(mktemp -d -t aegisai-linux-source-smoke.XXXXXX)" ||
  finish_fail "failed to create temporary config root."
DAEMON_STDOUT="$(mktemp -t aegisai-linux-source-smoke-daemon.XXXXXX.out)" ||
  finish_fail "failed to create daemon stdout file."
DAEMON_STDERR="$(mktemp -t aegisai-linux-source-smoke-daemon.XXXXXX.err)" ||
  finish_fail "failed to create daemon stderr file."

cp -R "${REPO_ROOT}/configs" "${TMP_ROOT}/configs" ||
  finish_fail "failed to copy repository configs to temporary config root."
write_runtime_config "${TMP_ROOT}/configs/runtime/runtime.example.toml" "$(join_by_comma "${worker_pids[@]}")"

daemon_command=(
  cargo run -q -p aegisai-runtime-daemon --
  --repo-root "${TMP_ROOT}"
  --source linux
  --metadata procfs
  --actuator-backend "${BACKEND}"
  --probe-poll-timeout-ms "${POLL_TIMEOUT_MS}"
  --batch-size "${BATCH_SIZE}"
  --max-events "${MAX_EVENTS}"
  --tick-ms "${TICK_MS}"
  --drain-ms "${DRAIN_MS}"
)
append "- Daemon command: \`$(format_command "${daemon_command[@]}")\`"

(
  cd "${REPO_ROOT}" &&
    "${daemon_command[@]}"
) >"${DAEMON_STDOUT}" 2>"${DAEMON_STDERR}"
daemon_status=$?

append "- Daemon exit status: \`${daemon_status}\`"
append_daemon_excerpt "${DAEMON_STDOUT}" "${DAEMON_STDERR}"

if [[ "${daemon_status}" -ne 0 ]]; then
  finish_fail "runtime daemon exited non-zero during controlled Linux source ingestion smoke."
fi

processed_events="$(awk -F': ' '$1 == "processed_events" { print $2; exit }' "${DAEMON_STDOUT}" | tr -d '[:space:]')"
if ! [[ "${processed_events}" =~ ^[0-9]+$ ]]; then
  finish_fail "daemon summary did not include a parseable processed_events field."
fi

if (( processed_events <= 0 )); then
  finish_fail "daemon summary recorded processed_events=${processed_events}; expected > 0."
fi

signal_line="$(grep -E '^  (run_queue_delay|cpu_migration|major_page_fault): events=[1-9][0-9]*' "${DAEMON_STDOUT}" | head -n 1 || true)"
if [[ -z "${signal_line}" ]]; then
  finish_fail "daemon summary did not include run_queue_delay, cpu_migration, or major_page_fault signal_observations with events > 0."
fi

append ""
append "- Status: \`PASS\`"
append "- Processed events: \`${processed_events}\`"
append "- Accepted signal observation: \`${signal_line}\`"
append "- Overall result: \`PASS\`"

printf 'PASS: processed_events=%s; %s\n' "${processed_events}" "${signal_line}"
