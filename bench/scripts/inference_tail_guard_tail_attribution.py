#!/usr/bin/env python3
"""Build an attribution report for Inference Tail Guard tail latency.

The report is intentionally artifact-only: it reads Phase 4 CSVs and daemon
logs, computes explainability rollups, and writes a documented handoff for the
next isolation work. It does not run Ollama, stress-ng, the daemon, or any live
actuator command.
"""

from __future__ import annotations

import argparse
import csv
import json
import os
import pathlib
import re
import statistics
import sys
from dataclasses import asdict, dataclass
from datetime import datetime, timezone
from typing import Iterable


REPO_ROOT = pathlib.Path(__file__).resolve().parents[2]
DEFAULT_PHASE4_RUN_ID = "live_guarded_phase4_sample_sizing_20260511T000000Z"
DEFAULT_PHASE4_ROOT = REPO_ROOT / ".cache" / "aegisai" / "inference_tail_guard_phase4"
DEFAULT_REPORT_PATH = REPO_ROOT / "docs" / "tail_guard_attribution_report.md"
KERNEL_CGROUP_V2_DOC = "https://docs.kernel.org/admin-guide/cgroup-v2.html"
KERNEL_SCHEDSTATS_DOC = "https://docs.kernel.org/scheduler/sched-stats.html"


@dataclass(frozen=True)
class DaemonSignals:
    run_queue_delay_events: int = 0
    run_queue_delay_total_us: int = 0
    run_queue_delay_max_us: int = 0
    cpu_migration_events: int = 0
    cpu_migration_total: int = 0
    major_page_fault_events: int = 0
    major_page_fault_total: int = 0
    offcpu_time_events: int = 0
    offcpu_time_max_us: int = 0
    io_latency_events: int = 0
    io_latency_max_us: int = 0
    apply_audit_count: int = 0
    rollback_audit_count: int = 0
    timing_capture: str = "not_captured"


@dataclass(frozen=True)
class AttributionRow:
    scenario: str
    scenario_label: str
    round: str
    mode: str
    latency_p95_ms: float | None
    latency_p99_ms: float | None
    ttft_p95_ms: float | None
    model_runtime_p95_ms: float | None
    run_queue_delay_ms_max: float
    offcpu_time_ms_max: float
    io_latency_ms_max: float
    duration_backed_scheduler_ms: float
    scheduler_attributable_tail_pct_p95: float | None
    cpu_migration_total: int
    cpu_migration_event_share_pct: float
    major_page_fault_total: int
    major_page_fault_event_share_pct: float
    offcpu_time_events: int
    io_latency_events: int
    trigger_count: int
    apply_audit_count: int
    rollback_count: int
    rollback_audit_count: int
    timing_capture: str
    artifact_dir: str


@dataclass(frozen=True)
class AttributionSummary:
    run_id: str
    baseline_rounds: int
    baseline_latency_p95_ms_mean: float | None
    baseline_latency_p99_ms_mean: float | None
    live_latency_p95_delta_pct: float | None
    live_latency_p99_delta_pct: float | None
    scheduler_attributable_tail_pct_max: float
    scheduler_attributable_tail_pct_mean: float
    run_queue_delay_ms_max: float
    offcpu_time_ms_max: float
    io_latency_ms_max: float
    cpu_migration_total: int
    major_page_fault_total: int
    offcpu_time_events_total: int
    io_latency_events_total: int
    live_effective_action_count_total: int
    live_trigger_count_total: int
    live_rollback_count_total: int
    live_apply_audit_count_total: int
    live_rollback_audit_count_total: int
    timing_capture_status: str
    helper_backed_signal_status: str
    p95_or_p99_15pct_plausibility: str
    plausibility_reason: str


