#!/usr/bin/env bash
set -u

SCRIPT_DIR="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" >/dev/null 2>&1 && pwd)"
REPO_ROOT="$(cd -- "${SCRIPT_DIR}/../.." >/dev/null 2>&1 && pwd)"
RUN_ID="${AEGISAI_HELPER_PORTABILITY_RUN_ID:-helper_portability_$(hostname)_$(uname -r | tr '.-' '_')_$(date -u +%Y%m%dT%H%M%SZ)}"
ARTIFACT_DIR="${AEGISAI_HELPER_PORTABILITY_ARTIFACT_DIR:-${REPO_ROOT}/.cache/aegisai/helper_portability/${RUN_ID}}"
LOG_PATH="${AEGISAI_VERIFY_LOG:-${ARTIFACT_DIR}/helper_portability.md}"
HELPER_PATH="${AEGISAI_EBPF_HELPER:-${REPO_ROOT}/target/debug/aegisai-ebpf-helper}"
DAEMON_PATH="${AEGISAI_RUNTIME_DAEMON:-${REPO_ROOT}/target/debug/aegisai-runtime-daemon}"
BPFTRACE_PATH="${AEGISAI_BPFTRACE:-/usr/bin/bpftrace}"
SIGNAL_AVAILABILITY_JSON="${ARTIFACT_DIR}/helper_signal_availability.json"
SIGNAL_AVAILABILITY_CSV="${ARTIFACT_DIR}/helper_signal_availability.csv"
RAW_OFFCPU_SECONDS="${AEGISAI_HELPER_PORTABILITY_RAW_OFFCPU_SECONDS:-8}"
RAW_IO_SECONDS="${AEGISAI_HELPER_PORTABILITY_RAW_IO_SECONDS:-10}"
DAEMON_TIMEOUT_SECONDS="${AEGISAI_HELPER_PORTABILITY_DAEMON_TIMEOUT_SECONDS:-20}"
DAEMON_MAX_EVENTS="${AEGISAI_HELPER_PORTABILITY_DAEMON_MAX_EVENTS:-8}"
DAEMON_POLL_TIMEOUT_MS="${AEGISAI_HELPER_PORTABILITY_DAEMON_POLL_TIMEOUT_MS:-1000}"

TMP_ROOT=""
worker_pids=()
raw_count_result=0
signal_events_result=0

mkdir -p "${ARTIFACT_DIR}/bin" "${ARTIFACT_DIR}/logs" "${ARTIFACT_DIR}/work" "$(dirname -- "${LOG_PATH}")"
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

write_signal_availability() {
  local bucket="$1"
  local overall_result="$2"
  local reason="$3"
  local offcpu_raw="${4:-0}"
  local io_raw="${5:-0}"
  local offcpu_normalized="${6:-0}"
  local io_normalized="${7:-0}"

  python3 - "${SIGNAL_AVAILABILITY_JSON}" "${SIGNAL_AVAILABILITY_CSV}" \
    "${RUN_ID}" "${bucket}" "${overall_result}" "${reason}" \
    "${offcpu_raw}" "${io_raw}" "${offcpu_normalized}" "${io_normalized}" <<'PY'
import csv
import json
import sys
from datetime import datetime, timezone

(
    json_path,
    csv_path,
    run_id,
    bucket,
    overall_result,
    reason,
    offcpu_raw,
    io_raw,
    offcpu_normalized,
    io_normalized,
) = sys.argv[1:11]

counts = {
    "offcpu_time": {
        "raw_events": int(offcpu_raw or 0),
        "normalized_events": int(offcpu_normalized or 0),
    },
    "io_latency": {
        "raw_events": int(io_raw or 0),
        "normalized_events": int(io_normalized or 0),
    },
}
for signal, values in counts.items():
    values["phase5_planning_status"] = (
        "included"
        if bucket == "validated signal"
        and values["raw_events"] > 0
        and values["normalized_events"] > 0
        else "excluded"
    )
    if values["phase5_planning_status"] == "included":
        values["reason"] = "raw and normalized helper-backed events observed"
    elif bucket in ("helper unavailable", "tracepoint incompatible"):
        values["reason"] = bucket
    elif bucket == "no workload events":
        values["reason"] = "compatible helper diagnostics but zero raw or normalized workload events"
    else:
        values["reason"] = reason

payload = {
    "schema_version": "helper_signal_availability.v1",
    "generated_at": datetime.now(timezone.utc).strftime("%Y-%m-%dT%H:%M:%SZ"),
    "run_id": run_id,
    "bucket": bucket,
    "overall_result": overall_result,
    "reason": reason,
    "phase5_helper_backed_signals": counts,
}
with open(json_path, "w", encoding="utf-8") as handle:
    json.dump(payload, handle, indent=2, sort_keys=True)
    handle.write("\n")

with open(csv_path, "w", newline="", encoding="utf-8") as handle:
    writer = csv.DictWriter(
        handle,
        fieldnames=[
            "signal",
            "phase5_planning_status",
            "raw_events",
            "normalized_events",
            "reason",
        ],
    )
    writer.writeheader()
    for signal, values in counts.items():
        writer.writerow(
            {
                "signal": signal,
                "phase5_planning_status": values["phase5_planning_status"],
                "raw_events": values["raw_events"],
                "normalized_events": values["normalized_events"],
                "reason": values["reason"],
            }
        )
PY
}

