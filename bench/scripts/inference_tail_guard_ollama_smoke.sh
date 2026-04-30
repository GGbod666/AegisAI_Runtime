#!/usr/bin/env bash
set -u

SCRIPT_DIR="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" >/dev/null 2>&1 && pwd)"
REPO_ROOT="$(cd -- "${SCRIPT_DIR}/../.." >/dev/null 2>&1 && pwd)"
LOG_PATH="${AEGISAI_VERIFY_LOG:-${REPO_ROOT}/docs/verification_log.md}"

MODEL="${AEGISAI_OLLAMA_MODEL:-qwen2.5:0.5b}"
NUM_PREDICT="${AEGISAI_OLLAMA_NUM_PREDICT:-96}"
DAEMON_BACKEND="${AEGISAI_DAEMON_BACKEND:-noop}"
DAEMON_POLL_TIMEOUT_MS="${AEGISAI_DAEMON_POLL_TIMEOUT_MS:-2000}"
STRESS_CPU="${AEGISAI_STRESS_CPU:-2}"
STRESS_TIMEOUT="${AEGISAI_STRESS_TIMEOUT:-12}"
OLLAMA_API_URL="${AEGISAI_OLLAMA_API_URL:-http://127.0.0.1:11434/api/generate}"
PROMPT="请用两句中文说明 AegisAI 正在进行实时推理 smoke test，并补一句当前目标是观察尾延迟。"

mkdir -p "$(dirname -- "${LOG_PATH}")"
touch "${LOG_PATH}"

daemon_pid=""
stress_pid=""
stress_status=""

append() {
  printf '%s\n' "$*" >>"${LOG_PATH}"
}

append_block() {
  local file="$1"
  append '```text'
  cat "${file}" >>"${LOG_PATH}"
  append '```'
}

has_command() {
  command -v "$1" >/dev/null 2>&1
}

json_number_field() {
  local file="$1"
  local key="$2"
  sed -n "s/.*\"${key}\":\\([0-9][0-9]*\\).*/\\1/p" "${file}" | head -n 1
}

