#!/usr/bin/env bash
set -u

SCRIPT_DIR="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" >/dev/null 2>&1 && pwd)"
REPO_ROOT="$(cd -- "${SCRIPT_DIR}/../.." >/dev/null 2>&1 && pwd)"
LOG_PATH="${AEGISAI_VERIFY_LOG:-${REPO_ROOT}/docs/verification_log.md}"
RUN_ID="${AEGISAI_AB_RUN_ID:-$(date -u +%Y%m%dT%H%M%SZ)}"
ARTIFACT_DIR="${AEGISAI_AB_ARTIFACT_DIR:-${REPO_ROOT}/.cache/aegisai/inference_tail_guard/${RUN_ID}}"

MODEL="${AEGISAI_OLLAMA_MODEL:-qwen2.5:0.5b}"
NUM_PREDICT="${AEGISAI_OLLAMA_NUM_PREDICT:-96}"
TEMPERATURE="${AEGISAI_OLLAMA_TEMPERATURE:-0}"
SEED="${AEGISAI_OLLAMA_SEED:-42}"
KEEP_ALIVE="${AEGISAI_OLLAMA_KEEP_ALIVE:-5m}"
OLLAMA_API_URL="${AEGISAI_OLLAMA_API_URL:-http://127.0.0.1:11434/api/generate}"
PROMPT="${AEGISAI_OLLAMA_PROMPT:-请用两句中文说明 AegisAI 正在进行实时推理 A/B harness，并补一句当前目标是观察尾延迟。}"

AB_MODES="${AEGISAI_AB_MODES:-baseline,noop_observation,dry_run}"
SAMPLES="${AEGISAI_AB_SAMPLES:-12}"
CONCURRENCY="${AEGISAI_AB_CONCURRENCY:-2}"
REQUEST_TIMEOUT="${AEGISAI_OLLAMA_REQUEST_TIMEOUT:-120}"
STRESS_CPU="${AEGISAI_STRESS_CPU:-2}"
STRESS_IO="${AEGISAI_STRESS_IO:-0}"
STRESS_HDD="${AEGISAI_STRESS_HDD:-0}"
STRESS_HDD_BYTES="${AEGISAI_STRESS_HDD_BYTES:-128M}"
STRESS_TEMP_PATH="${AEGISAI_STRESS_TEMP_PATH:-${ARTIFACT_DIR}/stress-tmp}"
STRESS_TIMEOUT="${AEGISAI_STRESS_TIMEOUT:-0}"
REQUIRE_STRESS="${AEGISAI_REQUIRE_STRESS:-1}"
STRESS_START_DELAY="${AEGISAI_STRESS_START_DELAY:-0.5}"
DAEMON_START_DELAY="${AEGISAI_DAEMON_START_DELAY:-0.5}"
DAEMON_WAIT_TIMEOUT="${AEGISAI_DAEMON_WAIT_TIMEOUT:-30}"
DAEMON_POLL_TIMEOUT_MS="${AEGISAI_DAEMON_POLL_TIMEOUT_MS:-3000}"
DAEMON_BATCH_SIZE="${AEGISAI_DAEMON_BATCH_SIZE:-32}"
DAEMON_TICK_MS="${AEGISAI_DAEMON_TICK_MS:-200}"
DAEMON_DRAIN_MS="${AEGISAI_DAEMON_DRAIN_MS:-5000}"
MODE_COOLDOWN="${AEGISAI_AB_MODE_COOLDOWN:-1}"
LIVE_CONFIRM="${AEGISAI_CONFIRM_LIVE_ACTUATOR:-0}"
LIVE_PID_ALLOWLIST="${AEGISAI_LIVE_PID_ALLOWLIST:-}"
LIVE_ENABLE_AFFINITY="${AEGISAI_ENABLE_LIVE_AFFINITY:-0}"

mkdir -p "$(dirname -- "${LOG_PATH}")" "${ARTIFACT_DIR}"
touch "${LOG_PATH}"

SAMPLES_CSV="${ARTIFACT_DIR}/samples.csv"
MODE_COUNTS_CSV="${ARTIFACT_DIR}/mode_counts.csv"
SUMMARY_CSV="${ARTIFACT_DIR}/summary.csv"
SUMMARY_MD="${ARTIFACT_DIR}/summary.md"
RUN_ENV="${ARTIFACT_DIR}/run.env"
PAYLOAD_STREAM="${ARTIFACT_DIR}/payload.stream.json"
PAYLOAD_WARMUP="${ARTIFACT_DIR}/payload.warmup.json"

daemon_pid=""
stress_pid=""
overall_status=0

declare -a SELECTED_MODES=()
declare -A MODE_BACKEND=(
  [baseline]="none"
  [noop_observation]="noop"
  [dry_run]="linux-command-dry-run"
  [live_guarded]="linux-command"
)
declare -A MODE_LABEL=(
  [baseline]="baseline"
  [noop_observation]="noop observation"
  [dry_run]="dry-run guarded"
  [live_guarded]="live guarded"
)

usage() {
  cat <<'USAGE'
Usage: bash bench/scripts/inference_tail_guard_ollama_smoke.sh

Runs the reproducible Inference Tail Guard Ollama A/B harness.

Default modes:
  baseline,noop_observation,dry_run

Common overrides:
  AEGISAI_AB_MODES=baseline,noop_observation,dry_run
  AEGISAI_AB_MODES=baseline,noop_observation,dry_run,live_guarded
  AEGISAI_CONFIRM_LIVE_ACTUATOR=1
  AEGISAI_LIVE_PID_ALLOWLIST=1234
  AEGISAI_AB_SAMPLES=12
  AEGISAI_AB_CONCURRENCY=2
  AEGISAI_STRESS_CPU=2
  AEGISAI_STRESS_IO=0
  AEGISAI_STRESS_HDD=0
  AEGISAI_STRESS_HDD_BYTES=128M
  AEGISAI_STRESS_TIMEOUT=0
  AEGISAI_OLLAMA_MODEL=qwen2.5:0.5b
  AEGISAI_AB_ARTIFACT_DIR=/path/to/results

Metrics:
  TTFT is curl time_starttransfer against streaming Ollama responses.
  P95/P99/jitter use end-to-end streaming request total latency.
  Jitter is the sample standard deviation of total latency.
USAGE
}

