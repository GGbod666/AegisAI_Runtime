#!/usr/bin/env python3
"""Summarize repeated Tool Call Booster A/B harness artifacts."""

from __future__ import annotations

import argparse
import csv
import json
import math
import re
import statistics
from pathlib import Path
from typing import Any


CONTRACT_STAGES = ("executor", "retrieval", "rerank")
GUARDED_MODES = {"guarded", "live_guarded", "linux_command", "linux-command"}
WARMUP_EXECUTOR_BOUNDARY = (
    "`WarmupExecutor` is plan/audit-only in the current backend: apply records "
    "`warmup executor deferred` and rollback records a no-op. No live "
    "executor/cache warmup side effect is implemented or required for this "
    "benefit verdict."
)


DETAIL_COLUMNS = [
    "round",
    "mode",
    "backend",
    "contract",
    "tool_call_id",
    "tool_call_latency_ms",
    "executor_ms",
    "retrieval_ms",
    "rerank_ms",
    "background_ms",
    "daemon_lifecycle_ms",
    "processed_events",
    "applied_actions",
    "total_rollbacks",
    "tool_call_booster_triggers",
    "executor_roles",
    "stages",
    "action_error_count",
    "artifact_prefix",
    "contract_reason",
]


SUMMARY_COLUMNS = [
    "mode",
    "backend",
    "mode_contract",
    "rounds",
    "contract_pass_rounds",
    "tool_call_latency_median_ms",
    "tool_call_latency_avg_ms",
    "baseline_latency_median_ms",
    "comparable_rounds",
    "improved_rounds",
    "avg_delta_vs_baseline_pct",
    "median_delta_vs_baseline_pct",
    "trigger_count_total",
    "rollback_count_total",
    "action_error_count_total",
    "latency_trend_verdict",
    "benefit_verdict",
    "verdict_reason",
]


def as_float(value: Any) -> float | None:
    try:
        return float(value)
    except (TypeError, ValueError):
        return None


def as_int(value: Any) -> int:
    try:
        return int(value)
    except (TypeError, ValueError):
        return 0


def fmt_float(value: float | None) -> str:
    if value is None or math.isnan(value):
        return ""
    return f"{value:.3f}"


def critical_chain_latency_ms(durations: dict[str, Any]) -> float | None:
    values = [as_float(durations.get(stage)) for stage in CONTRACT_STAGES]
    if any(value is None for value in values):
        return None
    return max(value for value in values if value is not None)


def parse_modes(raw_modes: str) -> list[str]:
    modes = [mode.strip() for mode in raw_modes.split(",") if mode.strip()]
    if not modes:
        raise ValueError("at least one mode is required")
    if "baseline" not in modes:
        raise ValueError("modes must include baseline for A/B comparison")
    return modes


def role_name(raw_role: str) -> str:
    if raw_role == "tool-executor":
        return "executor"
    return raw_role.removesuffix("-worker")


def parse_executor_stdout(path: Path) -> dict[str, Any]:
    result: dict[str, Any] = {
        "tool_call_id": "",
        "role_count": 0,
        "child_status_ok": True,
        "durations": {},
    }
    if not path.exists():
        result["child_status_ok"] = False
        return result

    for line in path.read_text(errors="replace").splitlines():
        try:
            record = json.loads(line)
        except json.JSONDecodeError:
            continue
        if not isinstance(record, dict):
            continue

        raw_role = str(record.get("role", ""))
        if raw_role:
            result["role_count"] += 1
        if not result["tool_call_id"] and record.get("tool_call_id"):
            result["tool_call_id"] = str(record["tool_call_id"])

        duration_ms = as_float(record.get("duration_ms"))
        if duration_ms is not None and raw_role:
            result["durations"][role_name(raw_role)] = duration_ms

        child_statuses = record.get("child_statuses")
        if isinstance(child_statuses, list):
            result["child_status_ok"] = all(as_int(status) == 0 for status in child_statuses)

    return result


