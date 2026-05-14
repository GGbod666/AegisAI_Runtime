#!/usr/bin/env python3
"""Offline evidence gate for deferred online adaptive policy planning.

The gate exercises a deterministic shadow-only adaptive policy slice against a
fixed replay. It writes audit and benchmark artifacts, but it does not connect
to the runtime daemon, actuator, or production profiles.
"""

from __future__ import annotations

import argparse
import csv
import json
import os
import pathlib
import sys
from collections import deque
from dataclasses import asdict, dataclass
from datetime import datetime, timezone
from typing import Callable, Iterable


REPO_ROOT = pathlib.Path(__file__).resolve().parents[2]
DEFAULT_REPLAY_PATH = REPO_ROOT / "bench" / "adaptive_policy_gate" / "default_replay.json"
DEFAULT_STATIC_THRESHOLD_US = 2_000
DEFAULT_ADAPTIVE_THRESHOLD_US = 2_200


class GateViolation(ValueError):
    """Raised when the shadow adaptive gate would violate a safety boundary."""


@dataclass(frozen=True)
class GateConfig:
    shadow_mode: bool = True
    operator_approved: bool = False
    live_mutation_requested: bool = False
    profile_write_requested: bool = False
    freeze_switch: bool = False
    retention_limit: int = 3
    max_priority_delta: int = 5
    max_boost_duration_ms: int = 1_000
    max_affinity_ratio: float = 0.5
    drift_threshold: float = 0.30
    static_threshold_us: int = DEFAULT_STATIC_THRESHOLD_US
    adaptive_threshold_us: int = DEFAULT_ADAPTIVE_THRESHOLD_US
    regression_budget_rate: float = 0.25


@dataclass(frozen=True)
class ReplaySample:
    timestamp_ms: int
    scenario: str
    pid: int
    process_name: str
    run_queue_delay_us: int
    p99_latency_ms: float
    baseline_p99_latency_ms: float
    should_intervene: bool

    @property
    def drift_score(self) -> float:
        if self.baseline_p99_latency_ms <= 0:
            return 1.0
        return abs(self.p99_latency_ms - self.baseline_p99_latency_ms) / self.baseline_p99_latency_ms


@dataclass(frozen=True)
class CandidateAction:
    priority_delta: int = -7
    duration_ms: int = 1_300
    affinity_ratio: float = 0.75
    live_mutation: bool = False
    profile_write: bool = False
    rollback_plan: tuple[str, ...] = ("restore_nice", "restore_affinity")
    operator_approval_id: str | None = None


@dataclass(frozen=True)
class ShadowRecommendation:
    audit_id: str
    mode: str
    priority_delta: int
    duration_ms: int
    affinity_ratio: float
    rollback_plan: tuple[str, ...]
    operator_gate: str
    live_mutation: bool
    profile_write: bool
    rationale: tuple[str, ...]


@dataclass(frozen=True)
class ReplayDecision:
    timestamp_ms: int
    pid: int
    scenario: str
    static_baseline_triggered: bool
    drift_score: float
    retained_samples: int
    freeze_active: bool
    freeze_reason: str | None
    recommendation: ShadowRecommendation | None


@dataclass(frozen=True)
class GateResult:
    config: GateConfig
    decisions: tuple[ReplayDecision, ...]
    static_metrics: dict[str, float | int | str]
    adaptive_metrics: dict[str, float | int | str]
    deterministic_pass: bool
    verdict: str


def default_replay_samples() -> list[ReplaySample]:
    return load_samples(DEFAULT_REPLAY_PATH)


def validate_config(config: GateConfig) -> None:
    violations: list[str] = []
    if not config.shadow_mode:
        violations.append("shadow_mode must remain enabled")
    if config.live_mutation_requested:
        violations.append("live mutation is not allowed in the deferred adaptive gate")
    if config.profile_write_requested:
        violations.append("profile writes are not allowed in the deferred adaptive gate")
    if config.operator_approved:
        violations.append("operator approval must not be consumed by a shadow-only gate")
    if config.retention_limit <= 0:
        violations.append("retention_limit must be positive")
    if config.max_priority_delta < 0:
        violations.append("max_priority_delta must be non-negative")
    if config.max_boost_duration_ms <= 0:
        violations.append("max_boost_duration_ms must be positive")
    if not 0.0 <= config.max_affinity_ratio <= 1.0:
        violations.append("max_affinity_ratio must be between 0.0 and 1.0")
    if config.regression_budget_rate < 0.0:
        violations.append("regression_budget_rate must be non-negative")
    if violations:
        raise GateViolation("; ".join(violations))


def default_candidate_factory(_sample: ReplaySample) -> CandidateAction:
    return CandidateAction()