if [[ "${1:-}" == "--help" || "${1:-}" == "-h" ]]; then
  usage
  exit 0
fi

append() {
  printf '%s\n' "$*" >>"${LOG_PATH}"
}

append_file() {
  local file="$1"
  if [[ -s "${file}" ]]; then
    cat "${file}" >>"${LOG_PATH}"
  else
    append "_empty_"
  fi
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

is_positive_uint() {
  is_uint "$1" && [[ "$1" -gt 0 ]]
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

to_ms() {
  local seconds="${1:-0}"
  awk -v value="${seconds}" 'BEGIN { printf "%.3f", value * 1000 }'
}

stress_command_label() {
  local -a parts=()

  if [[ "${STRESS_CPU}" -gt 0 ]]; then
    parts+=(--cpu "${STRESS_CPU}")
  fi
  if [[ "${STRESS_IO}" -gt 0 ]]; then
    parts+=(--io "${STRESS_IO}")
  fi
  if [[ "${STRESS_HDD}" -gt 0 ]]; then
    parts+=(--hdd "${STRESS_HDD}" --hdd-bytes "${STRESS_HDD_BYTES}" --temp-path "${STRESS_TEMP_PATH}")
  fi
  if [[ "${STRESS_TIMEOUT}" -gt 0 ]]; then
    parts+=(--timeout "${STRESS_TIMEOUT}s")
  fi

  if [[ "${#parts[@]}" -eq 0 ]]; then
    printf 'disabled'
  else
    printf 'stress-ng %s' "${parts[*]}"
  fi
}

cleanup() {
  if [[ -n "${daemon_pid}" ]]; then
    kill "${daemon_pid}" >/dev/null 2>&1 || true
  fi
  if [[ -n "${stress_pid}" ]]; then
    kill "${stress_pid}" >/dev/null 2>&1 || true
  fi
}

trap cleanup EXIT

parse_modes() {
  local raw="${AB_MODES//,/ }"
  local mode canonical

  for mode in ${raw}; do
    case "${mode}" in
      baseline)
        canonical="baseline"
        ;;
      noop|noop_observation|noop-observation)
        canonical="noop_observation"
        ;;
      dry_run|dry-run|dryrun)
        canonical="dry_run"
        ;;
      live|live_guarded|live-guarded|linux-command)
        canonical="live_guarded"
        ;;
      *)
        append "- Invalid mode: \`${mode}\`"
        printf 'Invalid AEGISAI_AB_MODES entry: %s\n' "${mode}" >&2
        exit 1
        ;;
    esac

    if [[ " ${SELECTED_MODES[*]} " != *" ${canonical} "* ]]; then
      SELECTED_MODES+=("${canonical}")
    fi
  done

  if [[ "${#SELECTED_MODES[@]}" -eq 0 ]]; then
    append "- Invalid mode set: no modes selected."
    printf 'No A/B modes selected.\n' >&2
    exit 1
  fi
}

require_command() {
  local command_name="$1"
  if ! has_command "${command_name}"; then
    append "- Requirement: \`${command_name}\`"
    append "- Status: \`FAIL\`"
    append "- Reason: command is not installed or is not on PATH."
    printf 'Missing required command: %s\n' "${command_name}" >&2
    exit 1
  fi
}