def int_field(text: str, key: str) -> int:
    match = re.search(rf"^{re.escape(key)}:\s+([0-9]+)$", text, re.MULTILINE)
    if not match:
        return 0
    return int(match.group(1))


def parse_lifecycle(text: str, tool_call_id: str) -> dict[str, Any]:
    if not tool_call_id:
        return {}

    pattern = re.compile(
        rf"^\s+{re.escape(tool_call_id)}:\s+duration_ms=([0-9.]+)\s+"
        r"stages=([^ ]+)\s+boosted_actions=([0-9]+)\s+"
        r"(?:rollback_actions=([0-9]+)\s+)?"
        r"background_events=([0-9]+)\s+isolation_events=([0-9]+)\s+pids=(.*)$",
        re.MULTILINE,
    )
    match = pattern.search(text)
    if not match:
        return {}
    return {
        "daemon_lifecycle_ms": as_float(match.group(1)),
        "stages": match.group(2),
        "boosted_actions": int(match.group(3)),
        "rollback_actions": int(match.group(4) or 0),
        "background_events": int(match.group(5)),
        "isolation_events": int(match.group(6)),
        "pids": match.group(7),
    }


def parse_daemon_stdout(path: Path, tool_call_id: str) -> dict[str, Any]:
    result: dict[str, Any] = {
        "backend": "none",
        "processed_events": 0,
        "applied_actions": 0,
        "total_rollbacks": 0,
        "tool_call_booster_triggers": 0,
        "daemon_lifecycle_ms": None,
        "stages": "",
        "action_error_count": 0,
    }
    if not path.exists():
        return result

    text = path.read_text(errors="replace")
    backend_match = re.search(r"^actuator_backend:\s+(.+)$", text, re.MULTILINE)
    if backend_match:
        result["backend"] = backend_match.group(1).strip()

    inline_rollbacks = int_field(text, "inline_rollbacks")
    tick_rollbacks = int_field(text, "tick_rollbacks")
    result.update(
        {
            "processed_events": int_field(text, "processed_events"),
            "applied_actions": int_field(text, "applied_actions"),
            "total_rollbacks": inline_rollbacks + tick_rollbacks,
            "tool_call_booster_triggers": int_field(text, "  tool_call_booster"),
            "action_error_count": count_action_errors(text),
        }
    )

    lifecycle = parse_lifecycle(text, tool_call_id)
    if lifecycle:
        result["daemon_lifecycle_ms"] = lifecycle["daemon_lifecycle_ms"]
        result["stages"] = lifecycle["stages"]

    return result


def count_action_errors(text: str) -> int:
    errors = 0
    errors += len(re.findall(r"status=error", text))
    errors += len(re.findall(r"apply\.result=error", text))
    errors += len(re.findall(r"rollback\.result=error", text))
    for failed_count in re.findall(r"failed_count=([0-9]+)", text):
        errors += int(failed_count)
    return errors


def mode_backend(mode: str, daemon_backend: str) -> str:
    if mode == "baseline":
        return "none"
    if daemon_backend != "none":
        return daemon_backend
    if mode == "dry_run":
        return "linux-command-dry-run"
    if mode in GUARDED_MODES:
        return "linux-command"
    return "noop"