def parse_args(argv: list[str]) -> argparse.Namespace:
    parser = argparse.ArgumentParser(description=__doc__)
    default_run_id = os.environ.get("AEGISAI_TAIL_ATTRIBUTION_RUN_ID", DEFAULT_PHASE4_RUN_ID)
    parser.add_argument(
        "--phase4-runs",
        type=pathlib.Path,
        default=pathlib.Path(os.environ["AEGISAI_TAIL_ATTRIBUTION_PHASE4_RUNS"])
        if os.environ.get("AEGISAI_TAIL_ATTRIBUTION_PHASE4_RUNS")
        else None,
        help="Phase 4 phase4_runs.csv to attribute.",
    )
    parser.add_argument(
        "--run-id",
        default=default_run_id,
        help="Logical run id to write into the report.",
    )
    parser.add_argument(
        "--artifact-dir",
        type=pathlib.Path,
        default=pathlib.Path(os.environ["AEGISAI_TAIL_ATTRIBUTION_ARTIFACT_DIR"])
        if os.environ.get("AEGISAI_TAIL_ATTRIBUTION_ARTIFACT_DIR")
        else None,
        help="Directory for attribution artifacts.",
    )
    parser.add_argument(
        "--report",
        type=pathlib.Path,
        default=pathlib.Path(os.environ.get("AEGISAI_TAIL_ATTRIBUTION_REPORT", DEFAULT_REPORT_PATH)),
        help="Markdown report path.",
    )
    parser.add_argument(
        "--plausibility-threshold-pct",
        type=float,
        default=float(os.environ.get("AEGISAI_TAIL_ATTRIBUTION_THRESHOLD_PCT", "15.0")),
        help="Target P95/P99 improvement threshold.",
    )
    args = parser.parse_args(argv)
    if args.phase4_runs is None:
        args.phase4_runs = DEFAULT_PHASE4_ROOT / args.run_id / "phase4_runs.csv"
    if args.artifact_dir is None:
        args.artifact_dir = (
            REPO_ROOT / ".cache" / "aegisai" / "inference_tail_guard_tail_attribution" / args.run_id
        )
    return args


def parse_float(value: str | None) -> float | None:
    if value is None or value in {"", "missing", "n/a"}:
        return None
    try:
        return float(value)
    except ValueError:
        return None


def parse_int(value: str | None) -> int:
    if value is None or value in {"", "missing", "n/a"}:
        return 0
    try:
        return int(float(value))
    except ValueError:
        return 0


def fmt(value: float | None) -> str:
    if value is None:
        return "n/a"
    return f"{value:.3f}"


def pct_fmt(value: float | None) -> str:
    if value is None:
        return "n/a"
    return f"{value:.2f}"


def mean(values: Iterable[float | None]) -> float | None:
    clean = [value for value in values if value is not None]
    if not clean:
        return None
    return statistics.mean(clean)


def pct_delta(before: float | None, after: float | None) -> float | None:
    if before is None or after is None or before <= 0:
        return None
    return (before - after) / before * 100.0


def parse_kv_body(body: str) -> dict[str, int]:
    values: dict[str, int] = {}
    for part in body.split():
        if "=" not in part:
            continue
        key, raw = part.split("=", 1)
        raw = raw.rstrip(",")
        if raw.isdigit():
            values[key] = int(raw)
    return values


def parse_daemon_log(round_dir: pathlib.Path, mode: str) -> DaemonSignals:
    path = round_dir / mode / "daemon.log"
    try:
        text = path.read_text(encoding="utf-8")
    except FileNotFoundError:
        return DaemonSignals(timing_capture="no_daemon_log")

    signal_rows: dict[str, dict[str, int]] = {}
    maxima: dict[str, int] = {}
    for line in text.splitlines():
        match = re.match(r"^\s{2}([a-zA-Z0-9_]+):\s+(.*)$", line)
        if not match:
            continue
        name, body = match.groups()
        if "=" in body:
            signal_rows[name] = parse_kv_body(body)
        elif body.strip().isdigit():
            maxima[name] = int(body.strip())

    apply_audit_count = len(re.findall(r"backend\.apply\.apply\.\d+\.detail=", text))
    rollback_audit_count = len(re.findall(r"backend\.rollback\.rollback\.\d+\.detail=", text))
    if apply_audit_count or rollback_audit_count:
        timing_capture = "audit_counts_only"
    elif mode == "baseline":
        timing_capture = "not_applicable_baseline"
    else:
        timing_capture = "not_captured"

    run_queue = signal_rows.get("run_queue_delay", {})
    cpu_migration = signal_rows.get("cpu_migration", {})
    major_fault = signal_rows.get("major_page_fault", {})
    offcpu = signal_rows.get("offcpu_time", {})
    io_latency = signal_rows.get("io_latency", {})
    return DaemonSignals(
        run_queue_delay_events=run_queue.get("events", 0),
        run_queue_delay_total_us=run_queue.get("total", 0),
        run_queue_delay_max_us=max(run_queue.get("max", 0), maxima.get("run_queue_delay_us_max", 0)),
        cpu_migration_events=cpu_migration.get("events", 0),
        cpu_migration_total=cpu_migration.get("total", 0),
        major_page_fault_events=major_fault.get("events", 0),
        major_page_fault_total=major_fault.get("total", 0),
        offcpu_time_events=offcpu.get("events", 0),
        offcpu_time_max_us=max(offcpu.get("max", 0), maxima.get("offcpu_time_us_max", 0)),
        io_latency_events=io_latency.get("events", 0),
        io_latency_max_us=max(io_latency.get("max", 0), maxima.get("optional_io_latency_us_max", 0)),
        apply_audit_count=apply_audit_count,
        rollback_audit_count=rollback_audit_count,
        timing_capture=timing_capture,
    )