finish_fail() {
  local reason="$1"

  write_signal_availability "script failure" "FAIL" "${reason}" 0 0 0 0
  append ""
  append "- Helper signal availability JSON: \`${SIGNAL_AVAILABILITY_JSON}\`"
  append "- Helper signal availability CSV: \`${SIGNAL_AVAILABILITY_CSV}\`"
  append "- Status: \`FAIL\`"
  append "- Failure reason: ${reason}"
  append "- Overall result: \`FAIL\`"
  printf 'FAIL: %s\n' "${reason}" >&2
  exit 1
}

finish_bucket_fail() {
  local bucket="$1"
  local reason="$2"

  write_signal_availability "${bucket}" "FAIL" "${reason}" 0 0 0 0
  append "- Final bucket: \`${bucket}\`"
  append "- Phase 5 helper-backed signals: \`excluded\`"
  append "- Helper signal availability JSON: \`${SIGNAL_AVAILABILITY_JSON}\`"
  append "- Helper signal availability CSV: \`${SIGNAL_AVAILABILITY_CSV}\`"
  append "- Status: \`FAIL\`"
  append "- Failure reason: ${reason}"
  append "- Overall result: \`FAIL\`"
  printf 'helper_portability_smoke=%s failure_reason=%q artifact_dir=%s\n' \
    "${bucket}" "${reason}" "${ARTIFACT_DIR}"
  printf 'FAIL: %s\n' "${reason}" >&2
  exit 1
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
}

trap cleanup EXIT

format_command() {
  printf '%q ' "$@"
}

require_command() {
  command -v "$1" >/dev/null 2>&1 || finish_fail "required command \`$1\` is not available."
}

describe_bpftrace() {
  local version

  if [[ ! -x "${BPFTRACE_PATH}" ]]; then
    printf 'unavailable'
    return
  fi

  version="$("${BPFTRACE_PATH}" --version 2>/dev/null | head -1 || true)"
  printf '%s' "${version:-unknown}"
}

read_compatibility_status() {
  local file="$1"
  local line=""
  local body
  local status=""

  line="$(grep '^[[:space:]]*helper compatibility: ' "${file}" | tail -1 || true)"
  if [[ -z "${line}" ]]; then
    return 1
  fi

  line="${line#"${line%%[![:space:]]*}"}"
  body="${line#helper compatibility: }"
  if [[ "${body}" == status=* ]]; then
    status="${body#status=}"
  elif [[ "${body}" == *"; status="* ]]; then
    status="${body#*; status=}"
  fi
  status="${status%%;*}"

  if [[ -z "${status}" ]]; then
    return 1
  fi
  printf '%s\n' "${status}"
}

require_compatible_status() {
  local status="$1"
  local context="$2"

  case "${status}" in
    compatible)
      ;;
    "helper unavailable" | "tracepoint incompatible")
      finish_bucket_fail "${status}" "${context} reported ${status}."
      ;;
    *)
      finish_fail "${context} reported unsupported helper compatibility status: ${status}"
      ;;
  esac
}