def build_detail_rows(artifact_dir: Path, modes: list[str], rounds: int) -> list[dict[str, str]]:
    rows: list[dict[str, str]] = []
    for round_no in range(1, rounds + 1):
        for mode in modes:
            prefix = f"round{round_no}.{mode}"
            executor_path = artifact_dir / f"executor.{prefix}.stdout"
            daemon_path = artifact_dir / f"daemon.{prefix}.stdout"
            executor = parse_executor_stdout(executor_path)
            durations = executor["durations"]
            tool_call_id = str(executor["tool_call_id"])
            daemon = parse_daemon_stdout(daemon_path, tool_call_id)

            reasons = contract_reasons(mode, executor, daemon)
            contract = "PASS" if not reasons else "FAIL"
            backend = mode_backend(mode, str(daemon["backend"]))
            rows.append(
                {
                    "round": str(round_no),
                    "mode": mode,
                    "backend": backend,
                    "contract": contract,
                    "tool_call_id": tool_call_id,
                    "tool_call_latency_ms": fmt_float(critical_chain_latency_ms(durations)),
                    "executor_ms": fmt_float(as_float(durations.get("executor"))),
                    "retrieval_ms": fmt_float(as_float(durations.get("retrieval"))),
                    "rerank_ms": fmt_float(as_float(durations.get("rerank"))),
                    "background_ms": fmt_float(as_float(durations.get("background"))),
                    "daemon_lifecycle_ms": fmt_float(as_float(daemon["daemon_lifecycle_ms"])),
                    "processed_events": str(daemon["processed_events"]),
                    "applied_actions": str(daemon["applied_actions"]),
                    "total_rollbacks": str(daemon["total_rollbacks"]),
                    "tool_call_booster_triggers": str(daemon["tool_call_booster_triggers"]),
                    "executor_roles": str(executor["role_count"]),
                    "stages": str(daemon["stages"] or "none"),
                    "action_error_count": str(daemon["action_error_count"]),
                    "artifact_prefix": prefix,
                    "contract_reason": ";".join(reasons) if reasons else "ok",
                }
            )
    return rows


def contract_reasons(mode: str, executor: dict[str, Any], daemon: dict[str, Any]) -> list[str]:
    reasons: list[str] = []
    durations = executor["durations"]
    for stage in CONTRACT_STAGES:
        if as_float(durations.get(stage)) is None:
            reasons.append(f"missing_{stage}_latency")
    if executor["role_count"] < 4:
        reasons.append("missing_executor_roles")
    if not executor["child_status_ok"]:
        reasons.append("child_status_nonzero")

    if mode == "baseline":
        return reasons

    if daemon["processed_events"] <= 0:
        reasons.append("no_processed_events")
    if daemon["applied_actions"] <= 0:
        reasons.append("no_applied_actions")
    if daemon["tool_call_booster_triggers"] <= 0:
        reasons.append("no_tool_call_booster_trigger")
    if daemon["total_rollbacks"] <= 0:
        reasons.append("no_rollback")
    stages = str(daemon["stages"])
    for stage in CONTRACT_STAGES:
        if f"{stage}:" not in stages:
            reasons.append(f"missing_{stage}_stage")
    if daemon["action_error_count"] > 0:
        reasons.append("action_audit_errors")
    return reasons


def latency(row: dict[str, str]) -> float | None:
    return as_float(row["tool_call_latency_ms"])


def mean(values: list[float]) -> float | None:
    if not values:
        return None
    return statistics.fmean(values)


def median(values: list[float]) -> float | None:
    if not values:
        return None
    return statistics.median(values)


