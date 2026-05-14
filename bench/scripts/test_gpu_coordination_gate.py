#!/usr/bin/env python3
"""Regression tests for the deferred GPU coordination evidence gate."""

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
GATE_SCRIPT = REPO_ROOT / "bench" / "scripts" / "gpu_coordination_gate.py"


def load_gate_module():
    spec = importlib.util.spec_from_file_location("gpu_coordination_gate", GATE_SCRIPT)
    assert spec is not None
    module = importlib.util.module_from_spec(spec)
    assert spec.loader is not None
    sys.modules[spec.name] = module
    spec.loader.exec_module(module)
    return module


class GpuCoordinationGateTests(unittest.TestCase):
    def setUp(self) -> None:
        self.gate = load_gate_module()

    def test_default_inventory_is_deterministic_and_observe_only(self) -> None:
        observations = self.gate.default_host_observations()
        first = self.gate.plan_hosts(observations)
        second = self.gate.plan_hosts(observations)

        self.assertEqual(
            self.gate.stable_result_payload(first),
            self.gate.stable_result_payload(second),
        )
        self.assertEqual(first.verdict, "PASS")
        self.assertEqual(first.metrics["mutation_count"], 0)
        self.assertGreater(first.metrics["dry_run_plan_count"], 0)
        self.assertGreater(first.metrics["unsupported_host_noop_count"], 0)
        self.assertEqual(first.metrics["unsupported_host_smoke"], "PASS")
        self.assertEqual(first.metrics["gpu_host_dry_run_proof"], "PASS")
        for plan in first.plans:
            self.assertEqual(plan.mode, "observe_plan_only")
            self.assertFalse(plan.live_mutation)
            if plan.status == "dry_run_plan":
                self.assertEqual(plan.gpu_vendor, "nvidia")
                self.assertTrue(plan.target_id)
            else:
                self.assertIn(plan.status, {"unsupported_noop", "telemetry_unavailable_noop"})

    def test_safety_matrix_rejects_future_mutation_paths(self) -> None:
        matrix = self.gate.default_safety_matrix()

        self.assertEqual(len(matrix), 6)
        self.assertTrue(all(not row.accepted for row in matrix))
        reasons = {row.name: set(row.reasons) for row in matrix}
        self.assertIn(
            "live_gpu_mutation_denied_by_default",
            reasons["live_action_default_denied"],
        )
        self.assertIn("target_not_in_allowlist", reasons["missing_target_allowlist"])
        self.assertIn("unsupported_vendor:amd", reasons["unsupported_vendor"])
        self.assertIn(
            "device_isolation_review_missing",
            reasons["missing_isolation_review"],
        )
        self.assertIn("privilege_review_missing", reasons["missing_privilege_review"])
        self.assertIn("rollback_or_noop_plan_missing", reasons["missing_rollback"])

    def test_overhead_budget_failure_blocks_gate(self) -> None:
        observations = list(self.gate.default_host_observations())
        bad = self.gate.HostObservation(
            **{
                **self.gate.asdict(observations[0]),
                "dry_run_latency_ms": 140.0,
            }
        )
        result = self.gate.plan_hosts([bad, *observations[1:]])

        self.assertEqual(result.metrics["overhead_budget"], "FAIL")
        self.assertEqual(result.verdict, "FAIL")

    def test_cli_writes_plan_benchmark_safety_and_report_artifacts(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            root = pathlib.Path(tmp)
            artifact_dir = root / "gpu_gate"
            env = os.environ.copy()
            env.update(
                {
                    "AEGISAI_GPU_COORDINATION_GATE_RUN_ID": "unit",
                    "AEGISAI_GPU_COORDINATION_GATE_ARTIFACT_DIR": str(artifact_dir),
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
            self.assertIn("gpu_coordination_gate=PASS", result.stdout)
            plan_path = artifact_dir / "gpu_coordination_plan.json"
            benchmark_path = artifact_dir / "gpu_coordination_benchmark.csv"
            safety_path = artifact_dir / "gpu_coordination_safety_matrix.csv"
            report_path = artifact_dir / "gpu_coordination_gate_report.md"
            self.assertTrue(plan_path.is_file())
            self.assertTrue(benchmark_path.is_file())
            self.assertTrue(safety_path.is_file())
            self.assertTrue(report_path.is_file())

            with benchmark_path.open(newline="", encoding="utf-8") as handle:
                rows = list(csv.DictReader(handle))
            self.assertTrue(rows)
            self.assertIn("dry_run_plan", {row["status"] for row in rows})
            with safety_path.open(newline="", encoding="utf-8") as handle:
                safety_rows = list(csv.DictReader(handle))
            self.assertEqual(len(safety_rows), 6)
            self.assertTrue(all(row["accepted"] == "false" for row in safety_rows))
            report = report_path.read_text(encoding="utf-8")
            self.assertIn("- Runtime behavior: `not_connected`", report)
            self.assertIn("- Mode: `observe_plan_only`", report)


if __name__ == "__main__":
    unittest.main()
