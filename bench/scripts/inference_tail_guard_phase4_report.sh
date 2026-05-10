#!/usr/bin/env bash
set -u

SCRIPT_DIR="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" >/dev/null 2>&1 && pwd)"
REPO_ROOT="$(cd -- "${SCRIPT_DIR}/../.." >/dev/null 2>&1 && pwd)"
LOG_PATH="${AEGISAI_VERIFY_LOG:-${REPO_ROOT}/docs/verification_log.md}"
RUN_ID="${AEGISAI_PHASE4_RUN_ID:-$(date -u +%Y%m%dT%H%M%SZ)}"
ARTIFACT_DIR="${AEGISAI_PHASE4_ARTIFACT_DIR:-${REPO_ROOT}/.cache/aegisai/inference_tail_guard_phase4/${RUN_ID}}"
REPORT_MD="${AEGISAI_PHASE4_REPORT:-${REPO_ROOT}/docs/mvp_benefit_report.md}"

ROUNDS="${AEGISAI_PHASE4_ROUNDS:-3}"
SAMPLES="${AEGISAI_AB_SAMPLES:-8}"
CONCURRENCY="${AEGISAI_AB_CONCURRENCY:-2}"
MODES="${AEGISAI_PHASE4_MODES:-baseline,noop_observation,dry_run}"
SCENARIOS="${AEGISAI_PHASE4_SCENARIOS:-cpu,cpu_io}"

MODEL="${AEGISAI_OLLAMA_MODEL:-qwen2.5:0.5b}"
NUM_PREDICT="${AEGISAI_OLLAMA_NUM_PREDICT:-96}"
CPU_WORKERS="${AEGISAI_PHASE4_CPU:-2}"
IO_WORKERS="${AEGISAI_PHASE4_IO:-1}"
HDD_WORKERS="${AEGISAI_PHASE4_HDD:-1}"
HDD_BYTES="${AEGISAI_PHASE4_HDD_BYTES:-128M}"
MODE_COOLDOWN="${AEGISAI_PHASE4_COOLDOWN:-2}"
REUSE_ARTIFACTS="${AEGISAI_PHASE4_REUSE_ARTIFACTS:-0}"

DETAIL_CSV="${ARTIFACT_DIR}/phase4_runs.csv"
AGGREGATE_CSV="${ARTIFACT_DIR}/phase4_aggregate.csv"
SUMMARY_MD="${ARTIFACT_DIR}/phase4_report.md"

mkdir -p "$(dirname -- "${LOG_PATH}")" "${ARTIFACT_DIR}" "$(dirname -- "${REPORT_MD}")"
touch "${LOG_PATH}"

append_log() {
  printf '%s\n' "$*" >>"${LOG_PATH}"
}

is_positive_uint() {
  [[ "${1:-}" =~ ^[0-9]+$ ]] && [[ "$1" -gt 0 ]]
}

scenario_label() {
  case "$1" in
    cpu)
      printf 'CPU interference'
      ;;
    cpu_io)
      printf 'CPU + optional I/O interference'
      ;;
    no_interference)
      printf 'No interference'
      ;;
    *)
      printf '%s' "$1"
      ;;
  esac
}

scenario_cpu() {
  case "$1" in
    no_interference)
      printf '0'
      ;;
    *)
      printf '%s' "${CPU_WORKERS}"
      ;;
  esac
}

scenario_io() {
  case "$1" in
    cpu_io)
      printf '%s' "${IO_WORKERS}"
      ;;
    *)
      printf '0'
      ;;
  esac
}

scenario_hdd() {
  case "$1" in
    cpu_io)
      printf '%s' "${HDD_WORKERS}"
      ;;
    *)
      printf '0'
      ;;
  esac
}