def build_summary_rows(
    rows: list[dict[str, str]], modes: list[str], rounds: int, min_benefit_pct: float
) -> list[dict[str, str]]:
    summary_rows: list[dict[str, str]] = []
    baseline_by_round = {
        row["round"]: row for row in rows if row["mode"] == "baseline" and row["contract"] == "PASS"
    }
    baseline_latencies = [
        value for row in baseline_by_round.values() if (value := latency(row)) is not None
    ]
    baseline_median = median(baseline_latencies)

    for mode in modes:
        mode_rows = [row for row in rows if row["mode"] == mode]
        pass_rows = [row for row in mode_rows if row["contract"] == "PASS"]
        latencies = [value for row in pass_rows if (value := latency(row)) is not None]
        deltas: list[float] = []
        for row in pass_rows:
            if mode == "baseline":
                continue
            baseline = baseline_by_round.get(row["round"])
            if baseline is None:
                continue
            base_latency = latency(baseline)
            mode_latency = latency(row)
            if base_latency is None or mode_latency is None or base_latency <= 0:
                continue
            deltas.append(((mode_latency - base_latency) / base_latency) * 100.0)

        comparable_rounds = len(deltas)
        required_improved_rounds = math.ceil(comparable_rounds * 2 / 3) if comparable_rounds else 0
        improved_rounds = sum(1 for delta in deltas if delta <= -min_benefit_pct)
        avg_delta = mean(deltas)
        median_delta = median(deltas)
        mode_contract_pass = len(pass_rows) == rounds
        latency_trend_pass = (
            mode != "baseline"
            and mode_contract_pass
            and comparable_rounds > 0
            and improved_rounds >= required_improved_rounds
            and avg_delta is not None
            and avg_delta <= -min_benefit_pct
        )
        guarded_mode = mode in GUARDED_MODES
        benefit_pass = latency_trend_pass and guarded_mode

        if mode == "baseline":
            trend_verdict = "BASELINE"
            benefit_verdict = "BASELINE"
            reason = "baseline reference"
        else:
            trend_verdict = "PASS" if latency_trend_pass else "FAIL"
            benefit_verdict = "PASS" if benefit_pass else "FAIL"
            reason = verdict_reason(
                mode,
                mode_contract_pass,
                guarded_mode,
                comparable_rounds,
                required_improved_rounds,
                improved_rounds,
                avg_delta,
                min_benefit_pct,
            )

        summary_rows.append(
            {
                "mode": mode,
                "backend": mode_backend(mode, mode_rows[0]["backend"] if mode_rows else "none"),
                "mode_contract": "PASS" if mode_contract_pass else "FAIL",
                "rounds": str(len(mode_rows)),
                "contract_pass_rounds": str(len(pass_rows)),
                "tool_call_latency_median_ms": fmt_float(median(latencies)),
                "tool_call_latency_avg_ms": fmt_float(mean(latencies)),
                "baseline_latency_median_ms": fmt_float(baseline_median),
                "comparable_rounds": str(comparable_rounds),
                "improved_rounds": str(improved_rounds),
                "avg_delta_vs_baseline_pct": fmt_float(avg_delta),
                "median_delta_vs_baseline_pct": fmt_float(median_delta),
                "trigger_count_total": str(sum(as_int(row["tool_call_booster_triggers"]) for row in mode_rows)),
                "rollback_count_total": str(sum(as_int(row["total_rollbacks"]) for row in mode_rows)),
                "action_error_count_total": str(sum(as_int(row["action_error_count"]) for row in mode_rows)),
                "latency_trend_verdict": trend_verdict,
                "benefit_verdict": benefit_verdict,
                "verdict_reason": reason,
            }
        )
    return summary_rows


def verdict_reason(
    mode: str,
    mode_contract_pass: bool,
    guarded_mode: bool,
    comparable_rounds: int,
    required_improved_rounds: int,
    improved_rounds: int,
    avg_delta: float | None,
    min_benefit_pct: float,
) -> str:
    if not mode_contract_pass:
        return "mode contract failed"
    if comparable_rounds == 0:
        return "no comparable baseline rounds"
    if improved_rounds < required_improved_rounds:
        return (
            f"only {improved_rounds}/{comparable_rounds} comparable rounds improved by "
            f">={min_benefit_pct:.1f}%"
        )
    if avg_delta is None or avg_delta > -min_benefit_pct:
        return f"average delta did not improve by >={min_benefit_pct:.1f}%"
    if not guarded_mode:
        return "control mode only; latency trend is not guarded host-level benefit proof"
    return (
        "scheduler-side guarded mode met repeated latency improvement gate; "
        "executor warmup is plan/audit-only"
    )


def write_csv(path: Path, fieldnames: list[str], rows: list[dict[str, str]]) -> None:
    with path.open("w", newline="") as handle:
        writer = csv.DictWriter(handle, fieldnames=fieldnames, lineterminator="\n")
        writer.writeheader()
        writer.writerows(rows)


def markdown_table(fieldnames: list[str], rows: list[dict[str, str]]) -> list[str]:
    lines = [
        "| " + " | ".join(fieldnames) + " |",
        "| " + " | ".join("---" for _ in fieldnames) + " |",
    ]
    for row in rows:
        lines.append("| " + " | ".join(row.get(field, "") for field in fieldnames) + " |")
    return lines