validate_config() {
  if ! is_positive_uint "${SAMPLES}"; then
    append "- Invalid sample count: \`${SAMPLES}\`"
    printf 'AEGISAI_AB_SAMPLES must be a positive integer.\n' >&2
    exit 1
  fi
  if [[ "${SAMPLES}" -lt 4 ]]; then
    append "- Invalid sample count: \`${SAMPLES}\`"
    append "- Reason: formal A/B runs require at least 4 samples per mode."
    printf 'AEGISAI_AB_SAMPLES must be at least 4.\n' >&2
    exit 1
  fi
  if ! is_positive_uint "${CONCURRENCY}"; then
    append "- Invalid concurrency: \`${CONCURRENCY}\`"
    printf 'AEGISAI_AB_CONCURRENCY must be a positive integer.\n' >&2
    exit 1
  fi
  if [[ "${CONCURRENCY}" -gt "${SAMPLES}" ]]; then
    append "- Invalid concurrency: \`${CONCURRENCY}\` exceeds samples \`${SAMPLES}\`."
    printf 'AEGISAI_AB_CONCURRENCY cannot exceed AEGISAI_AB_SAMPLES.\n' >&2
    exit 1
  fi
  if (( SAMPLES % CONCURRENCY != 0 )); then
    append "- Invalid run shape: samples \`${SAMPLES}\` must be divisible by concurrency \`${CONCURRENCY}\`."
    printf 'AEGISAI_AB_SAMPLES must be divisible by AEGISAI_AB_CONCURRENCY.\n' >&2
    exit 1
  fi
  if ! is_uint "${STRESS_CPU}"; then
    append "- Invalid CPU interference strength: \`${STRESS_CPU}\`"
    printf 'AEGISAI_STRESS_CPU must be a non-negative integer.\n' >&2
    exit 1
  fi
  if ! is_uint "${STRESS_IO}"; then
    append "- Invalid I/O sync interference strength: \`${STRESS_IO}\`"
    printf 'AEGISAI_STRESS_IO must be a non-negative integer.\n' >&2
    exit 1
  fi
  if ! is_uint "${STRESS_HDD}"; then
    append "- Invalid I/O disk interference strength: \`${STRESS_HDD}\`"
    printf 'AEGISAI_STRESS_HDD must be a non-negative integer.\n' >&2
    exit 1
  fi
  if ! is_uint "${STRESS_TIMEOUT}"; then
    append "- Invalid stress timeout: \`${STRESS_TIMEOUT}\`"
    printf 'AEGISAI_STRESS_TIMEOUT must be a non-negative integer.\n' >&2
    exit 1
  fi
  if ! is_positive_uint "${REQUEST_TIMEOUT}"; then
    append "- Invalid request timeout: \`${REQUEST_TIMEOUT}\`"
    printf 'AEGISAI_OLLAMA_REQUEST_TIMEOUT must be a positive integer.\n' >&2
    exit 1
  fi
  if ! is_positive_uint "${DAEMON_WAIT_TIMEOUT}"; then
    append "- Invalid daemon wait timeout: \`${DAEMON_WAIT_TIMEOUT}\`"
    printf 'AEGISAI_DAEMON_WAIT_TIMEOUT must be a positive integer.\n' >&2
    exit 1
  fi
  if [[ " ${SELECTED_MODES[*]} " == *" live_guarded "* ]]; then
    if [[ "${LIVE_CONFIRM}" != "1" ]]; then
      append "- Invalid live actuator confirmation: \`${LIVE_CONFIRM}\`"
      append "- Reason: live_guarded requires \`AEGISAI_CONFIRM_LIVE_ACTUATOR=1\`."
      printf 'live_guarded requires AEGISAI_CONFIRM_LIVE_ACTUATOR=1.\n' >&2
      exit 1
    fi
    if ! is_pid_allowlist "${LIVE_PID_ALLOWLIST}"; then
      append "- Invalid live PID allowlist: \`${LIVE_PID_ALLOWLIST}\`"
      append "- Reason: live_guarded requires \`AEGISAI_LIVE_PID_ALLOWLIST\` with one or more positive PIDs."
      printf 'live_guarded requires AEGISAI_LIVE_PID_ALLOWLIST with one or more positive PIDs.\n' >&2
      exit 1
    fi
    if [[ "${LIVE_ENABLE_AFFINITY}" != "0" && "${LIVE_ENABLE_AFFINITY}" != "1" ]]; then
      append "- Invalid live affinity flag: \`${LIVE_ENABLE_AFFINITY}\`"
      printf 'AEGISAI_ENABLE_LIVE_AFFINITY must be 0 or 1.\n' >&2
      exit 1
    fi
  fi
}

write_payload() {
  local stream="$1"
  local output="$2"

  AEGISAI_PAYLOAD_STREAM="${stream}" \
    AEGISAI_PAYLOAD_OUTPUT="${output}" \
    AEGISAI_PAYLOAD_MODEL="${MODEL}" \
    AEGISAI_PAYLOAD_PROMPT="${PROMPT}" \
    AEGISAI_PAYLOAD_NUM_PREDICT="${NUM_PREDICT}" \
    AEGISAI_PAYLOAD_TEMPERATURE="${TEMPERATURE}" \
    AEGISAI_PAYLOAD_SEED="${SEED}" \
    AEGISAI_PAYLOAD_KEEP_ALIVE="${KEEP_ALIVE}" \
    AEGISAI_PAYLOAD_NUM_THREAD="${AEGISAI_OLLAMA_NUM_THREAD:-}" \
    python3 - <<'PY'
import json
import os

options = {
    "num_predict": int(os.environ["AEGISAI_PAYLOAD_NUM_PREDICT"]),
    "temperature": float(os.environ["AEGISAI_PAYLOAD_TEMPERATURE"]),
    "seed": int(os.environ["AEGISAI_PAYLOAD_SEED"]),
}
num_thread = os.environ.get("AEGISAI_PAYLOAD_NUM_THREAD", "")
if num_thread:
    options["num_thread"] = int(num_thread)

payload = {
    "model": os.environ["AEGISAI_PAYLOAD_MODEL"],
    "prompt": os.environ["AEGISAI_PAYLOAD_PROMPT"],
    "stream": os.environ["AEGISAI_PAYLOAD_STREAM"] == "true",
    "keep_alive": os.environ["AEGISAI_PAYLOAD_KEEP_ALIVE"],
    "options": options,
}

with open(os.environ["AEGISAI_PAYLOAD_OUTPUT"], "w", encoding="utf-8") as handle:
    json.dump(payload, handle, ensure_ascii=False, separators=(",", ":"))
PY
}

prompt_sha256() {
  AEGISAI_PROMPT_TEXT="${PROMPT}" python3 - <<'PY'
import hashlib
import os

print(hashlib.sha256(os.environ["AEGISAI_PROMPT_TEXT"].encode("utf-8")).hexdigest())
PY
}

