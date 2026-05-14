#!/usr/bin/env python3
"""Offline evidence gate for deferred observability dashboard planning.

The gate parses recorded runtime audit output, verification artifacts, and
stable telemetry exports into a read-only dashboard snapshot. It writes export
artifacts and rejects profile editing, helper control, scheduler actions,
remote config, unredacted sensitive fields, and dashboard-sourced benefit
truth.
"""

from __future__ import annotations

import argparse
import csv
import json
import os
import pathlib
import sys
from dataclasses import asdict, dataclass
from datetime import datetime, timezone
from typing import Iterable


REPO_ROOT = pathlib.Path(__file__).resolve().parents[2]
DEFAULT_ARTIFACTS_PATH = (
    REPO_ROOT / "bench" / "observability_dashboard_gate" / "default_artifacts.json"
)


class GateViolation(ValueError):
    """Raised when dashboard planning crosses a read-only boundary."""


@dataclass(frozen=True)
class GateConfig:
    read_only: bool = True
    auth_scope: str = "local_operator"
    max_parse_ms: float = 25.0
    max_render_ms: float = 50.0
    max_daemon_loop_regression_pct: float = 1.0


@dataclass(frozen=True)
class RuntimeAuditRecord:
    schema_version: str
    artifact_path: str
    source: str
    mode: str
    backend: str
    processed_events: int
    applied_actions: int
    rollback_count: int
    action_error_count: int
    redaction: str


@dataclass(frozen=True)
class VerificationArtifact:
    schema_version: str
    artifact_path: str
    scenario: str
    contract_verdict: str
    benefit_verdict: str
    benefit_source: str
    summary: str


@dataclass(frozen=True)
class TelemetryMetric:
    schema_version: str
    name: str
    unit: str
    value: float
    attributes: dict[str, str]


@dataclass(frozen=True)
class DashboardInputs:
    runtime_audit: tuple[RuntimeAuditRecord, ...]
    verification_artifacts: tuple[VerificationArtifact, ...]
    telemetry_exports: tuple[TelemetryMetric, ...]
    benchmark: dict[str, float]


@dataclass(frozen=True)
class DashboardSnapshot:
    mode: str
    auth_scope: str
    source_counts: dict[str, int]
    metric_names: tuple[str, ...]
    artifact_verdicts: tuple[dict[str, str], ...]
    audit_totals: dict[str, int]
    redaction_status: str
    benefit_truth_source: str
    control_path_count: int


@dataclass(frozen=True)
class GateResult:
    config: GateConfig
    snapshot: DashboardSnapshot
    export_rows: tuple[dict[str, str | int | float], ...]
    metrics: dict[str, float | int | str]
    deterministic_pass: bool
    verdict: str


DISALLOWED_KEYS = {
    "policy_edit",
    "profile_write",
    "helper_control",
    "scheduler_action",
    "actuator_command",
    "remote_config",
    "benefit_verdict_override",
    "raw_cmdline",
    "environment",
    "secret",
}


def default_dashboard_inputs() -> DashboardInputs:
    return load_inputs(DEFAULT_ARTIFACTS_PATH)


def validate_config(config: GateConfig) -> None:
    violations: list[str] = []
    if not config.read_only:
        violations.append("dashboard gate must remain read-only")
    if config.auth_scope != "local_operator":
        violations.append("dashboard gate only records local_operator auth scope")
    if config.max_parse_ms < 0.0:
        violations.append("max_parse_ms must be non-negative")
    if config.max_render_ms < 0.0:
        violations.append("max_render_ms must be non-negative")
    if config.max_daemon_loop_regression_pct < 0.0:
        violations.append("max_daemon_loop_regression_pct must be non-negative")
    if violations:
        raise GateViolation("; ".join(violations))


def run_dashboard_gate(
    inputs: DashboardInputs,
    config: GateConfig | None = None,
) -> GateResult:
    config = config or GateConfig()
    validate_config(config)
    validate_read_only_inputs(inputs)
    snapshot = build_snapshot(inputs, config)
    export_rows = build_export_rows(inputs)
    metrics = summarize_metrics(inputs, snapshot, config)
    verdict = "PASS" if gate_passes(metrics) else "FAIL"
    return GateResult(
        config=config,
        snapshot=snapshot,
        export_rows=tuple(export_rows),
        metrics=metrics,
        deterministic_pass=True,
        verdict=verdict,
    )