def write_report(
    path: Path,
    run_id: str,
    artifact_dir: Path,
    modes: list[str],
    rounds: int,
    min_benefit_pct: float,
    detail_rows: list[dict[str, str]],
    summary_rows: list[dict[str, str]],
) -> tuple[str, str]:
    overall_contract = "PASS" if all(row["contract"] == "PASS" for row in detail_rows) else "FAIL"
    overall_benefit = (
        "PASS" if any(row["benefit_verdict"] == "PASS" for row in summary_rows) else "FAIL"
    )
    lines = [
        "# Tool Call Booster Repeated A/B Benefit Report",
        "",
        f"- Run ID: `{run_id}`",
        f"- Artifact dir: `{artifact_dir}`",
        f"- Rounds: `{rounds}`",
        f"- Modes: `{','.join(modes)}`",
        f"- Benefit threshold: `{min_benefit_pct:.1f}%` latency improvement in at least two thirds of comparable rounds",
        f"- Overall contract verdict: `{overall_contract}`",
        f"- Overall benefit verdict: `{overall_benefit}`",
        "- Benefit scope: guarded scheduler actions only (`nice` and, when enabled, `affinity`).",
        f"- Executor warmup boundary: {WARMUP_EXECUTOR_BOUNDARY}",
        "",
        "## Aggregate",
        "",
    ]
    lines.extend(markdown_table(SUMMARY_COLUMNS, summary_rows))
    lines.extend(
        [
            "",
            "## Detail",
            "",
        ]
    )
    compact_detail_columns = [
        "round",
        "mode",
        "contract",
        "tool_call_latency_ms",
        "tool_call_booster_triggers",
        "total_rollbacks",
        "stages",
        "contract_reason",
    ]
    lines.extend(markdown_table(compact_detail_columns, detail_rows))
    lines.extend(
        [
            "",
            "## Interpretation",
            "",
            "- `baseline` is the unobserved executor sample and anchors latency deltas.",
            "- `noop` and `dry_run` prove recognition, trigger, audit, and rollback closure, but they are controls rather than host-level guarded benefit proof.",
            "- A guarded benefit PASS requires a guarded scheduler mode, clean mode contracts, and repeated latency improvement versus same-round baseline.",
            "- Do not read a Tool Call Booster benefit PASS as proof that executor warmup is live unless a future backend records a real warmup side effect.",
        ]
    )
    path.write_text("\n".join(lines) + "\n")
    return overall_contract, overall_benefit


def parser() -> argparse.ArgumentParser:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--artifact-dir", type=Path, required=True)
    parser.add_argument("--run-id", required=True)
    parser.add_argument("--modes", required=True)
    parser.add_argument("--rounds", type=int, required=True)
    parser.add_argument("--min-benefit-pct", type=float, default=5.0)
    parser.add_argument("--detail-csv", type=Path, required=True)
    parser.add_argument("--summary-csv", type=Path, required=True)
    parser.add_argument("--report-md", type=Path, required=True)
    parser.add_argument("--require-benefit", action="store_true")
    return parser


def main() -> int:
    args = parser().parse_args()
    modes = parse_modes(args.modes)
    if args.rounds <= 0:
        raise SystemExit("rounds must be positive")
    if args.min_benefit_pct < 0:
        raise SystemExit("min benefit pct must be non-negative")

    detail_rows = build_detail_rows(args.artifact_dir, modes, args.rounds)
    summary_rows = build_summary_rows(detail_rows, modes, args.rounds, args.min_benefit_pct)

    write_csv(args.detail_csv, DETAIL_COLUMNS, detail_rows)
    write_csv(args.summary_csv, SUMMARY_COLUMNS, summary_rows)
    overall_contract, overall_benefit = write_report(
        args.report_md,
        args.run_id,
        args.artifact_dir,
        modes,
        args.rounds,
        args.min_benefit_pct,
        detail_rows,
        summary_rows,
    )

    print(f"overall_contract_verdict={overall_contract}")
    print(f"overall_benefit_verdict={overall_benefit}")
    if overall_contract != "PASS":
        return 1
    if args.require_benefit and overall_benefit != "PASS":
        return 1
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