ns_to_ms() {
  local value="${1:-}"
  if [[ -z "${value}" ]]; then
    printf 'n/a'
  else
    printf '%s' "$((value / 1000000))"
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

timestamp="$(date -Iseconds)"
overall_status=0
observed_trigger_count=0
processed_events=0

append ""
append "### ${timestamp} - Inference Tail Guard Ollama smoke"
append ""
append "- Scope: first real-runtime smoke run after the pre-Ollama preflight gate."
append "- Working directory: \`${REPO_ROOT}\`"
append "- Log path: \`${LOG_PATH}\`"
append "- Runtime: \`ollama\`"
append "- Selected model: \`${MODEL}\`"
append "- Observation backend: \`${DAEMON_BACKEND}\`"
append "- Daemon poll timeout: \`${DAEMON_POLL_TIMEOUT_MS}ms\`"
append "- Planned interference: \`stress-ng --cpu ${STRESS_CPU} --timeout ${STRESS_TIMEOUT}s\` when available."
append "- A/B status: \`not applicable\` in this smoke run; this pass validates real model execution plus policy observation."

if ! has_command ollama; then
  append ""
  append "#### ollama command"
  append ""
  append "- Requirement: required"
  append "- Status: \`FAIL\`"
  append "- Reason: \`ollama\` is not installed or is not on PATH."
  exit 1
fi

if ! has_command cargo; then
  append ""
  append "#### cargo command"
  append ""
  append "- Requirement: required"
  append "- Status: \`FAIL\`"
  append "- Reason: \`cargo\` is not installed or is not on PATH."
  exit 1
fi

if ! has_command curl; then
  append ""
  append "#### curl command"
  append ""
  append "- Requirement: required"
  append "- Status: \`FAIL\`"
  append "- Reason: \`curl\` is not installed or is not on PATH."
  exit 1
fi

tmp_show="$(mktemp)"
tmp_ps="$(mktemp)"
tmp_warmup="$(mktemp)"
tmp_daemon="$(mktemp)"
tmp_infer="$(mktemp)"
tmp_stress="$(mktemp)"

append ""
append "#### Selected model metadata"
append ""
append "- Requirement: required"
append "- Command: \`ollama show ${MODEL}\`"
append "- Working directory: \`${REPO_ROOT}\`"
(
  cd "${REPO_ROOT}" &&
    ollama show "${MODEL}"
) >"${tmp_show}" 2>&1
show_status=$?
append "- Exit status: \`${show_status}\`"
append_block "${tmp_show}"
if [[ "${show_status}" -ne 0 ]]; then
  overall_status=1
fi

append ""
append "#### Ollama process inventory before warmup"
append ""
append "- Requirement: informational"
append "- Command: \`ollama ps\`"
append "- Working directory: \`${REPO_ROOT}\`"
(
  cd "${REPO_ROOT}" &&
    ollama ps
) >"${tmp_ps}" 2>&1
ps_before_status=$?
append "- Exit status: \`${ps_before_status}\`"
append_block "${tmp_ps}"

append ""
append "#### Warmup inference request"
append ""
append "- Requirement: required"
append "- Endpoint: \`${OLLAMA_API_URL}\`"
append "- Model: \`${MODEL}\`"
append "- Command: \`curl -sS -X POST ${OLLAMA_API_URL}\`"
append "- Request shape: \`stream=false\`, \`num_predict=${NUM_PREDICT}\`"
append "- Working directory: \`${REPO_ROOT}\`"
curl -sS -X POST "${OLLAMA_API_URL}" \
  -H 'Content-Type: application/json' \
  -d "{\"model\":\"${MODEL}\",\"prompt\":\"${PROMPT}\",\"stream\":false,\"options\":{\"num_predict\":${NUM_PREDICT}}}" \
  >"${tmp_warmup}" 2>&1
warmup_status=$?
append "- Exit status: \`${warmup_status}\`"
append_block "${tmp_warmup}"
if [[ "${warmup_status}" -ne 0 ]]; then
  overall_status=1
fi

if has_command stress-ng; then
  stress-ng --cpu "${STRESS_CPU}" --timeout "${STRESS_TIMEOUT}"s >"${tmp_stress}" 2>&1 &
  stress_pid=$!
else
  printf '%s\n' '`stress-ng` is not installed or is not on PATH.' >"${tmp_stress}"
fi

(
  cd "${REPO_ROOT}" &&
    cargo run -p aegisai-runtime-daemon -- \
      --repo-root . \
      --source linux \
      --metadata procfs \
      --actuator-backend "${DAEMON_BACKEND}" \
      --allow-partial-probes \
      --probe-poll-timeout-ms "${DAEMON_POLL_TIMEOUT_MS}"
) >"${tmp_daemon}" 2>&1 &
daemon_pid=$!

sleep 0.5

curl -sS -X POST "${OLLAMA_API_URL}" \
  -H 'Content-Type: application/json' \
  -d "{\"model\":\"${MODEL}\",\"prompt\":\"${PROMPT}\",\"stream\":false,\"options\":{\"num_predict\":${NUM_PREDICT}}}" \
  >"${tmp_infer}" 2>&1
infer_status=$?

wait "${daemon_pid}"
daemon_status=$?
daemon_pid=""

if [[ -n "${stress_pid}" ]]; then
  wait "${stress_pid}"
  stress_status=$?
  stress_pid=""
else
  stress_status="skipped"
fi

append ""
append "#### Monitored inference request"
append ""
append "- Requirement: required"
append "- Endpoint: \`${OLLAMA_API_URL}\`"
append "- Model: \`${MODEL}\`"
append "- Command: \`curl -sS -X POST ${OLLAMA_API_URL}\`"
append "- Observation backend: \`${DAEMON_BACKEND}\`"
append "- Interference: \`stress-ng --cpu ${STRESS_CPU} --timeout ${STRESS_TIMEOUT}s\` when available"
append "- Exit status: \`${infer_status}\`"
append_block "${tmp_infer}"
if [[ "${infer_status}" -ne 0 ]]; then
  overall_status=1
fi

append ""
append "#### Runtime daemon observation"
append ""
append "- Requirement: required"
append "- Command: \`cargo run -p aegisai-runtime-daemon -- --repo-root . --source linux --metadata procfs --actuator-backend ${DAEMON_BACKEND} --allow-partial-probes --probe-poll-timeout-ms ${DAEMON_POLL_TIMEOUT_MS}\`"
append "- Exit status: \`${daemon_status}\`"
append_block "${tmp_daemon}"
if [[ "${daemon_status}" -ne 0 ]]; then
  overall_status=1
fi

append ""
append "#### stress-ng interference"
append ""
append "- Requirement: optional"
if has_command stress-ng; then
  append "- Command: \`stress-ng --cpu ${STRESS_CPU} --timeout ${STRESS_TIMEOUT}s\`"
  append "- Exit status: \`${stress_status}\`"
else
  append "- Status: \`SKIPPED\`"
fi
append_block "${tmp_stress}"

append ""
append "#### Ollama process inventory after monitored request"
append ""
append "- Requirement: informational"
append "- Command: \`ollama ps\`"
append "- Working directory: \`${REPO_ROOT}\`"
(
  cd "${REPO_ROOT}" &&
    ollama ps
) >"${tmp_ps}" 2>&1
ps_after_status=$?
append "- Exit status: \`${ps_after_status}\`"
append_block "${tmp_ps}"

processed_events="$(sed -n 's/^processed_events: \([0-9][0-9]*\)$/\1/p' "${tmp_daemon}" | head -n 1)"
observed_trigger_count="$(sed -n 's/^  inference_tail_guard: \([0-9][0-9]*\)$/\1/p' "${tmp_daemon}" | head -n 1)"
request_total_duration_ns="$(json_number_field "${tmp_infer}" total_duration)"
request_eval_duration_ns="$(json_number_field "${tmp_infer}" eval_duration)"
request_load_duration_ns="$(json_number_field "${tmp_infer}" load_duration)"

if [[ -z "${processed_events}" ]]; then
  processed_events=0
fi

if [[ -z "${observed_trigger_count}" ]]; then
  observed_trigger_count=0
fi

append ""
append "- Monitored request total duration: \`$(ns_to_ms "${request_total_duration_ns}")ms\`"
append "- Monitored request eval duration: \`$(ns_to_ms "${request_eval_duration_ns}")ms\`"
append "- Monitored request load duration: \`$(ns_to_ms "${request_load_duration_ns}")ms\`"
append "- Daemon processed events: \`${processed_events}\`"
append "- Observed \`inference_tail_guard\` trigger count: \`${observed_trigger_count}\`"
append "- Interpretation: \`$(if [[ "${observed_trigger_count}" -gt 0 ]]; then printf 'real-runtime trigger observed'; elif [[ "${processed_events}" -gt 0 ]]; then printf 'real-runtime events observed without trigger'; else printf 'request succeeded but no runtime events were captured'; fi)\`"
append "- Safety note: \`${DAEMON_BACKEND}\` keeps this smoke run in observation mode; no privileged boost/rollback syscalls were applied."

append ""
if [[ "${overall_status}" -eq 0 ]]; then
  append "- Overall result: \`PASS\`"
else
  append "- Overall result: \`FAIL\`"
fi

rm -f "${tmp_show}" "${tmp_ps}" "${tmp_warmup}" "${tmp_daemon}" "${tmp_infer}" "${tmp_stress}"

exit "${overall_status}"