append_round_detail() {
  local scenario="$1"
  local label="$2"
  local round="$3"
  local run_status="$4"
  local run_dir="$5"

  if [[ ! -s "${run_dir}/summary.csv" ]]; then
    printf '%s,%s,%s,%s,%s,%s,%s,%s,%s,%s,%s,%s,%s,%s,%s,%s,%s,%s,%s,%s,%s,%s,%s,%s\n' \
      "${scenario}" "${label}" "${round}" "${run_status}" "missing" "" "" "" "" "" "" "" "" "" "" "" "" "" "" "" "" "0" "0" "${run_dir}" \
      >>"${DETAIL_CSV}"
    return 0
  fi

  python3 - "${scenario}" "${label}" "${round}" "${run_status}" "${run_dir}" "${run_dir}/summary.csv" "${run_dir}/mode_counts.csv" >>"${DETAIL_CSV}" <<'PY'
import csv
import os
import re
import sys

scenario, label, round_id, harness_status, run_dir, summary_path, counts_path = sys.argv[1:8]

counts = {}
try:
    with open(counts_path, newline="", encoding="utf-8") as handle:
        counts = {row["mode"]: row for row in csv.DictReader(handle)}
except FileNotFoundError:
    counts = {}

def mode_status(mode, row):
    count = counts.get(mode, {})
    try:
        samples_ok = int(row["samples_ok"])
        samples_total = int(row["samples_total"])
        trigger_count = int(row["trigger_count"])
        rollback_count = int(row["rollback_count"])
        action_errors = int(count.get("action_error_count", "0") or 0)
        daemon_status = int(count.get("daemon_status", "0") or 0)
        stress_exhausted = int(count.get("stress_exhausted", "0") or 0)
    except ValueError:
        return "1"

    if samples_ok != samples_total or stress_exhausted != 0:
        return "1"
    if mode == "baseline":
        return "0"
    if daemon_status != 0 or trigger_count <= 0 or rollback_count <= 0 or action_errors != 0:
        return "1"
    return "0"

def live_effective_action_counts(mode):
    if mode != "live_guarded":
        return "0", "0"

    daemon_path = os.path.join(run_dir, mode, "daemon.log")
    try:
        with open(daemon_path, encoding="utf-8") as handle:
            text = handle.read()
    except FileNotFoundError:
        return "0", "0"

    priority_limited = text.count("priority_raise_limited=true")
    effective_renice = 0
    renice_pattern = re.compile(
        r"backend\.apply\.apply\.\d+\.detail=.*?command=renice\s+-?\d+\s+-p\s+\d+;output=[^;\n]*old priority\s+(-?\d+),\s+new priority\s+(-?\d+)"
    )
    for old_priority, new_priority in renice_pattern.findall(text):
        if old_priority != new_priority:
            effective_renice += 1

    def parse_cpu_set(raw):
        cpus = set()
        for segment in raw.strip().split(","):
            if not segment:
                continue
            if "-" in segment:
                start, end = segment.split("-", 1)
                try:
                    start, end = int(start), int(end)
                except ValueError:
                    return None
                if start > end:
                    return None
                cpus.update(range(start, end + 1))
            else:
                try:
                    cpus.add(int(segment))
                except ValueError:
                    return None
        return cpus if cpus else None

    effective_taskset = 0
    taskset_pattern = re.compile(
        r"backend\.apply\.apply\.\d+\.detail=.*?command=taskset\s+-pc\b[^\n]*;output=pid\s+\d+'s current affinity list:\s*([^\n]+)\n"
        r"pid\s+\d+'s new affinity list:\s*([^\n]+)"
    )
    for old_affinity, new_affinity in taskset_pattern.findall(text):
        old_cpus = parse_cpu_set(old_affinity)
        new_cpus = parse_cpu_set(new_affinity)
        if old_cpus is not None and new_cpus is not None:
            if old_cpus != new_cpus:
                effective_taskset += 1
        elif old_affinity.strip() != new_affinity.strip():
            effective_taskset += 1

    return str(effective_renice + effective_taskset), str(priority_limited)

with open(summary_path, newline="", encoding="utf-8") as handle:
    reader = csv.DictReader(handle)
    for row in reader:
        status = mode_status(row["mode"], row)
        count = counts.get(row["mode"], {})
        live_effective_actions, live_priority_limited = live_effective_action_counts(row["mode"])
        print(",".join([
            scenario,
            label,
            round_id,
            status,
            row["mode"],
            row["backend"],
            row["samples_ok"],
            row["samples_total"],
            row["ttft_p95_ms"],
            row["ttft_p99_ms"],
            row["latency_p95_ms"],
            row["latency_p99_ms"],
            row["jitter_ms"],
            row["trigger_count"],
            row["rollback_count"],
            count.get("action_error_count", "0"),
            row.get("cpu_migration_total", count.get("cpu_migration_total", "0")),
            row.get("cpu_migrations_per_sec_max", count.get("cpu_migrations_per_sec_max", "0")),
            row.get("major_page_fault_total", count.get("major_page_fault_total", "0")),
            row.get("major_page_faults_per_sec_max", count.get("major_page_faults_per_sec_max", "0")),
            row.get("offcpu_time_events", count.get("offcpu_time_events", "0")),
            live_effective_actions,
            live_priority_limited,
            run_dir,
        ]))
PY
}