def attribution_row(raw: dict[str, str]) -> AttributionRow:
    round_dir = pathlib.Path(raw["artifact_dir"])
    mode = raw["mode"]
    signals = parse_daemon_log(round_dir, mode)
    latency_p95 = parse_float(raw.get("latency_p95_ms"))
    latency_p99 = parse_float(raw.get("latency_p99_ms"))
    ttft_p95 = parse_float(raw.get("ttft_p95_ms"))
    model_runtime = (
        max(0.0, latency_p95 - ttft_p95)
        if latency_p95 is not None and ttft_p95 is not None
        else None
    )
    run_queue_ms = signals.run_queue_delay_max_us / 1000.0
    offcpu_ms = signals.offcpu_time_max_us / 1000.0
    io_ms = signals.io_latency_max_us / 1000.0
    duration_backed_ms = run_queue_ms + offcpu_ms + io_ms
    scheduler_pct = (
        min(100.0, duration_backed_ms / latency_p95 * 100.0)
        if latency_p95 is not None and latency_p95 > 0
        else None
    )

    migration_total = max(parse_int(raw.get("cpu_migration_total")), signals.cpu_migration_total)
    fault_total = max(parse_int(raw.get("major_page_fault_total")), signals.major_page_fault_total)
    offcpu_events = max(parse_int(raw.get("offcpu_time_events")), signals.offcpu_time_events)
    io_events = signals.io_latency_events
    total_event_pressure = migration_total + fault_total + offcpu_events + io_events
    migration_share = migration_total / total_event_pressure * 100.0 if total_event_pressure else 0.0
    fault_share = fault_total / total_event_pressure * 100.0 if total_event_pressure else 0.0
    return AttributionRow(
        scenario=raw["scenario"],
        scenario_label=raw["scenario_label"],
        round=raw["round"],
        mode=mode,
        latency_p95_ms=latency_p95,
        latency_p99_ms=latency_p99,
        ttft_p95_ms=ttft_p95,
        model_runtime_p95_ms=model_runtime,
        run_queue_delay_ms_max=run_queue_ms,
        offcpu_time_ms_max=offcpu_ms,
        io_latency_ms_max=io_ms,
        duration_backed_scheduler_ms=duration_backed_ms,
        scheduler_attributable_tail_pct_p95=scheduler_pct,
        cpu_migration_total=migration_total,
        cpu_migration_event_share_pct=migration_share,
        major_page_fault_total=fault_total,
        major_page_fault_event_share_pct=fault_share,
        offcpu_time_events=offcpu_events,
        io_latency_events=io_events,
        trigger_count=parse_int(raw.get("trigger_count")),
        apply_audit_count=signals.apply_audit_count,
        rollback_count=parse_int(raw.get("rollback_count")),
        rollback_audit_count=signals.rollback_audit_count,
        timing_capture=signals.timing_capture,
        artifact_dir=raw["artifact_dir"],
    )