write_run_env() {
  local prompt_hash="$1"

  {
    printf 'run_id=%s\n' "${RUN_ID}"
    printf 'repo_root=%s\n' "${REPO_ROOT}"
    printf 'log_path=%s\n' "${LOG_PATH}"
    printf 'artifact_dir=%s\n' "${ARTIFACT_DIR}"
    printf 'modes=%s\n' "${SELECTED_MODES[*]}"
    printf 'model=%s\n' "${MODEL}"
    printf 'prompt_sha256=%s\n' "${prompt_hash}"
    printf 'prompt=%s\n' "${PROMPT}"
    printf 'ollama_api_url=%s\n' "${OLLAMA_API_URL}"
    printf 'num_predict=%s\n' "${NUM_PREDICT}"
    printf 'temperature=%s\n' "${TEMPERATURE}"
    printf 'seed=%s\n' "${SEED}"
    printf 'keep_alive=%s\n' "${KEEP_ALIVE}"
    printf 'samples_per_mode=%s\n' "${SAMPLES}"
    printf 'concurrency=%s\n' "${CONCURRENCY}"
    printf 'stress_cpu=%s\n' "${STRESS_CPU}"
    printf 'stress_io=%s\n' "${STRESS_IO}"
    printf 'stress_hdd=%s\n' "${STRESS_HDD}"
    printf 'stress_hdd_bytes=%s\n' "${STRESS_HDD_BYTES}"
    printf 'stress_temp_path=%s\n' "${STRESS_TEMP_PATH}"
    printf 'stress_timeout_s=%s\n' "${STRESS_TIMEOUT}"
    printf 'stress_command=%s\n' "$(stress_command_label)"
    printf 'daemon_poll_timeout_ms=%s\n' "${DAEMON_POLL_TIMEOUT_MS}"
    printf 'daemon_batch_size=%s\n' "${DAEMON_BATCH_SIZE}"
    printf 'daemon_tick_ms=%s\n' "${DAEMON_TICK_MS}"
    printf 'daemon_drain_ms=%s\n' "${DAEMON_DRAIN_MS}"
    printf 'live_confirm=%s\n' "${LIVE_CONFIRM}"
    printf 'live_pid_allowlist=%s\n' "${LIVE_PID_ALLOWLIST}"
    printf 'live_enable_affinity=%s\n' "${LIVE_ENABLE_AFFINITY}"
    printf 'kernel=%s\n' "$(uname -srmo 2>/dev/null || true)"
    printf 'cpu_count=%s\n' "$(getconf _NPROCESSORS_ONLN 2>/dev/null || true)"
    printf 'ollama_version=%s\n' "$(ollama --version 2>/dev/null || true)"
    printf 'cargo_version=%s\n' "$(cargo --version 2>/dev/null || true)"
    printf 'curl_version=%s\n' "$(curl --version 2>/dev/null | head -n 1 || true)"
    printf 'stress_ng_version=%s\n' "$(stress-ng --version 2>/dev/null | head -n 1 || true)"
  } >"${RUN_ENV}"
}

run_http_payload() {
  local payload="$1"
  local body="$2"
  local writeout="$3"
  local error_log="$4"

  curl -sS -N \
    --max-time "${REQUEST_TIMEOUT}" \
    -X POST "${OLLAMA_API_URL}" \
    -H 'Content-Type: application/json' \
    --data-binary @"${payload}" \
    -o "${body}" \
    -w $'http_code=%{http_code}\ntime_starttransfer=%{time_starttransfer}\ntime_total=%{time_total}\n' \
    >"${writeout}" 2>"${error_log}"
}

write_ollama_request_sample() {
  local mode="$1"
  local sample="$2"
  local mode_dir="$3"
  local backend="${MODE_BACKEND[${mode}]}"
  local body="${mode_dir}/sample_${sample}.jsonl"
  local writeout="${mode_dir}/sample_${sample}.curl"
  local error_log="${mode_dir}/sample_${sample}.err"
  local row="${mode_dir}/sample_${sample}.csv"
  local curl_status http_code ttft_s total_s ttft_ms total_ms stream_done body_bytes error_bytes

  run_http_payload "${PAYLOAD_STREAM}" "${body}" "${writeout}" "${error_log}"
  curl_status=$?
  http_code="$(sed -n 's/^http_code=//p' "${writeout}" | head -n 1)"
  ttft_s="$(sed -n 's/^time_starttransfer=//p' "${writeout}" | head -n 1)"
  total_s="$(sed -n 's/^time_total=//p' "${writeout}" | head -n 1)"
  ttft_ms="$(to_ms "${ttft_s:-0}")"
  total_ms="$(to_ms "${total_s:-0}")"
  stream_done=0
  if grep -q '"done"[[:space:]]*:[[:space:]]*true' "${body}" 2>/dev/null; then
    stream_done=1
  fi
  body_bytes="$(wc -c <"${body}" | tr -d '[:space:]')"
  error_bytes="$(wc -c <"${error_log}" | tr -d '[:space:]')"

  printf '%s,%s,%s,%s,%s,%s,%s,%s,%s,%s\n' \
    "${sample}" \
    "${mode}" \
    "${backend}" \
    "${curl_status}" \
    "${http_code:-000}" \
    "${stream_done}" \
    "${ttft_ms}" \
    "${total_ms}" \
    "${body_bytes}" \
    "${error_bytes}" \
    >"${row}"

  if [[ "${curl_status}" -eq 0 && "${http_code:-000}" == "200" && "${stream_done}" -eq 1 ]]; then
    return 0
  fi
  return 1
}

run_request_batch() {
  local mode="$1"
  local mode_dir="$2"
  local next_sample=1
  local slot pid request_status=0
  local -a pids=()

  while [[ "${next_sample}" -le "${SAMPLES}" ]]; do
    pids=()
    for ((slot = 0; slot < CONCURRENCY && next_sample <= SAMPLES; slot += 1)); do
      write_ollama_request_sample "${mode}" "${next_sample}" "${mode_dir}" &
      pids+=("$!")
      next_sample=$((next_sample + 1))
    done

    for pid in "${pids[@]}"; do
      if ! wait "${pid}"; then
        request_status=1
      fi
    done
  done

  return "${request_status}"
}