run_one_round() {
  local scenario="$1"
  local round="$2"
  local label cpu_workers io_workers hdd_workers run_dir run_status

  label="$(scenario_label "${scenario}")"
  cpu_workers="$(scenario_cpu "${scenario}")"
  io_workers="$(scenario_io "${scenario}")"
  hdd_workers="$(scenario_hdd "${scenario}")"
  run_dir="${ARTIFACT_DIR}/${scenario}/round_${round}"
  mkdir -p "${run_dir}"

  append_log ""
  append_log "#### Phase 4 round: ${label} / ${round}"
  append_log ""
  append_log "- Artifact directory: \`${run_dir}\`"
  append_log "- Modes: \`${MODES}\`"
  append_log "- Samples per mode: \`${SAMPLES}\`"
  append_log "- Concurrency: \`${CONCURRENCY}\`"
  append_log "- CPU workers: \`${cpu_workers}\`"
  append_log "- I/O sync workers: \`${io_workers}\`"
  append_log "- I/O disk workers: \`${hdd_workers}\`"

  AEGISAI_AB_RUN_ID="${RUN_ID}_${scenario}_${round}" \
    AEGISAI_AB_ARTIFACT_DIR="${run_dir}" \
    AEGISAI_AB_MODES="${MODES}" \
    AEGISAI_AB_SAMPLES="${SAMPLES}" \
    AEGISAI_AB_CONCURRENCY="${CONCURRENCY}" \
    AEGISAI_OLLAMA_MODEL="${MODEL}" \
    AEGISAI_STRESS_CPU="${cpu_workers}" \
    AEGISAI_STRESS_IO="${io_workers}" \
    AEGISAI_STRESS_HDD="${hdd_workers}" \
    AEGISAI_STRESS_HDD_BYTES="${HDD_BYTES}" \
    AEGISAI_STRESS_TEMP_PATH="${run_dir}/stress-tmp" \
    AEGISAI_VERIFY_LOG="${LOG_PATH}" \
    bash "${SCRIPT_DIR}/inference_tail_guard_ollama_smoke.sh" \
    >"${run_dir}/harness.stdout" 2>"${run_dir}/harness.stderr"
  run_status=$?

  append_log "- Round exit status: \`${run_status}\`"
  append_log "- Harness stdout: \`${run_dir}/harness.stdout\`"
  append_log "- Harness stderr: \`${run_dir}/harness.stderr\`"

  append_round_detail "${scenario}" "${label}" "${round}" "${run_status}" "${run_dir}"
  sleep "${MODE_COOLDOWN}"
  return "${run_status}"
}

collect_one_round() {
  local scenario="$1"
  local round="$2"
  local label run_dir run_status

  label="$(scenario_label "${scenario}")"
  run_dir="${ARTIFACT_DIR}/${scenario}/round_${round}"
  run_status=0
  if [[ ! -s "${run_dir}/summary.csv" ]]; then
    run_status=1
  fi

  append_log ""
  append_log "#### Phase 4 reused round: ${label} / ${round}"
  append_log ""
  append_log "- Artifact directory: \`${run_dir}\`"
  append_log "- Reused existing summary: \`$(if [[ "${run_status}" -eq 0 ]]; then printf 'yes'; else printf 'missing'; fi)\`"

  append_round_detail "${scenario}" "${label}" "${round}" "${run_status}" "${run_dir}"
  return "${run_status}"
}