def summarize(
    run_id: str,
    rows: list[AttributionRow],
    raw_rows: list[dict[str, str]],
    threshold_pct: float,
) -> AttributionSummary:
    baseline = [row for row in rows if row.mode == "baseline"]
    live = [row for row in rows if row.mode == "live_guarded"]
    observer_rows = [row for row in rows if row.mode != "baseline"]
    baseline_p95 = mean(row.latency_p95_ms for row in baseline)
    baseline_p99 = mean(row.latency_p99_ms for row in baseline)
    live_p95 = mean(row.latency_p95_ms for row in live)
    live_p99 = mean(row.latency_p99_ms for row in live)
    scheduler_pcts = [
        row.scheduler_attributable_tail_pct_p95 or 0.0
        for row in observer_rows
    ]
    timing_statuses = {row.timing_capture for row in live}
    timing_capture_status = (
        "audit_counts_only"
        if timing_statuses == {"audit_counts_only"}
        else ",".join(sorted(timing_statuses)) or "not_captured"
    )
    offcpu_total = sum(row.offcpu_time_events for row in observer_rows)
    io_total = sum(row.io_latency_events for row in observer_rows)
    helper_status = "included" if offcpu_total > 0 and io_total > 0 else "excluded_or_zero"
    max_scheduler_pct = max(scheduler_pcts, default=0.0)
    live_delta_p95 = pct_delta(baseline_p95, live_p95)
    live_delta_p99 = pct_delta(baseline_p99, live_p99)
    observed_target = max(
        [value for value in (live_delta_p95, live_delta_p99) if value is not None],
        default=0.0,
    )

    if observed_target >= threshold_pct:
        plausibility = "OBSERVED"
        reason = (
            f"Live guarded already reached >= {threshold_pct:.1f}% on latency P95/P99 "
            f"for this artifact set; observed best delta {observed_target:.2f}%."
        )
    elif max_scheduler_pct >= threshold_pct:
        plausibility = "PLAUSIBLE"
        reason = (
            f"Duration-backed scheduler/off-CPU/I/O signals reach {max_scheduler_pct:.2f}% "
            f"of a P95 tail sample, meeting the {threshold_pct:.1f}% target envelope."
        )
    elif helper_status != "included":
        plausibility = "NOT_PROVEN_HELPER_GAP"
        reason = (
            f"Captured duration-backed scheduler signals peak at {max_scheduler_pct:.2f}% "
            f"of P95, below {threshold_pct:.1f}%, and helper-backed offcpu/io signals "
            "were excluded or zero in these artifacts."
        )
    else:
        plausibility = "NOT_PROVEN"
        reason = (
            f"Captured duration-backed scheduler/off-CPU/I/O signals peak at "
            f"{max_scheduler_pct:.2f}% of P95, below {threshold_pct:.1f}%."
        )

    live_raw = [row for row in raw_rows if row.get("mode") == "live_guarded"]
    return AttributionSummary(
        run_id=run_id,
        baseline_rounds=len(baseline),
        baseline_latency_p95_ms_mean=baseline_p95,
        baseline_latency_p99_ms_mean=baseline_p99,
        live_latency_p95_delta_pct=live_delta_p95,
        live_latency_p99_delta_pct=live_delta_p99,
        scheduler_attributable_tail_pct_max=max_scheduler_pct,
        scheduler_attributable_tail_pct_mean=statistics.mean(scheduler_pcts) if scheduler_pcts else 0.0,
        run_queue_delay_ms_max=max((row.run_queue_delay_ms_max for row in observer_rows), default=0.0),
        offcpu_time_ms_max=max((row.offcpu_time_ms_max for row in observer_rows), default=0.0),
        io_latency_ms_max=max((row.io_latency_ms_max for row in observer_rows), default=0.0),
        cpu_migration_total=sum(row.cpu_migration_total for row in observer_rows),
        major_page_fault_total=sum(row.major_page_fault_total for row in observer_rows),
        offcpu_time_events_total=offcpu_total,
        io_latency_events_total=io_total,
        live_effective_action_count_total=sum(parse_int(row.get("live_effective_action_count")) for row in live_raw),
        live_trigger_count_total=sum(row.trigger_count for row in live),
        live_rollback_count_total=sum(row.rollback_count for row in live),
        live_apply_audit_count_total=sum(row.apply_audit_count for row in live),
        live_rollback_audit_count_total=sum(row.rollback_audit_count for row in live),
        timing_capture_status=timing_capture_status,
        helper_backed_signal_status=helper_status,
        p95_or_p99_15pct_plausibility=plausibility,
        plausibility_reason=reason,
    )