append_mode_samples() {
  local mode_dir="$1"
  local sample

  for ((sample = 1; sample <= SAMPLES; sample += 1)); do
    if [[ -s "${mode_dir}/sample_${sample}.csv" ]]; then
      cat "${mode_dir}/sample_${sample}.csv" >>"${SAMPLES_CSV}"
    else
      printf '%s,%s,%s,missing,000,0,0.000,0.000,0,0\n' \
        "${sample}" "unknown" "unknown" >>"${SAMPLES_CSV}"
    fi
  done
}

successful_samples_for_mode() {
  local mode="$1"
  awk -F, -v mode="${mode}" '
    NR > 1 && $2 == mode && $4 == "0" && $5 == "200" && $6 == "1" { count += 1 }
    END { print count + 0 }
  ' "${SAMPLES_CSV}"
}

extract_daemon_number() {
  local key="$1"
  local file="$2"
  sed -n "s/^${key}: \([0-9][0-9]*\)$/\1/p" "${file}" | head -n 1
}

extract_trigger_count() {
  local file="$1"
  sed -n 's/^  inference_tail_guard: \([0-9][0-9]*\)$/\1/p' "${file}" | head -n 1
}

count_action_errors() {
  local file="$1"
  grep -Ec 'backend\.apply\.apply\.failed_count=[1-9][0-9]*|backend\.apply\.apply\.[0-9]+\.status=error|backend\.rollback\.rollback\.[0-9]+\.status=error|backend\.rollback\.rollback\.failed=' "${file}" 2>/dev/null || true
}

start_stress() {
  local stress_log="$1"

  stress_pid=""
  if [[ "${STRESS_CPU}" -eq 0 && "${STRESS_IO}" -eq 0 && "${STRESS_HDD}" -eq 0 ]]; then
    printf '%s\n' 'stress-ng disabled because all AEGISAI_STRESS_* worker counts are 0.' >"${stress_log}"
    return 0
  fi
  if ! has_command stress-ng; then
    printf '%s\n' 'stress-ng is not installed or is not on PATH.' >"${stress_log}"
    return 0
  fi

  local -a stress_args=()
  if [[ "${STRESS_CPU}" -gt 0 ]]; then
    stress_args+=(--cpu "${STRESS_CPU}")
  fi
  if [[ "${STRESS_IO}" -gt 0 ]]; then
    stress_args+=(--io "${STRESS_IO}")
  fi
  if [[ "${STRESS_HDD}" -gt 0 ]]; then
    mkdir -p "${STRESS_TEMP_PATH}"
    stress_args+=(--hdd "${STRESS_HDD}" --hdd-bytes "${STRESS_HDD_BYTES}" --temp-path "${STRESS_TEMP_PATH}")
  fi
  if [[ "${STRESS_TIMEOUT}" -gt 0 ]]; then
    stress_args+=(--timeout "${STRESS_TIMEOUT}s")
  fi
  stress-ng "${stress_args[@]}" >"${stress_log}" 2>&1 &
  stress_pid=$!
  sleep "${STRESS_START_DELAY}"
}

stop_stress() {
  local stress_status_var="$1"
  local stress_exhausted_var="$2"
  local status

  if [[ -z "${stress_pid}" ]]; then
    printf -v "${stress_status_var}" '%s' "disabled"
    printf -v "${stress_exhausted_var}" '%s' "0"
    return 0
  fi

  if kill -0 "${stress_pid}" >/dev/null 2>&1; then
    kill "${stress_pid}" >/dev/null 2>&1 || true
    wait "${stress_pid}" >/dev/null 2>&1
    status=$?
    printf -v "${stress_status_var}" '%s' "terminated:${status}"
    printf -v "${stress_exhausted_var}" '%s' "0"
  else
    wait "${stress_pid}" >/dev/null 2>&1
    status=$?
    printf -v "${stress_status_var}" '%s' "exited:${status}"
    printf -v "${stress_exhausted_var}" '%s' "1"
  fi

  stress_pid=""
}

start_daemon() {
  local backend="$1"
  local daemon_log="$2"
  local -a live_args=()

  daemon_pid=""
  if [[ "${backend}" == "none" ]]; then
    printf '%s\n' 'daemon disabled for baseline mode.' >"${daemon_log}"
    return 0
  fi
  if [[ "${backend}" == "linux-command" ]]; then
    live_args=(
      --confirm-live-actuator
      --live-pid-allowlist "${LIVE_PID_ALLOWLIST}"
    )
    if [[ "${LIVE_ENABLE_AFFINITY}" == "1" ]]; then
      live_args+=(--enable-live-affinity)
    fi
  fi

  (
    cd "${REPO_ROOT}" &&
      cargo run -p aegisai-runtime-daemon -- \
        --repo-root . \
        --source linux \
        --metadata procfs \
        --actuator-backend "${backend}" \
        --allow-partial-probes \
        --probe-poll-timeout-ms "${DAEMON_POLL_TIMEOUT_MS}" \
        --batch-size "${DAEMON_BATCH_SIZE}" \
        --tick-ms "${DAEMON_TICK_MS}" \
        --drain-ms "${DAEMON_DRAIN_MS}" \
        "${live_args[@]}"
  ) >"${daemon_log}" 2>&1 &
  daemon_pid=$!
  sleep "${DAEMON_START_DELAY}"
}

wait_for_process() {
  local pid="$1"
  local timeout_s="$2"
  local start_seconds="${SECONDS}"

  while kill -0 "${pid}" >/dev/null 2>&1; do
    if (( SECONDS - start_seconds >= timeout_s )); then
      kill "${pid}" >/dev/null 2>&1 || true
      wait "${pid}" >/dev/null 2>&1
      return 124
    fi
    sleep 0.2
  done

  wait "${pid}"
}

