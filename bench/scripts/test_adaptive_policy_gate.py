#!/usr/bin/env python3
"""Regression tests for the deferred adaptive policy evidence gate."""

from __future__ import annotations

import csv
import importlib.util
import os
import pathlib
import subprocess
import sys
import tempfile
import unittest


REPO_ROOT = pathlib.Path(__file__).resolve().parents[2]
GATE_SCRIPT = REPO_ROOT / "bench" / "scripts" / "adaptive_policy_gate.py"


def load_gate_module():
    spec = importlib.util.spec_from_file_location("adaptive_policy_gate", GATE_SCRIPT)
    assert spec is not None
    module = importlib.util.module_from_spec(spec)
    assert spec.loader is not None
    sys.modules[spec.name] = module
    spec.loader.exec_module(module)
    return module


class AdaptivePolicyGateTests(unittest.TestCase):
    def setUp(self) -> None:
        self.gate = load_gate_module()

    def test_default_replay_is_deterministic_and_shadow_only(self) -> None:
        samples = self.gate.default_replay_samples()
        first = self.gate.run_replay(samples)
        second = self.gate.run_replay(samples)

        self.assertEqual(
            self.gate.stable_result_payload(first),
            self.gate.stable_result_payload(second),
        )
        self.assertEqual(first.verdict, "PASS")
        self.assertEqual(first.adaptive_metrics["mutation_count"], 0)
        self.assertGreater(first.adaptive_metrics["trigger_or_recommendation_count"], 0)
        for decision in first.decisions:
            if decision.recommendation is None:
                continue
            self.assertEqual(decision.recommendation.mode, "shadow")
            self.assertFalse(decision.recommendation.live_mutation)
            self.assertFalse(decision.recommendation.profile_write)
            self.assertEqual(
                decision.recommendation.operator_gate,
                "required_before_live_mutation",
            )
            self.assertEqual(decision.recommendation.priority_delta, -5)
            self.assertEqual(decision.recommendation.duration_ms, 1_000)
            self.assertEqual(decision.recommendation.affinity_ratio, 0.5)
            self.assertTrue(decision.recommendation.rollback_plan)

    def test_safety_invariants_reject_live_or_profile_mutation(self) -> None:
        samples = self.gate.default_replay_samples()

        def live_candidate(_sample):
            return self.gate.CandidateAction(live_mutation=True)

        def profile_write_candidate(_sample):
            return self.gate.CandidateAction(profile_write=True)

        def missing_rollback_candidate(_sample):
            return self.gate.CandidateAction(rollback_plan=())

        with self.assertRaisesRegex(self.gate.GateViolation, "live mutation"):
            self.gate.run_replay(samples, candidate_factory=live_candidate)
        with self.assertRaisesRegex(self.gate.GateViolation, "profile write"):
            self.gate.run_replay(samples, candidate_factory=profile_write_candidate)
        with self.assertRaisesRegex(self.gate.GateViolation, "rollback plan"):
            self.gate.run_replay(samples, candidate_factory=missing_rollback_candidate)

    def test_freeze_and_retention_are_bounded(self) -> None:
        config = self.gate.GateConfig(retention_limit=2)
        result = self.gate.run_replay(self.gate.default_replay_samples(), config=config)

        self.assertLessEqual(result.adaptive_metrics["max_retained_samples"], 2)
        freeze_decisions = [decision for decision in result.decisions if decision.freeze_reason]
        self.assertTrue(freeze_decisions)
        self.assertIsNone(freeze_decisions[0].recommendation)
        self.assertIn("drift_score", freeze_decisions[0].freeze_reason)

        frozen = self.gate.run_replay(
            self.gate.default_replay_samples(),
            config=self.gate.GateConfig(freeze_switch=True),
        )
        self.assertEqual(frozen.adaptive_metrics["trigger_or_recommendation_count"], 0)
        self.assertEqual(frozen.verdict, "FAIL")

    def test_cli_writes_shadow_smoke_and_benchmark_artifacts(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            root = pathlib.Path(tmp)
            artifact_dir = root / "adaptive_gate"
            env = os.environ.copy()
            env.update(
                {
                    "AEGISAI_ADAPTIVE_POLICY_GATE_RUN_ID": "unit",
                    "AEGISAI_ADAPTIVE_POLICY_GATE_ARTIFACT_DIR": str(artifact_dir),
                }
            )
            result = subprocess.run(
                ["python3", str(GATE_SCRIPT)],
                cwd=REPO_ROOT,
                env=env,
                text=True,
                capture_output=True,
                check=False,
            )

            self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
            self.assertIn("adaptive_policy_gate=PASS", result.stdout)
            replay_path = artifact_dir / "adaptive_policy_shadow_replay.json"
            benchmark_path = artifact_dir / "adaptive_policy_benchmark.csv"
            report_path = artifact_dir / "adaptive_policy_gate_report.md"
            self.assertTrue(replay_path.is_file())
            self.assertTrue(benchmark_path.is_file())
            self.assertTrue(report_path.is_file())

            with benchmark_path.open(newline="", encoding="utf-8") as handle:
                rows = list(csv.DictReader(handle))
            self.assertEqual([row["policy"] for row in rows], ["static_baseline", "adaptive_shadow"])
            adaptive = rows[1]
            self.assertEqual(adaptive["verdict"], "PASS")
            self.assertEqual(adaptive["mutation_count"], "0")
            self.assertEqual(adaptive["stability_pass"], "PASS")
            report = report_path.read_text(encoding="utf-8")
            self.assertIn("- Runtime behavior: `not_connected`", report)
            self.assertIn("- Mode: `shadow_only`", report)


if __name__ == "__main__":
    unittest.main()