def normalize_candidate(candidate: CandidateAction, config: GateConfig) -> ShadowRecommendation:
    if candidate.live_mutation:
        raise GateViolation("candidate attempted live mutation")
    if candidate.profile_write:
        raise GateViolation("candidate attempted profile write")
    if candidate.operator_approval_id:
        raise GateViolation("shadow gate must not consume operator approval")
    if not candidate.rollback_plan:
        raise GateViolation("candidate recommendation lacks rollback plan")

    priority_delta = min(0, max(candidate.priority_delta, -config.max_priority_delta))
    duration_ms = max(1, min(candidate.duration_ms, config.max_boost_duration_ms))
    affinity_ratio = min(max(candidate.affinity_ratio, 0.0), config.max_affinity_ratio)

    rationale = [
        f"priority_delta_clamped:{candidate.priority_delta}->{priority_delta}",
        f"duration_clamped:{candidate.duration_ms}->{duration_ms}",
        f"affinity_ratio_clamped:{candidate.affinity_ratio:.3f}->{affinity_ratio:.3f}",
    ]

    return ShadowRecommendation(
        audit_id="",
        mode="shadow",
        priority_delta=priority_delta,
        duration_ms=duration_ms,
        affinity_ratio=round(affinity_ratio, 6),
        rollback_plan=tuple(candidate.rollback_plan),
        operator_gate="required_before_live_mutation",
        live_mutation=False,
        profile_write=False,
        rationale=tuple(rationale),
    )


def run_replay(
    samples: Iterable[ReplaySample],
    config: GateConfig | None = None,
    candidate_factory: Callable[[ReplaySample], CandidateAction] = default_candidate_factory,
    deterministic_pass: bool = True,
) -> GateResult:
    config = config or GateConfig()
    validate_config(config)
    sample_list = list(samples)

    retained: deque[ReplaySample] = deque(maxlen=config.retention_limit)
    decisions: list[ReplayDecision] = []
    freeze_latched = config.freeze_switch

    for sample in sample_list:
        static_triggered = sample.run_queue_delay_us >= config.static_threshold_us
        freeze_reason = None

        if sample.drift_score > config.drift_threshold:
            freeze_latched = True
            freeze_reason = f"drift_score:{sample.drift_score:.3f}>{config.drift_threshold:.3f}"
        elif freeze_latched:
            freeze_reason = "freeze_switch_active"

        recommendation = None
        if (
            not freeze_latched
            and sample.run_queue_delay_us >= config.adaptive_threshold_us
        ):
            normalized = normalize_candidate(candidate_factory(sample), config)
            recommendation = ShadowRecommendation(
                audit_id=f"adaptive-shadow-{sample.timestamp_ms}-{sample.pid}",
                mode=normalized.mode,
                priority_delta=normalized.priority_delta,
                duration_ms=normalized.duration_ms,
                affinity_ratio=normalized.affinity_ratio,
                rollback_plan=normalized.rollback_plan,
                operator_gate=normalized.operator_gate,
                live_mutation=normalized.live_mutation,
                profile_write=normalized.profile_write,
                rationale=tuple(
                    [
                        f"run_queue_delay_us:{sample.run_queue_delay_us}>={config.adaptive_threshold_us}",
                        *normalized.rationale,
                    ]
                ),
            )

        retained.append(sample)
        decisions.append(
            ReplayDecision(
                timestamp_ms=sample.timestamp_ms,
                pid=sample.pid,
                scenario=sample.scenario,
                static_baseline_triggered=static_triggered,
                drift_score=round(sample.drift_score, 6),
                retained_samples=len(retained),
                freeze_active=freeze_latched,
                freeze_reason=freeze_reason,
                recommendation=recommendation,
            )
        )

    static_metrics = policy_metrics(
        "static_baseline",
        sample_list,
        [decision.static_baseline_triggered for decision in decisions],
        drift_freeze_count=0,
        mutation_count=0,
        max_retained_samples=max((decision.retained_samples for decision in decisions), default=0),
        config=config,
    )
    adaptive_triggers = [decision.recommendation is not None for decision in decisions]
    mutation_count = sum(
        1
        for decision in decisions
        if decision.recommendation
        and (decision.recommendation.live_mutation or decision.recommendation.profile_write)
    )
    adaptive_metrics = policy_metrics(
        "adaptive_shadow",
        sample_list,
        adaptive_triggers,
        drift_freeze_count=sum(1 for decision in decisions if decision.freeze_reason),
        mutation_count=mutation_count,
        max_retained_samples=max((decision.retained_samples for decision in decisions), default=0),
        config=config,
    )
    adaptive_metrics["recommendations_with_rollback"] = sum(
        1
        for decision in decisions
        if decision.recommendation and decision.recommendation.rollback_plan
    )
    adaptive_metrics["unique_recommendation_shapes"] = len(
        {
            (
                decision.recommendation.priority_delta,
                decision.recommendation.duration_ms,
                decision.recommendation.affinity_ratio,
            )
            for decision in decisions
            if decision.recommendation
        }
    )
    adaptive_metrics["stability_pass"] = (
        "PASS" if adaptive_metrics["unique_recommendation_shapes"] <= 1 else "FAIL"
    )

    verdict = "PASS" if gate_passes(config, static_metrics, adaptive_metrics, deterministic_pass) else "FAIL"
    return GateResult(
        config=config,
        decisions=tuple(decisions),
        static_metrics=static_metrics,
        adaptive_metrics=adaptive_metrics,
        deterministic_pass=deterministic_pass,
        verdict=verdict,
    )