wait_daemon() {
  local daemon_status_var="$1"
  local status

  if [[ -z "${daemon_pid}" ]]; then
    printf -v "${daemon_status_var}" '%s' "0"
    return 0
  fi

  wait_for_process "${daemon_pid}" "${DAEMON_WAIT_TIMEOUT}"
  status=$?
  daemon_pid=""
  printf -v "${daemon_status_var}" '%s' "${status}"
}

write_summary_files() {
  python3 - "${SAMPLES_CSV}" "${MODE_COUNTS_CSV}" "${SUMMARY_CSV}" "${SUMMARY_MD}" <<'PY'
import csv
import math
import statistics
import sys

samples_path, counts_path, summary_csv_path, summary_md_path = sys.argv[1:5]

def percentile(values, percent):
    if not values:
        return None
    ordered = sorted(values)
    index = max(0, min(len(ordered) - 1, math.ceil(percent / 100 * len(ordered)) - 1))
    return ordered[index]

def fmt(value):
    if value is None:
        return "n/a"
    return f"{value:.3f}"

samples = []
with open(samples_path, newline="", encoding="utf-8") as handle:
    for row in csv.DictReader(handle):
        samples.append(row)

counts = []
with open(counts_path, newline="", encoding="utf-8") as handle:
    for row in csv.DictReader(handle):
        counts.append(row)

def successful_mode_samples(mode):
    return [
        row
        for row in samples
        if row["mode"] == mode
        and row["curl_status"] == "0"
        and row["http_code"] == "200"
        and row["stream_done"] == "1"
    ]

baseline_totals = [float(row["total_ms"]) for row in successful_mode_samples("baseline")]
baseline_p95 = percentile(baseline_totals, 95)

rows = []
for count in counts:
    mode = count["mode"]
    mode_samples = [
        row for row in successful_mode_samples(mode)
    ]
    ttfts = [float(row["ttft_ms"]) for row in mode_samples]
    totals = [float(row["total_ms"]) for row in mode_samples]
    latency_p95 = percentile(totals, 95)
    if baseline_p95 and latency_p95 is not None:
        p95_delta = (baseline_p95 - latency_p95) / baseline_p95 * 100
    else:
        p95_delta = None
    row = {
        "mode": mode,
        "backend": count["backend"],
        "samples_ok": str(len(mode_samples)),
        "samples_total": count["sample_count"],
        "ttft_p50_ms": fmt(percentile(ttfts, 50)),
        "ttft_p95_ms": fmt(percentile(ttfts, 95)),
        "ttft_p99_ms": fmt(percentile(ttfts, 99)),
        "latency_p50_ms": fmt(percentile(totals, 50)),
        "latency_p95_ms": fmt(latency_p95),
        "latency_p99_ms": fmt(percentile(totals, 99)),
        "jitter_ms": fmt(statistics.stdev(totals) if len(totals) > 1 else 0.0 if totals else None),
        "trigger_count": count["trigger_count"],
        "rollback_count": count["rollback_count"],
        "p95_delta_vs_baseline_pct": fmt(p95_delta),
    }
    rows.append(row)

fieldnames = [
    "mode",
    "backend",
    "samples_ok",
    "samples_total",
    "ttft_p50_ms",
    "ttft_p95_ms",
    "ttft_p99_ms",
    "latency_p50_ms",
    "latency_p95_ms",
    "latency_p99_ms",
    "jitter_ms",
    "trigger_count",
    "rollback_count",
    "p95_delta_vs_baseline_pct",
]

with open(summary_csv_path, "w", newline="", encoding="utf-8") as handle:
    writer = csv.DictWriter(handle, fieldnames=fieldnames)
    writer.writeheader()
    writer.writerows(rows)

headers = [
    "mode",
    "backend",
    "ok/total",
    "TTFT p50 ms",
    "TTFT p95 ms",
    "TTFT p99 ms",
    "lat P95 ms",
    "lat P99 ms",
    "jitter ms",
    "triggers",
    "rollbacks",
    "P95 delta vs baseline %",
]
lines = [
    "| " + " | ".join(headers) + " |",
    "| " + " | ".join(["---"] * len(headers)) + " |",
]
for row in rows:
    lines.append(
        "| "
        + " | ".join(
            [
                row["mode"],
                row["backend"],
                f'{row["samples_ok"]}/{row["samples_total"]}',
                row["ttft_p50_ms"],
                row["ttft_p95_ms"],
                row["ttft_p99_ms"],
                row["latency_p95_ms"],
                row["latency_p99_ms"],
                row["jitter_ms"],
                row["trigger_count"],
                row["rollback_count"],
                row["p95_delta_vs_baseline_pct"],
            ]
        )
        + " |"
    )

with open(summary_md_path, "w", encoding="utf-8") as handle:
    handle.write("\n".join(lines))
    handle.write("\n")
PY
}

append_daemon_excerpt() {
  local daemon_log="$1"

  if [[ ! -s "${daemon_log}" ]]; then
    append_block "${daemon_log}"
    return 0
  fi

  append '```text'
  grep -E '^(source|metadata|actuator_backend|processed_events|applied_actions|inline_rollbacks|tick_rollbacks|metric_records|trace_records|triggered_scenarios|  inference_tail_guard:)' "${daemon_log}" >>"${LOG_PATH}" || true
  append '```'
}