positive_integer() {
  [[ "$2" =~ ^[1-9][0-9]*$ ]] || finish_fail "$1 must be a positive integer; got \`$2\`."
}

write_runtime_config() {
  local path="$1"
  local signal="$2"

  cat >"${path}" <<EOF_RUNTIME
[target]
deployment_target = "linux"
kernel_min = "5.15"
cgroup_version = "v2"

[runtime]
primary_runtime = "ollama"
fallback_runtime = "python"

[selection]
mode = "process_name"
process_names = ["ollama"]
pid_allowlist = []

[collection]
focus_signals = [
  "${signal}"
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
EOF_RUNTIME
}

start_offcpu_workload() {
  local stdout="$1"
  local stderr="$2"
  local __result_var="$3"

  PATH="${ARTIFACT_DIR}/bin:${PATH}" "${ARTIFACT_DIR}/bin/ollama" -c 'import time
while True:
    time.sleep(0.002)
' >"${stdout}" 2>"${stderr}" &
  local spawned_pid="$!"
  worker_pids+=("${spawned_pid}")
  printf -v "${__result_var}" '%s' "${spawned_pid}"
}

start_io_workload() {
  local path="$1"
  local stdout="$2"
  local stderr="$3"
  local __result_var="$4"

  AEGISAI_IO_WORKLOAD_PATH="${path}" PATH="${ARTIFACT_DIR}/bin:${PATH}" "${ARTIFACT_DIR}/bin/ollama" -c 'import os, pathlib
path = pathlib.Path(os.environ["AEGISAI_IO_WORKLOAD_PATH"])
block = b"x" * 1048576
while True:
    with path.open("wb") as handle:
        for _ in range(16):
            handle.write(block)
        handle.flush()
        os.fsync(handle.fileno())
' >"${stdout}" 2>"${stderr}" &
  local spawned_pid="$!"
  worker_pids+=("${spawned_pid}")
  printf -v "${__result_var}" '%s' "${spawned_pid}"
}

run_raw_stream() {
  local signal="$1"
  local seconds="$2"
  local work_dir="${ARTIFACT_DIR}/work/raw_${signal}"
  local helper_args=()
  local pid=""

  mkdir -p "${work_dir}"
  if [[ "${signal}" == "offcpu_time" ]]; then
    start_offcpu_workload "${work_dir}/workload.stdout" "${work_dir}/workload.stderr" pid
    sleep 0.5
    cat "/proc/${pid}/comm" >"${work_dir}/workload.comm"
    helper_args=(stream --offcpu --pid "${pid}")
  else
    start_io_workload "${work_dir}/io-workload.bin" "${work_dir}/workload.stdout" "${work_dir}/workload.stderr" pid
    sleep 0.5
    cat "/proc/${pid}/comm" >"${work_dir}/workload.comm"
    helper_args=(stream --io --process-name ollama)
  fi

  append "- Raw ${signal} helper command: \`$(format_command timeout "${seconds}s" "${HELPER_PATH}" "${helper_args[@]}")\`"
  set +e
  PATH="${ARTIFACT_DIR}/bin:${PATH}" AEGISAI_BPFTRACE="${BPFTRACE_PATH}" timeout "${seconds}s" "${HELPER_PATH}" "${helper_args[@]}" >"${work_dir}/helper.stdout" 2>"${work_dir}/helper.stderr"
  local status=$?
  set -e
  printf '%s\n' "${status}" >"${work_dir}/helper.status"

  local raw_count
  raw_count="$(grep -c "^aegisai_probe signal=${signal} " "${work_dir}/helper.stdout" || true)"
  append "- Raw ${signal} status: \`${status}\`; events: \`${raw_count}\`; stderr lines: \`$(wc -l <"${work_dir}/helper.stderr")\`"
  raw_count_result="${raw_count}"
}

run_daemon_signal() {
  local signal="$1"
  local work_dir="${ARTIFACT_DIR}/work/daemon_${signal}"
  local pid=""

  mkdir -p "${work_dir}"
  if [[ "${signal}" == "offcpu_time" ]]; then
    start_offcpu_workload "${work_dir}/workload.stdout" "${work_dir}/workload.stderr" pid
  else
    start_io_workload "${work_dir}/io-workload.bin" "${work_dir}/workload.stdout" "${work_dir}/workload.stderr" pid
  fi
  sleep 0.8
  cat "/proc/${pid}/comm" >"${work_dir}/workload.comm"

  TMP_ROOT="$(mktemp -d -t aegisai-helper-portability.XXXXXX)" ||
    finish_fail "failed to create temporary config root."
  cp -R "${REPO_ROOT}/configs" "${TMP_ROOT}/configs" ||
    finish_fail "failed to copy repository configs."
  write_runtime_config "${TMP_ROOT}/configs/runtime/runtime.example.toml" "${signal}"

  local command=(
    timeout "${DAEMON_TIMEOUT_SECONDS}s"
    "${ARTIFACT_DIR}/bin/aegisai-runtime-daemon"
    --repo-root "${TMP_ROOT}"
    --source linux
    --metadata procfs
    --actuator-backend linux-skeleton
    --allow-partial-probes
    --probe-poll-timeout-ms "${DAEMON_POLL_TIMEOUT_MS}"
    --batch-size 16
    --max-events "${DAEMON_MAX_EVENTS}"
    --drain-ms 0
    --verification-log "${work_dir}/daemon_verification.md"
  )
  append "- Daemon ${signal} command: \`$(format_command "${command[@]}")\`"

  set +e
  PATH="${ARTIFACT_DIR}/bin:${PATH}" AEGISAI_EBPF_HELPER="${HELPER_PATH}" AEGISAI_BPFTRACE="${BPFTRACE_PATH}" "${command[@]}" >"${work_dir}/daemon.stdout" 2>"${work_dir}/daemon.stderr"
  local status=$?
  set -e
  printf '%s\n' "${status}" >"${work_dir}/daemon.status"

  local processed_events
  processed_events="$(awk -F': ' '/^processed_events:/ {print $2}' "${work_dir}/daemon.stdout" | tail -1)"
  local signal_events
  signal_events="$(awk -v signal="${signal}" '$1 == signal ":" {for (i=1; i<=NF; i++) if ($i ~ /^events=/) {sub(/^events=/, "", $i); print $i}}' "${work_dir}/daemon.stdout" | tail -1)"
  append "- Daemon ${signal} status: \`${status}\`; processed_events: \`${processed_events:-0}\`; normalized ${signal} events: \`${signal_events:-0}\`; stderr lines: \`$(wc -l <"${work_dir}/daemon.stderr")\`"
  append "- Daemon ${signal} source diagnostics:"
  grep '^  helper compatibility:' "${work_dir}/daemon.stdout" >>"${LOG_PATH}" || true

  if [[ "${status}" != "0" ]]; then
    append "- Daemon ${signal} stderr:"
    append_block "${work_dir}/daemon.stderr"
  fi

  local daemon_compatibility_status
  daemon_compatibility_status="$(read_compatibility_status "${work_dir}/daemon.stdout")" ||
    finish_fail "daemon ${signal} did not produce helper compatibility diagnostics."
  require_compatible_status "${daemon_compatibility_status}" "daemon ${signal} source diagnostics"

  signal_events_result="${signal_events:-0}"
}

positive_integer AEGISAI_HELPER_PORTABILITY_RAW_OFFCPU_SECONDS "${RAW_OFFCPU_SECONDS}"
positive_integer AEGISAI_HELPER_PORTABILITY_RAW_IO_SECONDS "${RAW_IO_SECONDS}"
positive_integer AEGISAI_HELPER_PORTABILITY_DAEMON_TIMEOUT_SECONDS "${DAEMON_TIMEOUT_SECONDS}"
positive_integer AEGISAI_HELPER_PORTABILITY_DAEMON_MAX_EVENTS "${DAEMON_MAX_EVENTS}"
positive_integer AEGISAI_HELPER_PORTABILITY_DAEMON_POLL_TIMEOUT_MS "${DAEMON_POLL_TIMEOUT_MS}"
require_command timeout
require_command grep
require_command awk

if [[ "$(uname -s)" != "Linux" ]]; then
  finish_fail "helper portability smoke requires Linux."
fi
if [[ ! -x "${HELPER_PATH}" ]]; then
  finish_fail "helper path is not executable: ${HELPER_PATH}"
fi
if [[ ! -x "${DAEMON_PATH}" ]]; then
  finish_fail "runtime daemon binary is missing; run cargo build -p aegisai-runtime-daemon."
fi

install -m 0755 "${DAEMON_PATH}" "${ARTIFACT_DIR}/bin/aegisai-runtime-daemon" ||
  finish_fail "failed to copy runtime daemon."
ln -sf /usr/bin/python3 "${ARTIFACT_DIR}/bin/ollama" ||
  finish_fail "failed to create ollama workload shim."

append ""
append "### $(date -u +%Y-%m-%dT%H:%M:%SZ) - Helper Portability Smoke"
append ""
append "- Run ID: \`${RUN_ID}\`"
append "- Artifact directory: \`${ARTIFACT_DIR}\`"
append "- Host: \`$(uname -a)\`"
if command -v lsb_release >/dev/null 2>&1; then
  distro="$(lsb_release -ds 2>/dev/null || printf 'unknown')"
else
  distro="$(. /etc/os-release 2>/dev/null && printf '%s' "${PRETTY_NAME:-unknown}")"
fi
append "- Distro: \`${distro}\`"
append "- bpftrace: \`$(describe_bpftrace)\`"
append "- tracefs root: \`$(findmnt -no TARGET -T /sys/kernel/tracing 2>/dev/null || printf 'unavailable')\`"
append "- Helper path: \`${HELPER_PATH}\`"
append "- Runtime daemon path: \`${DAEMON_PATH}\`"

PATH="${ARTIFACT_DIR}/bin:${PATH}" AEGISAI_BPFTRACE="${BPFTRACE_PATH}" "${HELPER_PATH}" compatibility --offcpu --io >"${ARTIFACT_DIR}/logs/helper_compatibility.stdout" 2>"${ARTIFACT_DIR}/logs/helper_compatibility.stderr" ||
  finish_fail "helper compatibility command failed."
append "- Helper compatibility command: \`$(format_command "${HELPER_PATH}" compatibility --offcpu --io)\`"
append_block "${ARTIFACT_DIR}/logs/helper_compatibility.stdout"
helper_compatibility_status="$(read_compatibility_status "${ARTIFACT_DIR}/logs/helper_compatibility.stdout")" ||
  finish_fail "helper compatibility command did not produce compatibility diagnostics."
require_compatible_status "${helper_compatibility_status}" "helper compatibility preflight"

run_raw_stream offcpu_time "${RAW_OFFCPU_SECONDS}"
offcpu_raw="${raw_count_result}"
run_raw_stream io_latency "${RAW_IO_SECONDS}"
io_raw="${raw_count_result}"
run_daemon_signal offcpu_time
offcpu_normalized="${signal_events_result}"
run_daemon_signal io_latency
io_normalized="${signal_events_result}"

bucket="validated signal"
if (( offcpu_raw == 0 || io_raw == 0 || offcpu_normalized == 0 || io_normalized == 0 )); then
  bucket="no workload events"
fi

write_signal_availability "${bucket}" "PASS" "helper portability smoke completed" "${offcpu_raw}" "${io_raw}" "${offcpu_normalized}" "${io_normalized}"

append "- Final bucket: \`${bucket}\`"
if [[ "${bucket}" == "validated signal" ]]; then
  append "- Phase 5 helper-backed signals: \`included\`"
else
  append "- Phase 5 helper-backed signals: \`excluded\`"
fi
append "- Helper signal availability JSON: \`${SIGNAL_AVAILABILITY_JSON}\`"
append "- Helper signal availability CSV: \`${SIGNAL_AVAILABILITY_CSV}\`"
append "- Overall result: \`PASS\`"

printf 'helper_portability_smoke=%s offcpu_raw=%s io_raw=%s offcpu_normalized=%s io_normalized=%s artifact_dir=%s\n' \
  "${bucket}" "${offcpu_raw}" "${io_raw}" "${offcpu_normalized}" "${io_normalized}" "${ARTIFACT_DIR}"
