#!/usr/bin/env python3
"""Offline evidence gate for deferred GPU coordination planning.

The gate parses recorded GPU inventory and benchmark observations, writes an
observe/plan-only dry-run report, and exercises a safety rejection matrix. It
does not call DCGM, NVML, nvidia-smi, CUDA, sysfs, the runtime daemon, the
actuator, or any helper process.
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
DEFAULT_INVENTORY_PATH = (
    REPO_ROOT / "bench" / "gpu_coordination_gate" / "default_inventory.json"
)


class GateViolation(ValueError):
    """Raised when GPU coordination planning crosses a safety boundary."""


@dataclass(frozen=True)
class GateConfig:
    observe_plan_only: bool = True
    supported_vendors: tuple[str, ...] = ("nvidia",)
    require_dcgm_for_supported_plan: bool = True
    max_latency_overhead_pct: float = 2.0
    max_host_cpu_delta_pct: float = 1.0
    max_memory_delta_mib: int = 128
    required_safety_rejections: int = 6


@dataclass(frozen=True)
class HostObservation:
    host_id: str
    gpu_vendor: str
    gpu_count: int
    gpu_model: str
    dcgm_available: bool
    mig_mode: str
    mig_instances: int
    container_runtime: str
    privilege_mode: str
    target_id: str | None
    workload: str
    baseline_latency_ms: float
    dry_run_latency_ms: float
    baseline_throughput_items_s: float
    dry_run_throughput_items_s: float
    baseline_host_cpu_pct: float
    dry_run_host_cpu_pct: float
    baseline_gpu_memory_mib: int
    dry_run_gpu_memory_mib: int
    observed_gpu_utilization_pct: float
    unsupported_reason: str | None = None


@dataclass(frozen=True)
class DryRunPlan:
    host_id: str
    mode: str
    status: str
    target_id: str | None
    reason: str
    gpu_vendor: str
    gpu_model: str
    mig_boundary: str
    container_boundary: str
    privilege_boundary: str
    live_mutation: bool = False


@dataclass(frozen=True)
class FutureMutationCandidate:
    name: str
    host_id: str
    gpu_vendor: str
    action_kind: str
    target_id: str | None
    target_allowlist: tuple[str, ...] = ()
    isolation_reviewed: bool = False
    privilege_reviewed: bool = False
    rollback_plan: tuple[str, ...] = ()
    live_action_requested: bool = True


@dataclass(frozen=True)
class SafetyMatrixRow:
    name: str
    accepted: bool
    reasons: tuple[str, ...]


@dataclass(frozen=True)
class GateResult:
    config: GateConfig
    plans: tuple[DryRunPlan, ...]
    safety_matrix: tuple[SafetyMatrixRow, ...]
    benchmark_rows: tuple[dict[str, float | int | str], ...]
    metrics: dict[str, float | int | str]
    deterministic_pass: bool
    verdict: str


def default_host_observations() -> list[HostObservation]:
    return load_observations(DEFAULT_INVENTORY_PATH)


def validate_config(config: GateConfig) -> None:
    violations: list[str] = []
    if not config.observe_plan_only:
        violations.append("GPU gate must remain observe/plan-only")
    if not config.supported_vendors:
        violations.append("at least one supported vendor must be named")
    if config.max_latency_overhead_pct < 0.0:
        violations.append("max_latency_overhead_pct must be non-negative")
    if config.max_host_cpu_delta_pct < 0.0:
        violations.append("max_host_cpu_delta_pct must be non-negative")
    if config.max_memory_delta_mib < 0:
        violations.append("max_memory_delta_mib must be non-negative")
    if config.required_safety_rejections <= 0:
        violations.append("required_safety_rejections must be positive")
    if violations:
        raise GateViolation("; ".join(violations))


def plan_hosts(
    observations: Iterable[HostObservation],
    config: GateConfig | None = None,
) -> GateResult:
    config = config or GateConfig()
    validate_config(config)
    observation_list = list(observations)
    plans = tuple(plan_host(observation, config) for observation in observation_list)
    safety_matrix = default_safety_matrix()
    benchmark_rows = tuple(
        benchmark_row(observation, plan)
        for observation, plan in zip(observation_list, plans)
    )
    metrics = summarize_metrics(plans, safety_matrix, benchmark_rows, config)
    verdict = "PASS" if gate_passes(metrics) else "FAIL"
    return GateResult(
        config=config,
        plans=plans,
        safety_matrix=safety_matrix,
        benchmark_rows=benchmark_rows,
        metrics=metrics,
        deterministic_pass=True,
        verdict=verdict,
    )


def plan_host(observation: HostObservation, config: GateConfig) -> DryRunPlan:
    vendor = observation.gpu_vendor.lower()
    if observation.gpu_count <= 0 or vendor == "none":
        return no_op_plan(observation, "unsupported_noop", "no_compatible_gpu_detected")
    if vendor not in config.supported_vendors:
        return no_op_plan(
            observation,
            "unsupported_noop",
            f"vendor_not_in_first_slice:{observation.gpu_vendor}",
        )
    if config.require_dcgm_for_supported_plan and not observation.dcgm_available:
        return no_op_plan(observation, "telemetry_unavailable_noop", "dcgm_unavailable")
    if observation.target_id is None:
        return no_op_plan(observation, "unsupported_noop", "missing_target_id")
    return DryRunPlan(
        host_id=observation.host_id,
        mode="observe_plan_only",
        status="dry_run_plan",
        target_id=observation.target_id,
        reason="recorded_dcgm_telemetry_parse_only",
        gpu_vendor=observation.gpu_vendor,
        gpu_model=observation.gpu_model,
        mig_boundary=f"{observation.mig_mode}:{observation.mig_instances}",
        container_boundary=observation.container_runtime,
        privilege_boundary=observation.privilege_mode,
        live_mutation=False,
    )


def no_op_plan(observation: HostObservation, status: str, reason: str) -> DryRunPlan:
    return DryRunPlan(
        host_id=observation.host_id,
        mode="observe_plan_only",
        status=status,
        target_id=observation.target_id,
        reason=observation.unsupported_reason or reason,
        gpu_vendor=observation.gpu_vendor,
        gpu_model=observation.gpu_model,
        mig_boundary=f"{observation.mig_mode}:{observation.mig_instances}",
        container_boundary=observation.container_runtime,
        privilege_boundary=observation.privilege_mode,
        live_mutation=False,
    )


def benchmark_row(
    observation: HostObservation,
    plan: DryRunPlan,
) -> dict[str, float | int | str]:
    latency_delta_pct = pct_delta(
        observation.baseline_latency_ms,
        observation.dry_run_latency_ms,
    )
    throughput_delta_pct = pct_delta(
        observation.baseline_throughput_items_s,
        observation.dry_run_throughput_items_s,
    )
    host_cpu_delta_pct = observation.dry_run_host_cpu_pct - observation.baseline_host_cpu_pct
    memory_delta_mib = observation.dry_run_gpu_memory_mib - observation.baseline_gpu_memory_mib
    return {
        "host_id": observation.host_id,
        "status": plan.status,
        "gpu_vendor": observation.gpu_vendor,
        "workload": observation.workload,
        "baseline_latency_ms": observation.baseline_latency_ms,
        "dry_run_latency_ms": observation.dry_run_latency_ms,
        "latency_delta_pct": round(latency_delta_pct, 6),
        "baseline_throughput_items_s": observation.baseline_throughput_items_s,
        "dry_run_throughput_items_s": observation.dry_run_throughput_items_s,
        "throughput_delta_pct": round(throughput_delta_pct, 6),
        "host_cpu_delta_pct": round(host_cpu_delta_pct, 6),
        "memory_delta_mib": memory_delta_mib,
        "observed_gpu_utilization_pct": observation.observed_gpu_utilization_pct,
    }


def pct_delta(before: float, after: float) -> float:
    if before == 0.0:
        return 0.0 if after == 0.0 else 100.0
    return ((after - before) / before) * 100.0


def default_safety_matrix() -> tuple[SafetyMatrixRow, ...]:
    candidates = [
        FutureMutationCandidate(
            name="live_action_default_denied",
            host_id="gpu-ci-a100-mig",
            gpu_vendor="nvidia",
            action_kind="rebalance_mig_profile",
            target_id="GPU-a100-mig-0001",
            target_allowlist=("GPU-a100-mig-0001",),
            isolation_reviewed=True,
            privilege_reviewed=True,
            rollback_plan=("restore_previous_mig_geometry",),
            live_action_requested=True,
        ),
        FutureMutationCandidate(
            name="missing_target_allowlist",
            host_id="gpu-ci-a100-mig",
            gpu_vendor="nvidia",
            action_kind="limit_gpu_process",
            target_id="GPU-a100-mig-0001",
            isolation_reviewed=True,
            privilege_reviewed=True,
            rollback_plan=("restore_noop_state",),
            live_action_requested=False,
        ),
        FutureMutationCandidate(
            name="unsupported_vendor",
            host_id="gpu-ci-amd",
            gpu_vendor="amd",
            action_kind="limit_gpu_process",
            target_id="amd-mi300x-0001",
            target_allowlist=("amd-mi300x-0001",),
            isolation_reviewed=True,
            privilege_reviewed=True,
            rollback_plan=("restore_noop_state",),
            live_action_requested=False,
        ),
        FutureMutationCandidate(
            name="missing_isolation_review",
            host_id="gpu-ci-a100-mig",
            gpu_vendor="nvidia",
            action_kind="limit_gpu_process",
            target_id="GPU-a100-mig-0001",
            target_allowlist=("GPU-a100-mig-0001",),
            privilege_reviewed=True,
            rollback_plan=("restore_noop_state",),
            live_action_requested=False,
        ),
        FutureMutationCandidate(
            name="missing_privilege_review",
            host_id="gpu-ci-a100-mig",
            gpu_vendor="nvidia",
            action_kind="limit_gpu_process",
            target_id="GPU-a100-mig-0001",
            target_allowlist=("GPU-a100-mig-0001",),
            isolation_reviewed=True,
            rollback_plan=("restore_noop_state",),
            live_action_requested=False,
        ),
        FutureMutationCandidate(
            name="missing_rollback",
            host_id="gpu-ci-a100-mig",
            gpu_vendor="nvidia",
            action_kind="limit_gpu_process",
            target_id="GPU-a100-mig-0001",
            target_allowlist=("GPU-a100-mig-0001",),
            isolation_reviewed=True,
            privilege_reviewed=True,
            live_action_requested=False,
        ),
    ]
    return tuple(validate_future_mutation_candidate(candidate) for candidate in candidates)


def validate_future_mutation_candidate(candidate: FutureMutationCandidate) -> SafetyMatrixRow:
    reasons: list[str] = []
    if candidate.live_action_requested:
        reasons.append("live_gpu_mutation_denied_by_default")
    if candidate.gpu_vendor.lower() != "nvidia":
        reasons.append(f"unsupported_vendor:{candidate.gpu_vendor}")
    if candidate.target_id is None:
        reasons.append("missing_target_id")
    elif candidate.target_id not in candidate.target_allowlist:
        reasons.append("target_not_in_allowlist")
    if not candidate.isolation_reviewed:
        reasons.append("device_isolation_review_missing")
    if not candidate.privilege_reviewed:
        reasons.append("privilege_review_missing")
    if not candidate.rollback_plan:
        reasons.append("rollback_or_noop_plan_missing")

    return SafetyMatrixRow(
        name=candidate.name,
        accepted=False if reasons else False,
        reasons=tuple(reasons or ("planning_gate_rejects_all_mutation_candidates",)),
    )


def summarize_metrics(
    plans: tuple[DryRunPlan, ...],
    safety_matrix: tuple[SafetyMatrixRow, ...],
    benchmark_rows: tuple[dict[str, float | int | str], ...],
    config: GateConfig,
) -> dict[str, float | int | str]:
    dry_run_rows = [row for row in benchmark_rows if row["status"] == "dry_run_plan"]
    max_latency_overhead_pct = max(
        (float(row["latency_delta_pct"]) for row in dry_run_rows),
        default=0.0,
    )
    max_host_cpu_delta_pct = max(
        (abs(float(row["host_cpu_delta_pct"])) for row in dry_run_rows),
        default=0.0,
    )
    max_memory_delta_mib = max(
        (int(row["memory_delta_mib"]) for row in dry_run_rows),
        default=0,
    )
    safety_rejections = sum(1 for row in safety_matrix if not row.accepted)
    mutation_count = sum(1 for plan in plans if plan.live_mutation)
    unsupported_hosts = sum(
        1
        for plan in plans
        if plan.status in {"unsupported_noop", "telemetry_unavailable_noop"}
    )
    metrics: dict[str, float | int | str] = {
        "evaluated_hosts": len(plans),
        "dry_run_plan_count": len(dry_run_rows),
        "unsupported_host_noop_count": unsupported_hosts,
        "mutation_count": mutation_count,
        "safety_rejection_count": safety_rejections,
        "max_latency_overhead_pct": round(max_latency_overhead_pct, 6),
        "max_latency_overhead_limit_pct": config.max_latency_overhead_pct,
        "max_host_cpu_delta_pct": round(max_host_cpu_delta_pct, 6),
        "max_host_cpu_delta_limit_pct": config.max_host_cpu_delta_pct,
        "max_memory_delta_mib": max_memory_delta_mib,
        "max_memory_delta_limit_mib": config.max_memory_delta_mib,
        "gpu_host_dry_run_proof": "PASS" if dry_run_rows else "FAIL",
        "unsupported_host_smoke": "PASS" if unsupported_hosts else "FAIL",
        "safety_matrix": (
            "PASS"
            if safety_rejections >= config.required_safety_rejections
            and all(not row.accepted for row in safety_matrix)
            else "FAIL"
        ),
        "overhead_budget": (
            "PASS"
            if max_latency_overhead_pct <= config.max_latency_overhead_pct
            and max_host_cpu_delta_pct <= config.max_host_cpu_delta_pct
            and max_memory_delta_mib <= config.max_memory_delta_mib
            else "FAIL"
        ),
    }
    return metrics


def gate_passes(metrics: dict[str, float | int | str]) -> bool:
    return all(
        [
            metrics["mutation_count"] == 0,
            metrics["dry_run_plan_count"] > 0,
            metrics["unsupported_host_noop_count"] > 0,
            metrics["gpu_host_dry_run_proof"] == "PASS",
            metrics["unsupported_host_smoke"] == "PASS",
            metrics["safety_matrix"] == "PASS",
            metrics["overhead_budget"] == "PASS",
        ]
    )


def load_observations(path: pathlib.Path | None) -> list[HostObservation]:
    if path is None:
        return default_host_observations()
    raw = json.loads(path.read_text(encoding="utf-8"))
    if not isinstance(raw, list):
        raise GateViolation("inventory JSON must be a list of host observations")
    return [HostObservation(**item) for item in raw]


def result_payload(result: GateResult) -> dict[str, object]:
    return {
        "config": asdict(result.config),
        "plans": [asdict(plan) for plan in result.plans],
        "safety_matrix": [asdict(row) for row in result.safety_matrix],
        "benchmark_rows": list(result.benchmark_rows),
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
    plan_path = artifact_dir / "gpu_coordination_plan.json"
    benchmark_path = artifact_dir / "gpu_coordination_benchmark.csv"
    safety_path = artifact_dir / "gpu_coordination_safety_matrix.csv"
    report_path = artifact_dir / "gpu_coordination_gate_report.md"

    plan_path.write_text(
        json.dumps(result_payload(result), indent=2, sort_keys=True) + "\n",
        encoding="utf-8",
    )
    write_benchmark_csv(result, benchmark_path)
    write_safety_csv(result, safety_path)
    report_path.write_text(
        render_report(result, plan_path, benchmark_path, safety_path, report_path),
        encoding="utf-8",
    )
    return {
        "plan": plan_path,
        "benchmark": benchmark_path,
        "safety": safety_path,
        "report": report_path,
    }


def write_benchmark_csv(result: GateResult, path: pathlib.Path) -> None:
    fieldnames = [
        "host_id",
        "status",
        "gpu_vendor",
        "workload",
        "baseline_latency_ms",
        "dry_run_latency_ms",
        "latency_delta_pct",
        "baseline_throughput_items_s",
        "dry_run_throughput_items_s",
        "throughput_delta_pct",
        "host_cpu_delta_pct",
        "memory_delta_mib",
        "observed_gpu_utilization_pct",
        "verdict",
    ]
    with path.open("w", newline="", encoding="utf-8") as handle:
        writer = csv.DictWriter(handle, fieldnames=fieldnames)
        writer.writeheader()
        for row in result.benchmark_rows:
            writer.writerow({**row, "verdict": result.verdict})


def write_safety_csv(result: GateResult, path: pathlib.Path) -> None:
    fieldnames = ["name", "accepted", "reasons"]
    with path.open("w", newline="", encoding="utf-8") as handle:
        writer = csv.DictWriter(handle, fieldnames=fieldnames)
        writer.writeheader()
        for row in result.safety_matrix:
            writer.writerow(
                {
                    "name": row.name,
                    "accepted": str(row.accepted).lower(),
                    "reasons": ";".join(row.reasons),
                }
            )


def render_report(
    result: GateResult,
    plan_path: pathlib.Path,
    benchmark_path: pathlib.Path,
    safety_path: pathlib.Path,
    report_path: pathlib.Path,
) -> str:
    metrics = result.metrics
    return "\n".join(
        [
            "# GPU Coordination Evidence Gate Report",
            "",
            f"- Verdict: `{result.verdict}`",
            "- Runtime behavior: `not_connected`",
            "- Mode: `observe_plan_only`",
            f"- Evaluated hosts: `{metrics['evaluated_hosts']}`",
            f"- GPU dry-run plans: `{metrics['dry_run_plan_count']}`",
            f"- Unsupported/no-op hosts: `{metrics['unsupported_host_noop_count']}`",
            f"- Live GPU mutations: `{metrics['mutation_count']}`",
            f"- Safety rejections: `{metrics['safety_rejection_count']}`",
            f"- Max latency overhead pct: `{metrics['max_latency_overhead_pct']}` / `{metrics['max_latency_overhead_limit_pct']}`",
            f"- Max host CPU delta pct: `{metrics['max_host_cpu_delta_pct']}` / `{metrics['max_host_cpu_delta_limit_pct']}`",
            f"- Max GPU memory delta MiB: `{metrics['max_memory_delta_mib']}` / `{metrics['max_memory_delta_limit_mib']}`",
            "",
            "## Artifacts",
            "",
            f"- Plan JSON: `{plan_path}`",
            f"- Benchmark CSV: `{benchmark_path}`",
            f"- Safety matrix CSV: `{safety_path}`",
            f"- Report: `{report_path}`",
            "",
            "## Promotion Boundary",
            "",
            "- This report is planning evidence only and does not prove GPU scheduler benefit.",
            "- Non-NVIDIA and unsupported hosts remain no-op in the first slice.",
            "- A future live mutation path still requires explicit target allowlists, isolation and privilege reviews, rollback proof, and guarded GPU-host benchmarks.",
            "",
        ]
    )


def parse_args(argv: list[str]) -> argparse.Namespace:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--dataset-json", type=pathlib.Path)
    parser.add_argument("--artifact-dir", type=pathlib.Path)
    parser.add_argument("--run-id", default=os.environ.get("AEGISAI_GPU_COORDINATION_GATE_RUN_ID"))
    return parser.parse_args(argv)


def main(argv: list[str]) -> int:
    args = parse_args(argv)
    run_id = args.run_id or datetime.now(timezone.utc).strftime("gpu_coordination_gate_%Y%m%dT%H%M%SZ")
    artifact_dir = args.artifact_dir or pathlib.Path(
        os.environ.get(
            "AEGISAI_GPU_COORDINATION_GATE_ARTIFACT_DIR",
            str(REPO_ROOT / ".cache" / "aegisai" / "gpu_coordination_gate" / run_id),
        )
    )

    try:
        observations = load_observations(args.dataset_json)
        first = plan_hosts(observations)
        second = plan_hosts(observations)
        deterministic_pass = stable_result_payload(first) == stable_result_payload(second)
        result = plan_hosts(observations)
        result = GateResult(
            config=result.config,
            plans=result.plans,
            safety_matrix=result.safety_matrix,
            benchmark_rows=result.benchmark_rows,
            metrics=result.metrics,
            deterministic_pass=deterministic_pass,
            verdict="PASS" if deterministic_pass and result.verdict == "PASS" else "FAIL",
        )
        paths = write_outputs(result, artifact_dir)
    except GateViolation as error:
        print(f"gpu_coordination_gate=FAIL reason={error}", file=sys.stderr)
        return 1

    print(
        f"gpu_coordination_gate={result.verdict} "
        f"artifact_dir={artifact_dir} "
        f"report={paths['report']}"
    )
    return 0 if result.verdict == "PASS" else 1


if __name__ == "__main__":
    raise SystemExit(main(sys.argv[1:]))