def row_for_csv(row: AttributionRow) -> dict[str, str]:
    data = asdict(row)
    for key, value in list(data.items()):
        if isinstance(value, float):
            data[key] = fmt(value)
        elif value is None:
            data[key] = "n/a"
        else:
            data[key] = str(value)
    return data


def write_csv(path: pathlib.Path, rows: list[AttributionRow]) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    fieldnames = list(asdict(rows[0]).keys()) if rows else list(AttributionRow.__dataclass_fields__)
    with path.open("w", newline="", encoding="utf-8") as handle:
        writer = csv.DictWriter(handle, fieldnames=fieldnames)
        writer.writeheader()
        for row in rows:
            writer.writerow(row_for_csv(row))


def markdown_table(headers: list[str], table_rows: list[list[str]]) -> list[str]:
    return [
        "| " + " | ".join(headers) + " |",
        "| " + " | ".join(["---"] * len(headers)) + " |",
        *["| " + " | ".join(row) + " |" for row in table_rows],
    ]


def build_report(
    summary: AttributionSummary,
    rows: list[AttributionRow],
    detail_path: pathlib.Path,
    csv_path: pathlib.Path,
    json_path: pathlib.Path,
) -> str:
    detail_lines = markdown_table(
        [
            "scenario",
            "round",
            "mode",
            "lat P95",
            "model/runtime P95",
            "runq max ms",
            "offcpu max ms",
            "I/O max ms",
            "sched tail %",
            "cpu mig",
            "maj faults",
            "triggers",
            "apply audits",
            "rollbacks",
            "rollback audits",
            "timing",
        ],
        [
            [
                row.scenario_label,
                row.round,
                row.mode,
                fmt(row.latency_p95_ms),
                fmt(row.model_runtime_p95_ms),
                fmt(row.run_queue_delay_ms_max),
                fmt(row.offcpu_time_ms_max),
                fmt(row.io_latency_ms_max),
                pct_fmt(row.scheduler_attributable_tail_pct_p95),
                str(row.cpu_migration_total),
                str(row.major_page_fault_total),
                str(row.trigger_count),
                str(row.apply_audit_count),
                str(row.rollback_count),
                str(row.rollback_audit_count),
                row.timing_capture,
            ]
            for row in rows
        ],
    )

    generated = datetime.now(timezone.utc).strftime("%Y-%m-%dT%H:%M:%SZ")
    return "\n".join(
        [
            "# Tail Guard Attribution Report",
            "",
            f"- Generated: `{generated}`",
            f"- Source Phase 4 detail CSV: `{detail_path}`",
            f"- Run ID: `{summary.run_id}`",
            f"- Baseline rounds: `{summary.baseline_rounds}`",
            f"- Baseline latency P95 mean: `{fmt(summary.baseline_latency_p95_ms_mean)} ms`",
            f"- Baseline latency P99 mean: `{fmt(summary.baseline_latency_p99_ms_mean)} ms`",
            f"- Live latency P95 delta: `{pct_fmt(summary.live_latency_p95_delta_pct)}%`",
            f"- Live latency P99 delta: `{pct_fmt(summary.live_latency_p99_delta_pct)}%`",
            f"- Duration-backed scheduler-attributable tail max: `{pct_fmt(summary.scheduler_attributable_tail_pct_max)}%`",
            f"- Duration-backed scheduler-attributable tail mean: `{pct_fmt(summary.scheduler_attributable_tail_pct_mean)}%`",
            f"- P95/P99 >=15% plausibility: `{summary.p95_or_p99_15pct_plausibility}`",
            f"- Plausibility reason: {summary.plausibility_reason}",
            "",
            "## Signal Rollup",
            "",
            f"- Run-queue delay max: `{fmt(summary.run_queue_delay_ms_max)} ms`",
            f"- Helper off-CPU max: `{fmt(summary.offcpu_time_ms_max)} ms`",
            f"- Helper I/O latency max: `{fmt(summary.io_latency_ms_max)} ms`",
            f"- CPU migration total: `{summary.cpu_migration_total}`",
            f"- Major page fault total: `{summary.major_page_fault_total}`",
            f"- Helper off-CPU events: `{summary.offcpu_time_events_total}`",
            f"- Helper I/O events: `{summary.io_latency_events_total}`",
            f"- Helper-backed signal status for Phase 5 planning: `{summary.helper_backed_signal_status}`",
            "",
            "## Trigger Apply Rollback Capture",
            "",
            f"- Live triggers: `{summary.live_trigger_count_total}`",
            f"- Live rollbacks: `{summary.live_rollback_count_total}`",
            f"- Live apply audit detail count: `{summary.live_apply_audit_count_total}`",
            f"- Live rollback audit detail count: `{summary.live_rollback_audit_count_total}`",
            f"- Live effective host actions: `{summary.live_effective_action_count_total}`",
            f"- Timing capture status: `{summary.timing_capture_status}`",
            "",
            "## Per-Round Attribution",
            "",
            *detail_lines,
            "",
            "## Interpretation",
            "",
            "- `model/runtime P95` is `latency_p95_ms - ttft_p95_ms`; it is the request body/runtime portion not explained by TTFT.",
            "- `sched tail %` is duration-backed `run_queue_delay + offcpu_time + io_latency` max divided by latency P95, capped at 100%.",
            "- CPU migration and major page fault columns are event-pressure attribution, not duration attribution; they should guide stronger isolation tests but are not added to duration-backed plausibility.",
            "- Trigger/apply/rollback timing is currently audit-count based in Phase 4 artifacts; exact apply and rollback timestamps are a remaining instrumentation gap.",
            "- Kernel cgroup v2 CPU controls support proportional `cpu.weight`, bandwidth `cpu.max`, and CPU pressure accounting, which is why the next live isolation work must stay scoped to an owned cgroup subtree.",
            "- Kernel scheduler stats expose per-process runqueue wait time through `/proc/<pid>/schedstat`; this report uses the daemon's procfs-derived `run_queue_delay` observations as the runqueue-wait attribution source.",
            "",
            "## References",
            "",
            f"- Linux kernel cgroup v2 CPU interface files: {KERNEL_CGROUP_V2_DOC}",
            f"- Linux kernel scheduler statistics and `/proc/<pid>/schedstat`: {KERNEL_SCHEDSTATS_DOC}",
            "",
            "## Artifacts",
            "",
            f"- Attribution CSV: `{csv_path}`",
            f"- Summary JSON: `{json_path}`",
        ]
    ) + "\n"