def validate_read_only_inputs(inputs: DashboardInputs) -> None:
    raw = {
        "runtime_audit": [asdict(item) for item in inputs.runtime_audit],
        "verification_artifacts": [asdict(item) for item in inputs.verification_artifacts],
        "telemetry_exports": [asdict(item) for item in inputs.telemetry_exports],
        "benchmark": dict(inputs.benchmark),
    }
    disallowed = sorted(find_disallowed_keys(raw))
    if disallowed:
        raise GateViolation(f"dashboard input contains control or sensitive fields: {', '.join(disallowed)}")

    for record in inputs.runtime_audit:
        if record.schema_version != "runtime_audit.v1":
            raise GateViolation(f"unsupported runtime audit schema: {record.schema_version}")
        if record.redaction != "sanitized":
            raise GateViolation(f"runtime audit artifact is not sanitized: {record.artifact_path}")
        if record.action_error_count != 0:
            raise GateViolation(f"runtime audit artifact has action errors: {record.artifact_path}")

    for artifact in inputs.verification_artifacts:
        if artifact.schema_version != "verification_artifact.v1":
            raise GateViolation(f"unsupported verification artifact schema: {artifact.schema_version}")
        if artifact.benefit_source != "artifact":
            raise GateViolation(f"benefit verdict must be artifact-derived: {artifact.artifact_path}")

    for metric in inputs.telemetry_exports:
        if metric.schema_version != "otel_metric_export.v1":
            raise GateViolation(f"unsupported telemetry export schema: {metric.schema_version}")
        if not metric.name.startswith("aegisai."):
            raise GateViolation(f"dashboard metric is outside aegisai namespace: {metric.name}")


def find_disallowed_keys(value: object) -> set[str]:
    found: set[str] = set()
    if isinstance(value, dict):
        for key, child in value.items():
            if key in DISALLOWED_KEYS:
                found.add(key)
            found.update(find_disallowed_keys(child))
    elif isinstance(value, list):
        for child in value:
            found.update(find_disallowed_keys(child))
    return found


def build_snapshot(inputs: DashboardInputs, config: GateConfig) -> DashboardSnapshot:
    artifact_verdicts = tuple(
        {
            "scenario": artifact.scenario,
            "artifact_path": artifact.artifact_path,
            "contract_verdict": artifact.contract_verdict,
            "benefit_verdict": artifact.benefit_verdict,
            "benefit_source": artifact.benefit_source,
        }
        for artifact in inputs.verification_artifacts
    )
    audit_totals = {
        "processed_events": sum(record.processed_events for record in inputs.runtime_audit),
        "applied_actions": sum(record.applied_actions for record in inputs.runtime_audit),
        "rollback_count": sum(record.rollback_count for record in inputs.runtime_audit),
        "action_error_count": sum(record.action_error_count for record in inputs.runtime_audit),
    }
    return DashboardSnapshot(
        mode="read_only",
        auth_scope=config.auth_scope,
        source_counts={
            "runtime_audit": len(inputs.runtime_audit),
            "verification_artifacts": len(inputs.verification_artifacts),
            "telemetry_exports": len(inputs.telemetry_exports),
        },
        metric_names=tuple(sorted(metric.name for metric in inputs.telemetry_exports)),
        artifact_verdicts=artifact_verdicts,
        audit_totals=audit_totals,
        redaction_status="PASS",
        benefit_truth_source="artifact",
        control_path_count=0,
    )


def build_export_rows(inputs: DashboardInputs) -> list[dict[str, str | int | float]]:
    rows: list[dict[str, str | int | float]] = []
    for record in inputs.runtime_audit:
        rows.append(
            {
                "row_type": "runtime_audit",
                "schema_version": record.schema_version,
                "name": record.source,
                "artifact_path": record.artifact_path,
                "value": record.processed_events,
                "unit": "{event}",
                "verdict": "PASS" if record.action_error_count == 0 else "FAIL",
            }
        )
    for artifact in inputs.verification_artifacts:
        rows.append(
            {
                "row_type": "verification_artifact",
                "schema_version": artifact.schema_version,
                "name": artifact.scenario,
                "artifact_path": artifact.artifact_path,
                "value": artifact.benefit_verdict,
                "unit": "verdict",
                "verdict": artifact.contract_verdict,
            }
        )
    for metric in inputs.telemetry_exports:
        rows.append(
            {
                "row_type": "telemetry_export",
                "schema_version": metric.schema_version,
                "name": metric.name,
                "artifact_path": "recorded_metric_export",
                "value": metric.value,
                "unit": metric.unit,
                "verdict": "PASS",
            }
        )
    return rows