if ! is_positive_uint "${ROUNDS}"; then
  printf 'AEGISAI_PHASE4_ROUNDS must be a positive integer.\n' >&2
  exit 1
fi
if ! is_positive_uint "${SAMPLES}"; then
  printf 'AEGISAI_AB_SAMPLES must be a positive integer.\n' >&2
  exit 1
fi
if ! is_positive_uint "${CONCURRENCY}"; then
  printf 'AEGISAI_AB_CONCURRENCY must be a positive integer.\n' >&2
  exit 1
fi
if [[ "${REUSE_ARTIFACTS}" != "0" && "${REUSE_ARTIFACTS}" != "1" ]]; then
  printf 'AEGISAI_PHASE4_REUSE_ARTIFACTS must be 0 or 1.\n' >&2
  exit 1
fi

timestamp="$(date -Iseconds)"

append_log ""
append_log "### ${timestamp} - Phase 4 MVP benefit report run"
append_log ""
append_log "- Scope: multi-round CPU interference and optional I/O perturbation benefit report."
append_log "- Working directory: \`${REPO_ROOT}\`"
append_log "- Artifact directory: \`${ARTIFACT_DIR}\`"
append_log "- Report path: \`${REPORT_MD}\`"
append_log "- Run ID: \`${RUN_ID}\`"
append_log "- Reuse existing artifacts: \`${REUSE_ARTIFACTS}\`"
append_log "- Success criterion: MVP benefit is true only when P95/P99, TTFT, or jitter shows a stable improvement trend vs baseline across rounds and live_guarded records effective host-level actuator changes."

printf 'scenario,scenario_label,round,run_status,mode,backend,samples_ok,samples_total,ttft_p95_ms,ttft_p99_ms,latency_p95_ms,latency_p99_ms,jitter_ms,trigger_count,rollback_count,action_error_count,cpu_migration_total,cpu_migrations_per_sec_max,major_page_fault_total,major_page_faults_per_sec_max,offcpu_time_events,live_effective_action_count,live_priority_limited_count,artifact_dir\n' >"${DETAIL_CSV}"