def run(args: argparse.Namespace) -> AttributionSummary:
    if not args.phase4_runs.is_file():
        raise FileNotFoundError(f"Phase 4 detail CSV not found: {args.phase4_runs}")
    with args.phase4_runs.open(newline="", encoding="utf-8") as handle:
        raw_rows = list(csv.DictReader(handle))
    if not raw_rows:
        raise ValueError(f"Phase 4 detail CSV has no rows: {args.phase4_runs}")

    rows = [attribution_row(raw) for raw in raw_rows if raw.get("mode") and raw.get("mode") != "missing"]
    summary = summarize(args.run_id, rows, raw_rows, args.plausibility_threshold_pct)

    args.artifact_dir.mkdir(parents=True, exist_ok=True)
    csv_path = args.artifact_dir / "tail_attribution.csv"
    json_path = args.artifact_dir / "tail_attribution_summary.json"
    write_csv(csv_path, rows)
    json_path.write_text(json.dumps(asdict(summary), indent=2, sort_keys=True) + "\n", encoding="utf-8")
    args.report.parent.mkdir(parents=True, exist_ok=True)
    args.report.write_text(
        build_report(summary, rows, args.phase4_runs, csv_path, json_path),
        encoding="utf-8",
    )
    return summary


def main(argv: list[str] | None = None) -> int:
    args = parse_args(argv or sys.argv[1:])
    try:
        summary = run(args)
    except Exception as error:  # pragma: no cover - CLI boundary
        print(f"tail_attribution=FAIL reason={error}", file=sys.stderr)
        return 1
    print(
        "tail_attribution="
        f"{summary.p95_or_p99_15pct_plausibility} "
        f"scheduler_attributable_tail_pct_max={summary.scheduler_attributable_tail_pct_max:.2f} "
        f"report={args.report} artifact_dir={args.artifact_dir}"
    )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