def summarize_metrics(
    inputs: DashboardInputs,
    snapshot: DashboardSnapshot,
    config: GateConfig,
) -> dict[str, float | int | str]:
    baseline = inputs.benchmark.get("baseline_daemon_loop_latency_ms", 0.0)
    post = inputs.benchmark.get("post_dashboard_daemon_loop_latency_ms", baseline)
    regression_pct = pct_delta(baseline, post)
    parse_ms = inputs.benchmark.get("dashboard_parse_ms", 0.0)
    render_ms = inputs.benchmark.get("dashboard_render_ms", 0.0)
    return {
        "runtime_audit_records": snapshot.source_counts["runtime_audit"],
        "verification_artifacts": snapshot.source_counts["verification_artifacts"],
        "telemetry_exports": snapshot.source_counts["telemetry_exports"],
        "control_path_count": snapshot.control_path_count,
        "parse_ms": round(parse_ms, 6),
        "parse_limit_ms": config.max_parse_ms,
        "render_ms": round(render_ms, 6),
        "render_limit_ms": config.max_render_ms,
        "daemon_loop_regression_pct": round(regression_pct, 6),
        "daemon_loop_regression_limit_pct": config.max_daemon_loop_regression_pct,
        "redaction": snapshot.redaction_status,
        "benefit_truth_source": snapshot.benefit_truth_source,
        "overhead_budget": (
            "PASS"
            if parse_ms <= config.max_parse_ms
            and render_ms <= config.max_render_ms
            and regression_pct <= config.max_daemon_loop_regression_pct
            else "FAIL"
        ),
    }


def pct_delta(before: float, after: float) -> float:
    if before == 0.0:
        return 0.0 if after == 0.0 else 100.0
    return ((after - before) / before) * 100.0


def gate_passes(metrics: dict[str, float | int | str]) -> bool:
    return all(
        [
            metrics["runtime_audit_records"] > 0,
            metrics["verification_artifacts"] > 0,
            metrics["telemetry_exports"] > 0,
            metrics["control_path_count"] == 0,
            metrics["redaction"] == "PASS",
            metrics["benefit_truth_source"] == "artifact",
            metrics["overhead_budget"] == "PASS",
        ]
    )


def load_inputs(path: pathlib.Path | None) -> DashboardInputs:
    if path is None:
        return default_dashboard_inputs()
    raw = json.loads(path.read_text(encoding="utf-8"))
    if not isinstance(raw, dict):
        raise GateViolation("dashboard artifact JSON must be an object")
    disallowed = sorted(find_disallowed_keys(raw))
    if disallowed:
        raise GateViolation(f"dashboard input contains control or sensitive fields: {', '.join(disallowed)}")
    return DashboardInputs(
        runtime_audit=tuple(RuntimeAuditRecord(**item) for item in raw.get("runtime_audit", [])),
        verification_artifacts=tuple(
            VerificationArtifact(**item) for item in raw.get("verification_artifacts", [])
        ),
        telemetry_exports=tuple(
            TelemetryMetric(**item) for item in raw.get("telemetry_exports", [])
        ),
        benchmark={key: float(value) for key, value in raw.get("benchmark", {}).items()},
    )


def result_payload(result: GateResult) -> dict[str, object]:
    return {
        "config": asdict(result.config),
        "snapshot": asdict(result.snapshot),
        "export_rows": list(result.export_rows),
        "metrics": result.metrics,
        "deterministic_pass": result.deterministic_pass,
        "verdict": result.verdict,
    }


def stable_result_payload(result: GateResult) -> dict[str, object]:
    payload = result_payload(result)
    payload["deterministic_pass"] = True
    payload["verdict"] = "PASS"
    return payload


def write_outputs(result: GateResult, artifact_dir: pathlib.Path) -> dict[str, pathlib.Path]:
    artifact_dir.mkdir(parents=True, exist_ok=True)
    snapshot_path = artifact_dir / "observability_dashboard_snapshot.json"
    export_path = artifact_dir / "observability_dashboard_export.csv"
    report_path = artifact_dir / "observability_dashboard_gate_report.md"

    snapshot_path.write_text(
        json.dumps(result_payload(result), indent=2, sort_keys=True) + "\n",
        encoding="utf-8",
    )
    write_export_csv(result, export_path)
    report_path.write_text(
        render_report(result, snapshot_path, export_path, report_path),
        encoding="utf-8",
    )
    return {
        "snapshot": snapshot_path,
        "export": export_path,
        "report": report_path,
    }