run_mode() {
  local mode="$1"
  local backend="${MODE_BACKEND[${mode}]}"
  local label="${MODE_LABEL[${mode}]}"
  local mode_dir="${ARTIFACT_DIR}/${mode}"
  local daemon_log="${mode_dir}/daemon.log"
  local stress_log="${mode_dir}/stress-ng.log"
  local mode_request_status=0
  local daemon_status=0
  local stress_status="not_started"
  local stress_exhausted=0
  local processed_events=0
  local trigger_count=0
  local inline_rollbacks=0
  local tick_rollbacks=0
  local rollback_count=0
  local action_error_count=0
  local success_count=0
  local mode_status=0

  mkdir -p "${mode_dir}"

  append ""
  append "#### Mode: ${label}"
  append ""
  append "- Backend: \`${backend}\`"
  append "- Samples: \`${SAMPLES}\`"
  append "- Concurrency: \`${CONCURRENCY}\`"
  append "- Interference: \`$(stress_command_label)\`"

  start_stress "${stress_log}"
  start_daemon "${backend}" "${daemon_log}"

  if ! run_request_batch "${mode}" "${mode_dir}"; then
    mode_request_status=1
  fi

  wait_daemon daemon_status
  stop_stress stress_status stress_exhausted
  append_mode_samples "${mode_dir}"

  success_count="$(successful_samples_for_mode "${mode}")"
  if [[ "${backend}" != "none" ]]; then
    processed_events="$(extract_daemon_number "processed_events" "${daemon_log}")"
    trigger_count="$(extract_trigger_count "${daemon_log}")"
    inline_rollbacks="$(extract_daemon_number "inline_rollbacks" "${daemon_log}")"
    tick_rollbacks="$(extract_daemon_number "tick_rollbacks" "${daemon_log}")"
    action_error_count="$(count_action_errors "${daemon_log}")"
    processed_events="${processed_events:-0}"
    trigger_count="${trigger_count:-0}"
    inline_rollbacks="${inline_rollbacks:-0}"
    tick_rollbacks="${tick_rollbacks:-0}"
    rollback_count=$((inline_rollbacks + tick_rollbacks))
  fi

  if [[ "${mode_request_status}" -ne 0 || "${success_count}" -ne "${SAMPLES}" ]]; then
    mode_status=1
  fi
  if [[ "${stress_exhausted}" -ne 0 ]]; then
    mode_status=1
  fi
  if [[ "${backend}" != "none" ]]; then
    if [[ "${daemon_status}" -ne 0 || "${processed_events}" -le 0 || "${trigger_count}" -le 0 || "${rollback_count}" -le 0 || "${action_error_count}" -ne 0 ]]; then
      mode_status=1
    fi
  fi

  printf '%s,%s,%s,%s,%s,%s,%s,%s,%s,%s,%s\n' \
    "${mode}" \
    "${backend}" \
    "${processed_events}" \
    "${trigger_count}" \
    "${rollback_count}" \
    "${daemon_status}" \
    "${stress_status}" \
    "${stress_exhausted}" \
    "${action_error_count}" \
    "${success_count}" \
    "${SAMPLES}" \
    >>"${MODE_COUNTS_CSV}"

  append "- Request success: \`${success_count}/${SAMPLES}\`"
  append "- Daemon status: \`${daemon_status}\`"
  append "- Stress status: \`${stress_status}\`"
  append "- Stress exhausted before mode finished: \`${stress_exhausted}\`"
  append "- Daemon processed events: \`${processed_events}\`"
  append "- Trigger count: \`${trigger_count}\`"
  append "- Rollback count: \`${rollback_count}\`"
  append "- Action audit error count: \`${action_error_count}\`"
  append "- Mode artifacts: \`${mode_dir}\`"
  append "- Mode result: \`$(if [[ "${mode_status}" -eq 0 ]]; then printf 'PASS'; else printf 'FAIL'; fi)\`"
  append ""
  append "Daemon summary excerpt:"
  append_daemon_excerpt "${daemon_log}"

  if [[ "${mode_status}" -ne 0 ]]; then
    overall_status=1
  fi

  sleep "${MODE_COOLDOWN}"
}

parse_modes

timestamp="$(date -Iseconds)"

append ""
append "### ${timestamp} - Inference Tail Guard Ollama A/B harness"
append ""
append "- Scope: Phase 2 MVP reproducible A/B proof, replacing the old single-request smoke semantics."
append "- Working directory: \`${REPO_ROOT}\`"
append "- Log path: \`${LOG_PATH}\`"
append "- Artifact directory: \`${ARTIFACT_DIR}\`"
append "- Runtime: \`ollama\`"
append "- Selected modes: \`${SELECTED_MODES[*]}\`"
append "- Exit contract: every mode must finish all samples; observation/guarded modes must capture daemon events, trigger \`inference_tail_guard\`, roll back, and have no action audit errors."

validate_config
require_command python3
require_command ollama
require_command cargo
require_command curl

if [[ "${STRESS_CPU}" -gt 0 || "${STRESS_IO}" -gt 0 || "${STRESS_HDD}" -gt 0 ]] && [[ "${REQUIRE_STRESS}" == "1" ]]; then
  require_command stress-ng
fi

if [[ " ${SELECTED_MODES[*]} " == *" live_guarded "* ]]; then
  if [[ "$(uname -s)" != "Linux" ]]; then
    append "- Requirement: live guarded mode requires Linux."
    append "- Status: \`FAIL\`"
    printf 'live_guarded mode requires Linux.\n' >&2
    exit 1
  fi
  require_command renice
  if [[ "${LIVE_ENABLE_AFFINITY}" == "1" ]]; then
    require_command taskset
  fi
fi

prompt_hash="$(prompt_sha256)"
write_payload true "${PAYLOAD_STREAM}"
write_payload false "${PAYLOAD_WARMUP}"
write_run_env "${prompt_hash}"