def policy_metrics(
    policy_name: str,
    samples: list[ReplaySample],
    triggered: list[bool],
    *,
    drift_freeze_count: int,
    mutation_count: int,
    max_retained_samples: int,
    config: GateConfig,
) -> dict[str, float | int | str]:
    true_positive = sum(
        1 for sample, is_triggered in zip(samples, triggered) if is_triggered and sample.should_intervene
    )
    false_positive = sum(
        1 for sample, is_triggered in zip(samples, triggered) if is_triggered and not sample.should_intervene
    )
    false_negative = sum(
        1 for sample, is_triggered in zip(samples, triggered) if not is_triggered and sample.should_intervene
    )
    evaluated = len(samples)
    regression_budget_rate = (
        (false_positive + false_negative) / evaluated if evaluated else 0.0
    )
    return {
        "policy": policy_name,
        "evaluated_samples": evaluated,
        "trigger_or_recommendation_count": sum(1 for item in triggered if item),
        "true_positive": true_positive,
        "false_positive": false_positive,
        "false_negative": false_negative,
        "drift_freeze_count": drift_freeze_count,
        "mutation_count": mutation_count,
        "max_retained_samples": max_retained_samples,
        "regression_budget_rate": round(regression_budget_rate, 6),
        "regression_budget_limit": config.regression_budget_rate,
    }


def gate_passes(
    config: GateConfig,
    static_metrics: dict[str, float | int | str],
    adaptive_metrics: dict[str, float | int | str],
    deterministic_pass: bool,
) -> bool:
    return all(
        [
            deterministic_pass,
            adaptive_metrics["mutation_count"] == 0,
            adaptive_metrics["trigger_or_recommendation_count"] > 0,
            adaptive_metrics["recommendations_with_rollback"]
            == adaptive_metrics["trigger_or_recommendation_count"],
            adaptive_metrics["drift_freeze_count"] > 0,
            adaptive_metrics["max_retained_samples"] <= config.retention_limit,
            adaptive_metrics["regression_budget_rate"] <= config.regression_budget_rate,
            adaptive_metrics["false_positive"] <= static_metrics["false_positive"],
            adaptive_metrics["stability_pass"] == "PASS",
        ]
    )


def result_payload(result: GateResult) -> dict[str, object]:
    return {
        "config": asdict(result.config),
        "decisions": [decision_payload(decision) for decision in result.decisions],
        "static_metrics": result.static_metrics,
        "adaptive_metrics": result.adaptive_metrics,
        "deterministic_pass": result.deterministic_pass,
        "verdict": result.verdict,
    }


def decision_payload(decision: ReplayDecision) -> dict[str, object]:
    payload = asdict(decision)
    if decision.recommendation is not None:
        payload["recommendation"] = asdict(decision.recommendation)
    return payload


def stable_result_payload(result: GateResult) -> dict[str, object]:
    payload = result_payload(result)
    payload["deterministic_pass"] = True
    payload["verdict"] = "PASS"
    return payload


def load_samples(path: pathlib.Path | None) -> list[ReplaySample]:
    if path is None:
        return default_replay_samples()

    raw = json.loads(path.read_text(encoding="utf-8"))
    if not isinstance(raw, list):
        raise GateViolation("dataset JSON must be a list of replay samples")
    return [ReplaySample(**item) for item in raw]


def write_outputs(result: GateResult, artifact_dir: pathlib.Path) -> dict[str, pathlib.Path]:
    artifact_dir.mkdir(parents=True, exist_ok=True)
    replay_path = artifact_dir / "adaptive_policy_shadow_replay.json"
    benchmark_path = artifact_dir / "adaptive_policy_benchmark.csv"
    report_path = artifact_dir / "adaptive_policy_gate_report.md"

    replay_path.write_text(
        json.dumps(result_payload(result), indent=2, sort_keys=True) + "\n",
        encoding="utf-8",
    )
    write_benchmark_csv(result, benchmark_path)
    report_path.write_text(render_report(result, replay_path, benchmark_path, report_path), encoding="utf-8")
    return {
        "replay": replay_path,
        "benchmark": benchmark_path,
        "report": report_path,
    }