overall_status=0
for scenario in ${SCENARIOS//,/ }; do
  for ((round = 1; round <= ROUNDS; round += 1)); do
    if [[ "${REUSE_ARTIFACTS}" == "1" ]]; then
      if ! collect_one_round "${scenario}" "${round}"; then
        overall_status=1
      fi
    elif ! run_one_round "${scenario}" "${round}"; then
      overall_status=1
    fi
  done
done

python3 - "${DETAIL_CSV}" "${AGGREGATE_CSV}" "${SUMMARY_MD}" "${RUN_ID}" "${ROUNDS}" "${SAMPLES}" "${CONCURRENCY}" "${MODES}" "${MODEL}" "${NUM_PREDICT}" "${SCENARIOS}" "${CPU_WORKERS}" "${IO_WORKERS}" "${HDD_WORKERS}" "${HDD_BYTES}" "${AEGISAI_CONFIRM_LIVE_ACTUATOR:-0}" "${AEGISAI_LIVE_PID_ALLOWLIST:-}" "${AEGISAI_ENABLE_LIVE_AFFINITY:-0}" <<'PY'
import csv
import math
import statistics
import sys
from collections import defaultdict

(
    detail_path,
    aggregate_path,
    summary_path,
    run_id,
    rounds,
    samples,
    concurrency,
    modes,
    model,
    num_predict,
    scenarios,
    cpu_workers,
    io_workers,
    hdd_workers,
    hdd_bytes,
    live_confirm,
    live_pid_allowlist,
    live_enable_affinity,
) = sys.argv[1:19]

METRICS = [
    ("ttft_p95_ms", "TTFT P95"),
    ("ttft_p99_ms", "TTFT P99"),
    ("latency_p95_ms", "Latency P95"),
    ("latency_p99_ms", "Latency P99"),
    ("jitter_ms", "Jitter"),
]

def parse_float(value):
    if value in ("", "n/a", "missing"):
        return None
    try:
        return float(value)
    except ValueError:
        return None

def fmt(value):
    if value is None:
        return "n/a"
    return f"{value:.3f}"

def pct_fmt(value):
    if value is None:
        return "n/a"
    return f"{value:.2f}"

with open(detail_path, newline="", encoding="utf-8") as handle:
    rows = list(csv.DictReader(handle))

by_key = defaultdict(list)
for row in rows:
    by_key[(row["scenario"], row["mode"])].append(row)

scenario_labels = {}
for row in rows:
    scenario_labels[row["scenario"]] = row["scenario_label"]

aggregate_rows = []
for (scenario, mode), mode_rows in sorted(by_key.items()):
    if mode == "missing":
        continue
    agg = {
        "scenario": scenario,
        "scenario_label": scenario_labels.get(scenario, scenario),
        "mode": mode,
        "rounds_ok": str(sum(1 for row in mode_rows if row["run_status"] == "0")),
        "rounds_total": str(len(mode_rows)),
        "samples_ok_total": str(sum(int(row["samples_ok"] or 0) for row in mode_rows)),
        "samples_total": str(sum(int(row["samples_total"] or 0) for row in mode_rows)),
        "trigger_count_total": str(sum(int(row["trigger_count"] or 0) for row in mode_rows)),
        "rollback_count_total": str(sum(int(row["rollback_count"] or 0) for row in mode_rows)),
        "action_error_count_total": str(sum(int(row["action_error_count"] or 0) for row in mode_rows)),
        "cpu_migration_total": str(sum(int(row["cpu_migration_total"] or 0) for row in mode_rows)),
        "cpu_migrations_per_sec_max": str(max(int(row["cpu_migrations_per_sec_max"] or 0) for row in mode_rows)),
        "major_page_fault_total": str(sum(int(row["major_page_fault_total"] or 0) for row in mode_rows)),
        "major_page_faults_per_sec_max": str(max(int(row["major_page_faults_per_sec_max"] or 0) for row in mode_rows)),
        "offcpu_time_events_total": str(sum(int(row["offcpu_time_events"] or 0) for row in mode_rows)),
        "live_effective_action_count_total": str(sum(int(row["live_effective_action_count"] or 0) for row in mode_rows)),
        "live_priority_limited_count_total": str(sum(int(row["live_priority_limited_count"] or 0) for row in mode_rows)),
    }
    for metric, _label in METRICS:
        values = [parse_float(row[metric]) for row in mode_rows]
        values = [value for value in values if value is not None]
        agg[f"{metric}_mean"] = fmt(statistics.mean(values) if values else None)
    aggregate_rows.append(agg)

baseline_by_scenario = {
    row["scenario"]: row
    for row in aggregate_rows
    if row["mode"] == "baseline"
}

for row in aggregate_rows:
    baseline = baseline_by_scenario.get(row["scenario"])
    for metric, _label in METRICS:
        baseline_value = parse_float(baseline.get(f"{metric}_mean", "")) if baseline else None
        mode_value = parse_float(row.get(f"{metric}_mean", ""))
        if baseline_value and mode_value is not None:
            delta = (baseline_value - mode_value) / baseline_value * 100.0
        else:
            delta = None
        row[f"{metric}_delta_vs_baseline_pct"] = pct_fmt(delta)

fieldnames = [
    "scenario",
    "scenario_label",
    "mode",
    "rounds_ok",
    "rounds_total",
    "samples_ok_total",
    "samples_total",
    "ttft_p95_ms_mean",
    "ttft_p99_ms_mean",
    "latency_p95_ms_mean",
    "latency_p99_ms_mean",
    "jitter_ms_mean",
    "ttft_p95_ms_delta_vs_baseline_pct",
    "ttft_p99_ms_delta_vs_baseline_pct",
    "latency_p95_ms_delta_vs_baseline_pct",
    "latency_p99_ms_delta_vs_baseline_pct",
    "jitter_ms_delta_vs_baseline_pct",
    "trigger_count_total",
    "rollback_count_total",
    "action_error_count_total",
    "cpu_migration_total",
    "cpu_migrations_per_sec_max",
    "major_page_fault_total",
    "major_page_faults_per_sec_max",
    "offcpu_time_events_total",
    "live_effective_action_count_total",
    "live_priority_limited_count_total",
]

with open(aggregate_path, "w", newline="", encoding="utf-8") as handle:
    writer = csv.DictWriter(handle, fieldnames=fieldnames)
    writer.writeheader()
    writer.writerows(aggregate_rows)

stable_improvements = []
live_stable_improvements = []
trend_notes = []
for scenario in sorted(scenario_labels):
    baseline_rows = [row for row in rows if row["scenario"] == scenario and row["mode"] == "baseline" and row["run_status"] == "0"]
    if not baseline_rows:
        trend_notes.append(f"- {scenario_labels[scenario]}: no successful baseline rounds; cannot judge benefit.")
        continue

    for mode in sorted({row["mode"] for row in rows if row["scenario"] == scenario and row["mode"] not in ("baseline", "missing")}):
        mode_rows = [row for row in rows if row["scenario"] == scenario and row["mode"] == mode and row["run_status"] == "0"]
        if not mode_rows:
            trend_notes.append(f"- {scenario_labels[scenario]} / {mode}: no successful rounds; cannot judge benefit.")
            continue

        for metric, label in METRICS:
            wins = 0
            comparisons = 0
            deltas = []
            for base, candidate in zip(baseline_rows, mode_rows):
                base_value = parse_float(base[metric])
                candidate_value = parse_float(candidate[metric])
                if base_value is None or candidate_value is None or base_value <= 0:
                    continue
                comparisons += 1
                delta = (base_value - candidate_value) / base_value * 100.0
                deltas.append(delta)
                if delta > 0:
                    wins += 1
            if comparisons == 0:
                continue
            mean_delta = statistics.mean(deltas)
            stable = wins >= max(2, math.ceil(comparisons * 2 / 3)) and mean_delta >= 5.0
            if stable:
                stable_improvements.append((scenario_labels[scenario], mode, label, wins, comparisons, mean_delta))
                if mode == "live_guarded":
                    live_stable_improvements.append((scenario_labels[scenario], mode, label, wins, comparisons, mean_delta))

live_effective_action_count = sum(
    int(row["live_effective_action_count"] or 0)
    for row in rows
    if row["mode"] == "live_guarded" and row["run_status"] == "0"
)
live_priority_limited_count = sum(
    int(row["live_priority_limited_count"] or 0)
    for row in rows
    if row["mode"] == "live_guarded" and row["run_status"] == "0"
)
live_host_effective = live_effective_action_count > 0
mode_contracts_pass = all(
    row["mode"] == "missing" or row["run_status"] == "0"
    for row in rows
)

if live_stable_improvements and live_host_effective:
    mvp_result = "PASS"
    verdict = "MVP benefit observed: live_guarded shows a stable improvement trend with effective host-level actuator changes."
elif live_stable_improvements:
    mvp_result = "FAIL"
    verdict = "MVP benefit not proven: live_guarded trend was observed, but live actuator changes were priority-limited or no-op."
else:
    mvp_result = "FAIL"
    verdict = "MVP benefit not proven: no live guarded mode met the stable improvement threshold."

detail_headers = [
    "scenario",
    "round",
    "status",
    "mode",
    "ok/total",
    "TTFT P95",
    "TTFT P99",
    "lat P95",
    "lat P99",
    "jitter",
    "triggers",
    "rollbacks",
    "action errors",
    "cpu mig total",
    "maj fault total",
    "live effective actions",
    "live priority-limited",
]
detail_lines = [
    "| " + " | ".join(detail_headers) + " |",
    "| " + " | ".join(["---"] * len(detail_headers)) + " |",
]
for row in rows:
    detail_lines.append("| " + " | ".join([
        row["scenario_label"],
        row["round"],
        row["run_status"],
        row["mode"],
        f'{row["samples_ok"]}/{row["samples_total"]}',
        row["ttft_p95_ms"],
        row["ttft_p99_ms"],
        row["latency_p95_ms"],
        row["latency_p99_ms"],
        row["jitter_ms"],
        row["trigger_count"],
        row["rollback_count"],
        row["action_error_count"],
        row["cpu_migration_total"],
        row["major_page_fault_total"],
        row["live_effective_action_count"],
        row["live_priority_limited_count"],
    ]) + " |")

agg_headers = [
    "scenario",
    "mode",
    "rounds",
    "samples",
    "TTFT P95 mean",
    "TTFT P99 mean",
    "lat P95 mean",
    "lat P99 mean",
    "jitter mean",
    "cpu mig total",
    "maj fault total",
    "TTFT P95 delta %",
    "TTFT P99 delta %",
    "lat P95 delta %",
    "lat P99 delta %",
    "jitter delta %",
    "live effective actions",
    "live priority-limited",
]
agg_lines = [
    "| " + " | ".join(agg_headers) + " |",
    "| " + " | ".join(["---"] * len(agg_headers)) + " |",
]
for row in aggregate_rows:
    agg_lines.append("| " + " | ".join([
        row["scenario_label"],
        row["mode"],
        f'{row["rounds_ok"]}/{row["rounds_total"]}',
        f'{row["samples_ok_total"]}/{row["samples_total"]}',
        row["ttft_p95_ms_mean"],
        row["ttft_p99_ms_mean"],
        row["latency_p95_ms_mean"],
        row["latency_p99_ms_mean"],
        row["jitter_ms_mean"],
        row["cpu_migration_total"],
        row["major_page_fault_total"],
        row["ttft_p95_ms_delta_vs_baseline_pct"],
        row["ttft_p99_ms_delta_vs_baseline_pct"],
        row["latency_p95_ms_delta_vs_baseline_pct"],
        row["latency_p99_ms_delta_vs_baseline_pct"],
        row["jitter_ms_delta_vs_baseline_pct"],
        row["live_effective_action_count_total"],
        row["live_priority_limited_count_total"],
    ]) + " |")

stable_lines = []
if stable_improvements:
    for scenario_label, mode, label, wins, comparisons, mean_delta in stable_improvements:
        stable_lines.append(f"- {scenario_label} / {mode} / {label}: {wins}/{comparisons} rounds improved, mean delta {mean_delta:.2f}%.")
else:
    stable_lines.append("- No metric crossed the stable trend rule: at least two thirds of comparable rounds improved and mean improvement was at least 5%.")
if stable_improvements and not live_stable_improvements:
    stable_lines.append("- Apparent improvements were limited to observation or dry-run modes, so they are treated as non-proof for MVP benefit.")
if live_stable_improvements and not live_host_effective:
    stable_lines.append("- Live guarded trend is treated as non-proof because no effective live actuator changes were observed.")

failure_lines = []
for row in rows:
    if row["mode"] != "live_guarded" or row["run_status"] == "0":
        continue
    reasons = []
    if row["samples_ok"] != row["samples_total"]:
        reasons.append(f'requests {row["samples_ok"]}/{row["samples_total"]}')
    if int(row["trigger_count"] or 0) <= 0:
        reasons.append("no inference_tail_guard trigger")
    if int(row["rollback_count"] or 0) <= 0:
        reasons.append("no rollback")
    if int(row["action_error_count"] or 0) > 0:
        reasons.append(f'{row["action_error_count"]} action audit error(s)')
    if not reasons:
        reasons.append("mode contract failed")
    failure_lines.append(f'- {row["scenario_label"]} round {row["round"]}: {", ".join(reasons)}.')
if not failure_lines:
    failure_lines.append("- No live guarded mode contract failures were recorded.")
failure_lines.append(f"- Selected mode contracts: `{'PASS' if mode_contracts_pass else 'FAIL'}`.")
failure_lines.append(f"- Live effective host-level actuator changes: `{live_effective_action_count}`.")
failure_lines.append(f"- Live priority-limited/no-op nice applications: `{live_priority_limited_count}`.")
if not live_host_effective:
    failure_lines.append(f"- Live guarded recorded no effective host-level actuator changes; priority-limited no-op nice applications: {live_priority_limited_count}.")

content = [
    "# MVP Benefit Report",
    "",
    "## Verdict",
    "",
    f"- Result: `{mvp_result}`",
    f"- Conclusion: {verdict}",
    f"- Run ID: `{run_id}`",
    "",
    "## Controls",
    "",
    f"- Model: `{model}`",
    f"- Num predict: `{num_predict}`",
    f"- Rounds per scenario: `{rounds}`",
    f"- Samples per mode: `{samples}`",
    f"- Concurrency: `{concurrency}`",
    f"- Modes: `{modes}`",
    f"- Scenarios: `{scenarios}`",
    f"- Interference shape: `cpu_workers={cpu_workers}; io_workers={io_workers}; hdd_workers={hdd_workers}; hdd_bytes={hdd_bytes}`",
    f"- Live actuator confirmation: `{live_confirm}`",
    f"- Live PID allowlist: `{live_pid_allowlist}`",
    f"- Live affinity enabled: `{live_enable_affinity}`",
    "",
    "## Aggregate Comparison",
    "",
    *agg_lines,
    "",
    "## Per-Round Comparison",
    "",
    *detail_lines,
    "",
    "## Stable Trend Check",
    "",
    *stable_lines,
    "",
    "## Live Guarded Contract",
    "",
    *failure_lines,
    "",
    "## Interpretation",
    "",
    "- `dry_run` and `noop_observation` validate recognition, trigger, audit, and rollback paths but do not by themselves prove host-level performance benefit.",
    "- `cpu_migration` and `major_page_fault` columns are procfs-backed explainability signals for the run shape; they do not replace the live guarded latency benefit rule.",
    "- `offcpu_time` can be sourced from the real eBPF helper when available, but it is not a blocking benefit gate in this report.",
    "- Host-level MVP benefit requires a real guarded actuator run to show a stable downward trend in tail latency, TTFT, or jitter.",
    "- If live `renice` is denied by host permissions, the report remains a closed-loop validation artifact, not a benefit proof.",
    "",
    "## Artifacts",
    "",
    f"- Detail CSV: `{detail_path}`",
    f"- Aggregate CSV: `{aggregate_path}`",
]

with open(summary_path, "w", encoding="utf-8") as handle:
    handle.write("\n".join(content))
    handle.write("\n")

print(mvp_result)
PY

report_status="$(head -n 1 "${SUMMARY_MD}" >/dev/null 2>&1; python3 - "${SUMMARY_MD}" <<'PY'
import re
import sys
text = open(sys.argv[1], encoding="utf-8").read()
match = re.search(r"Result: `([^`]+)`", text)
print(match.group(1) if match else "FAIL")
PY
)"

cp "${SUMMARY_MD}" "${REPORT_MD}"

append_log ""
append_log "#### Phase 4 MVP benefit report summary"
append_log ""
append_log "- Detail CSV: \`${DETAIL_CSV}\`"
append_log "- Aggregate CSV: \`${AGGREGATE_CSV}\`"
append_log "- Report: \`${REPORT_MD}\`"
append_log "- Harness aggregate exit status: \`${overall_status}\`"
append_log "- Benefit verdict: \`${report_status}\`"

printf '%s\n' "Phase 4 MVP benefit report:"
cat "${SUMMARY_MD}"
printf '%s\n' "Report: ${REPORT_MD}"
printf '%s\n' "Artifacts: ${ARTIFACT_DIR}"
printf '%s\n' "Verification log: ${LOG_PATH}"

if [[ "${overall_status}" -ne 0 || "${report_status}" != "PASS" ]]; then
  exit 1
fi
exit 0