append ""
append "#### Fixed experiment controls"
append ""
append "- Model: \`${MODEL}\`"
append "- Prompt sha256: \`${prompt_hash}\`"
append "- Prompt: \`${PROMPT}\`"
append "- Ollama endpoint: \`${OLLAMA_API_URL}\`"
append "- Request shape: \`stream=true\`, \`num_predict=${NUM_PREDICT}\`, \`temperature=${TEMPERATURE}\`, \`seed=${SEED}\`, \`keep_alive=${KEEP_ALIVE}\`"
append "- Samples per mode: \`${SAMPLES}\`"
append "- Concurrency: \`${CONCURRENCY}\`"
append "- Interference: \`$(stress_command_label)\`"
append "- Stress lifecycle: \`$(if [[ "${STRESS_CPU}" -eq 0 && "${STRESS_IO}" -eq 0 && "${STRESS_HDD}" -eq 0 ]]; then printf 'disabled'; elif [[ "${STRESS_TIMEOUT}" -gt 0 ]]; then printf 'self-timeout cap; mode fails if pressure exits early'; else printf 'harness-controlled per mode'; fi)\`"
append "- Daemon poll timeout: \`${DAEMON_POLL_TIMEOUT_MS}ms\`"
if [[ " ${SELECTED_MODES[*]} " == *" live_guarded "* ]]; then
  append "- Live actuator confirmation: \`${LIVE_CONFIRM}\`"
  append "- Live PID allowlist: \`${LIVE_PID_ALLOWLIST}\`"
  append "- Live actuator scope: \`$(if [[ "${LIVE_ENABLE_AFFINITY}" == "1" ]]; then printf 'nice,affinity'; else printf 'nice'; fi)\`"
fi
append "- Run environment artifact: \`${RUN_ENV}\`"

printf 'sample,mode,backend,curl_status,http_code,stream_done,ttft_ms,total_ms,body_bytes,error_bytes\n' >"${SAMPLES_CSV}"
printf 'mode,backend,processed_events,trigger_count,rollback_count,daemon_status,stress_status,stress_exhausted,action_error_count,sample_success_count,sample_count\n' >"${MODE_COUNTS_CSV}"

tmp_show="${ARTIFACT_DIR}/ollama.show.txt"
tmp_ps_before="${ARTIFACT_DIR}/ollama.ps.before.txt"
tmp_ps_after="${ARTIFACT_DIR}/ollama.ps.after.txt"
tmp_warmup="${ARTIFACT_DIR}/warmup.body.json"
tmp_warmup_curl="${ARTIFACT_DIR}/warmup.curl"
tmp_warmup_err="${ARTIFACT_DIR}/warmup.err"

append ""
append "#### Selected model metadata"
append ""
append "- Requirement: required"
append "- Command: \`ollama show ${MODEL}\`"
(
  cd "${REPO_ROOT}" &&
    ollama show "${MODEL}"
) >"${tmp_show}" 2>&1
show_status=$?
append "- Exit status: \`${show_status}\`"
append_block "${tmp_show}"
if [[ "${show_status}" -ne 0 ]]; then
  append "- Overall result: \`FAIL\`"
  exit 1
fi

append ""
append "#### Ollama process inventory before harness"
append ""
append "- Requirement: informational"
append "- Command: \`ollama ps\`"
(
  cd "${REPO_ROOT}" &&
    ollama ps
) >"${tmp_ps_before}" 2>&1
ps_before_status=$?
append "- Exit status: \`${ps_before_status}\`"
append_block "${tmp_ps_before}"

append ""
append "#### Warmup inference request"
append ""
append "- Requirement: required"
append "- Endpoint: \`${OLLAMA_API_URL}\`"
append "- Model: \`${MODEL}\`"
run_http_payload "${PAYLOAD_WARMUP}" "${tmp_warmup}" "${tmp_warmup_curl}" "${tmp_warmup_err}"
warmup_status=$?
warmup_http_code="$(sed -n 's/^http_code=//p' "${tmp_warmup_curl}" | head -n 1)"
append "- Curl exit status: \`${warmup_status}\`"
append "- HTTP status: \`${warmup_http_code:-000}\`"
append "- Curl timing:"
append_block "${tmp_warmup_curl}"
append "- Response body:"
append_block "${tmp_warmup}"
if [[ "${warmup_status}" -ne 0 || "${warmup_http_code:-000}" != "200" ]]; then
  append "- Warmup stderr:"
  append_block "${tmp_warmup_err}"
  append "- Overall result: \`FAIL\`"
  exit 1
fi

for mode in "${SELECTED_MODES[@]}"; do
  run_mode "${mode}"
done

write_summary_files

append ""
append "#### A/B metrics summary"
append ""
append "- TTFT column: p50 of \`curl time_starttransfer\` against streaming Ollama responses."
append "- P95/P99 columns: end-to-end streaming request total latency."
append "- Jitter column: sample standard deviation of total latency."
append "- Raw samples: \`${SAMPLES_CSV}\`"
append "- Mode counts: \`${MODE_COUNTS_CSV}\`"
append "- Summary CSV: \`${SUMMARY_CSV}\`"
append ""
append_file "${SUMMARY_MD}"

append ""
append "#### Ollama process inventory after harness"
append ""
append "- Requirement: informational"
append "- Command: \`ollama ps\`"
(
  cd "${REPO_ROOT}" &&
    ollama ps
) >"${tmp_ps_after}" 2>&1
ps_after_status=$?
append "- Exit status: \`${ps_after_status}\`"
append_block "${tmp_ps_after}"

append ""
if [[ "${overall_status}" -eq 0 ]]; then
  append "- Overall result: \`PASS\`"
else
  append "- Overall result: \`FAIL\`"
fi

printf '%s\n' "Inference Tail Guard Ollama A/B harness summary:"
cat "${SUMMARY_MD}"
printf '%s\n' "Artifacts: ${ARTIFACT_DIR}"
printf '%s\n' "Verification log: ${LOG_PATH}"

exit "${overall_status}"