def write_benchmark_csv(result: GateResult, path: pathlib.Path) -> None:
    fieldnames = [
        "policy",
        "evaluated_samples",
        "trigger_or_recommendation_count",
        "true_positive",
        "false_positive",
        "false_negative",
        "drift_freeze_count",
        "mutation_count",
        "max_retained_samples",
        "regression_budget_rate",
        "regression_budget_limit",
        "recommendations_with_rollback",
        "unique_recommendation_shapes",
        "stability_pass",
        "verdict",
    ]
    rows = [
        {**result.static_metrics, "verdict": "REFERENCE"},
        {**result.adaptive_metrics, "verdict": result.verdict},
    ]
    with path.open("w", newline="", encoding="utf-8") as handle:
        writer = csv.DictWriter(handle, fieldnames=fieldnames)
        writer.writeheader()
        for row in rows:
            writer.writerow({field: row.get(field, "") for field in fieldnames})


def render_report(
    result: GateResult,
    replay_path: pathlib.Path,
    benchmark_path: pathlib.Path,
    report_path: pathlib.Path,
) -> str:
    adaptive = result.adaptive_metrics
    static = result.static_metrics
    return "\n".join(
        [
            "# Adaptive Policy Evidence Gate Report",
            "",
            f"- Verdict: `{result.verdict}`",
            "- Runtime behavior: `not_connected`",
            "- Mode: `shadow_only`",
            f"- Deterministic replay: `{'PASS' if result.deterministic_pass else 'FAIL'}`",
            f"- Shadow recommendations: `{adaptive['trigger_or_recommendation_count']}`",
            f"- Live/profile mutations: `{adaptive['mutation_count']}`",
            f"- Drift freezes: `{adaptive['drift_freeze_count']}`",
            f"- Max retained samples: `{adaptive['max_retained_samples']}`",
            f"- Regression budget rate: `{adaptive['regression_budget_rate']}` / `{adaptive['regression_budget_limit']}`",
            f"- Static baseline false positives: `{static['false_positive']}`",
            f"- Adaptive shadow false positives: `{adaptive['false_positive']}`",
            "",
            "## Artifacts",
            "",
            f"- Replay audit: `{replay_path}`",
            f"- Benchmark CSV: `{benchmark_path}`",
            f"- Report: `{report_path}`",
            "",
            "## Promotion Boundary",
            "",
            "- This report is planning evidence only and does not prove host-level benefit.",
            "- A future live mutation path still requires operator approval, live guarded A/B proof, rollback verification, and production profile review.",
            "- `noop` and `dry_run` remain control evidence; adaptive suggestions are not scheduler benefit proof.",
            "",
        ]
    )


def parse_args(argv: list[str]) -> argparse.Namespace:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--dataset-json", type=pathlib.Path)
    parser.add_argument("--artifact-dir", type=pathlib.Path)
    parser.add_argument("--run-id", default=os.environ.get("AEGISAI_ADAPTIVE_POLICY_GATE_RUN_ID"))
    return parser.parse_args(argv)


def main(argv: list[str]) -> int:
    args = parse_args(argv)
    run_id = args.run_id or datetime.now(timezone.utc).strftime("adaptive_policy_gate_%Y%m%dT%H%M%SZ")
    artifact_dir = args.artifact_dir or pathlib.Path(
        os.environ.get(
            "AEGISAI_ADAPTIVE_POLICY_GATE_ARTIFACT_DIR",
            str(REPO_ROOT / ".cache" / "aegisai" / "adaptive_policy_gate" / run_id),
        )
    )

    try:
        samples = load_samples(args.dataset_json)
        first = run_replay(samples, deterministic_pass=True)
        second = run_replay(samples, deterministic_pass=True)
        deterministic_pass = stable_result_payload(first) == stable_result_payload(second)
        result = run_replay(samples, deterministic_pass=deterministic_pass)
        paths = write_outputs(result, artifact_dir)
    except GateViolation as error:
        print(f"adaptive_policy_gate=FAIL reason={error}", file=sys.stderr)
        return 1

    print(
        f"adaptive_policy_gate={result.verdict} "
        f"artifact_dir={artifact_dir} "
        f"report={paths['report']}"
    )
    return 0 if result.verdict == "PASS" else 1


if __name__ == "__main__":
    raise SystemExit(main(sys.argv[1:]))