def write_export_csv(result: GateResult, path: pathlib.Path) -> None:
    fieldnames = [
        "row_type",
        "schema_version",
        "name",
        "artifact_path",
        "value",
        "unit",
        "verdict",
    ]
    with path.open("w", newline="", encoding="utf-8") as handle:
        writer = csv.DictWriter(handle, fieldnames=fieldnames)
        writer.writeheader()
        for row in result.export_rows:
            writer.writerow({field: row.get(field, "") for field in fieldnames})


def render_report(
    result: GateResult,
    snapshot_path: pathlib.Path,
    export_path: pathlib.Path,
    report_path: pathlib.Path,
) -> str:
    metrics = result.metrics
    return "\n".join(
        [
            "# Observability Dashboard Evidence Gate Report",
            "",
            f"- Verdict: `{result.verdict}`",
            "- Runtime behavior: `not_connected`",
            "- Mode: `read_only`",
            "- Auth scope: `local_operator`",
            f"- Runtime audit records: `{metrics['runtime_audit_records']}`",
            f"- Verification artifacts: `{metrics['verification_artifacts']}`",
            f"- Telemetry exports: `{metrics['telemetry_exports']}`",
            f"- Control paths: `{metrics['control_path_count']}`",
            f"- Redaction: `{metrics['redaction']}`",
            f"- Benefit truth source: `{metrics['benefit_truth_source']}`",
            f"- Parse ms: `{metrics['parse_ms']}` / `{metrics['parse_limit_ms']}`",
            f"- Render ms: `{metrics['render_ms']}` / `{metrics['render_limit_ms']}`",
            f"- Daemon loop regression pct: `{metrics['daemon_loop_regression_pct']}` / `{metrics['daemon_loop_regression_limit_pct']}`",
            "",
            "## Artifacts",
            "",
            f"- Snapshot JSON: `{snapshot_path}`",
            f"- Export CSV: `{export_path}`",
            f"- Report: `{report_path}`",
            "",
            "## Promotion Boundary",
            "",
            "- This report is planning evidence only and does not create a dashboard service.",
            "- Benefit verdicts remain artifact-derived; UI state is not a source of truth.",
            "- A future dashboard still requires bind/auth, no-new-privilege, redaction, failure-isolation, and overhead evidence.",
            "",
        ]
    )


def parse_args(argv: list[str]) -> argparse.Namespace:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--dataset-json", type=pathlib.Path)
    parser.add_argument("--artifact-dir", type=pathlib.Path)
    parser.add_argument("--run-id", default=os.environ.get("AEGISAI_OBSERVABILITY_DASHBOARD_GATE_RUN_ID"))
    return parser.parse_args(argv)


def main(argv: list[str]) -> int:
    args = parse_args(argv)
    run_id = args.run_id or datetime.now(timezone.utc).strftime(
        "observability_dashboard_gate_%Y%m%dT%H%M%SZ"
    )
    artifact_dir = args.artifact_dir or pathlib.Path(
        os.environ.get(
            "AEGISAI_OBSERVABILITY_DASHBOARD_GATE_ARTIFACT_DIR",
            str(REPO_ROOT / ".cache" / "aegisai" / "observability_dashboard_gate" / run_id),
        )
    )

    try:
        inputs = load_inputs(args.dataset_json)
        first = run_dashboard_gate(inputs)
        second = run_dashboard_gate(inputs)
        deterministic_pass = stable_result_payload(first) == stable_result_payload(second)
        result = run_dashboard_gate(inputs)
        result = GateResult(
            config=result.config,
            snapshot=result.snapshot,
            export_rows=result.export_rows,
            metrics=result.metrics,
            deterministic_pass=deterministic_pass,
            verdict="PASS" if deterministic_pass and result.verdict == "PASS" else "FAIL",
        )
        paths = write_outputs(result, artifact_dir)
    except GateViolation as error:
        print(f"observability_dashboard_gate=FAIL reason={error}", file=sys.stderr)
        return 1

    print(
        f"observability_dashboard_gate={result.verdict} "
        f"artifact_dir={artifact_dir} "
        f"report={paths['report']}"
    )
    return 0 if result.verdict == "PASS" else 1


if __name__ == "__main__":
    raise SystemExit(main(sys.argv[1:]))
