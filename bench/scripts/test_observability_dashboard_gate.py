#!/usr/bin/env python3
"""Regression tests for the deferred observability dashboard evidence gate."""

from __future__ import annotations

import csv
import importlib.util
import json
import os
import pathlib
import subprocess
import sys
import tempfile
import unittest


REPO_ROOT = pathlib.Path(__file__).resolve().parents[2]
GATE_SCRIPT = REPO_ROOT / "bench" / "scripts" / "observability_dashboard_gate.py"


def load_gate_module():
    spec = importlib.util.spec_from_file_location("observability_dashboard_gate", GATE_SCRIPT)
    assert spec is not None
    module = importlib.util.module_from_spec(spec)
    assert spec.loader is not None
    sys.modules[spec.name] = module
    spec.loader.exec_module(module)
    return module


class ObservabilityDashboardGateTests(unittest.TestCase):
    def setUp(self) -> None:
        self.gate = load_gate_module()

    def test_default_artifacts_are_deterministic_and_read_only(self) -> None:
        inputs = self.gate.default_dashboard_inputs()
        first = self.gate.run_dashboard_gate(inputs)
        second = self.gate.run_dashboard_gate(inputs)

        self.assertEqual(
            self.gate.stable_result_payload(first),
            self.gate.stable_result_payload(second),
        )
        self.assertEqual(first.verdict, "PASS")
        self.assertEqual(first.snapshot.mode, "read_only")
        self.assertEqual(first.snapshot.auth_scope, "local_operator")
        self.assertEqual(first.snapshot.control_path_count, 0)
        self.assertEqual(first.snapshot.redaction_status, "PASS")
        self.assertEqual(first.snapshot.benefit_truth_source, "artifact")
        self.assertGreater(first.metrics["runtime_audit_records"], 0)
        self.assertGreater(first.metrics["verification_artifacts"], 0)
        self.assertGreater(first.metrics["telemetry_exports"], 0)

    def test_rejects_control_paths_sensitive_fields_and_dashboard_benefit_truth(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            path = pathlib.Path(tmp) / "bad_dashboard.json"
            raw = json.loads(
                (REPO_ROOT / "bench" / "observability_dashboard_gate" / "default_artifacts.json").read_text(
                    encoding="utf-8"
                )
            )
            raw["runtime_audit"][0]["scheduler_action"] = "renice"
            path.write_text(json.dumps(raw), encoding="utf-8")

            with self.assertRaisesRegex(self.gate.GateViolation, "scheduler_action"):
                self.gate.load_inputs(path)
                self.gate.run_dashboard_gate(self.gate.load_inputs(path))

            raw = json.loads(
                (REPO_ROOT / "bench" / "observability_dashboard_gate" / "default_artifacts.json").read_text(
                    encoding="utf-8"
                )
            )
            raw["runtime_audit"][0]["redaction"] = "raw"
            raw["runtime_audit"][0]["raw_cmdline"] = "python -m workload --token secret"
            path.write_text(json.dumps(raw), encoding="utf-8")
            with self.assertRaisesRegex(self.gate.GateViolation, "raw_cmdline"):
                self.gate.run_dashboard_gate(self.gate.load_inputs(path))

            raw = json.loads(
                (REPO_ROOT / "bench" / "observability_dashboard_gate" / "default_artifacts.json").read_text(
                    encoding="utf-8"
                )
            )
            raw["verification_artifacts"][0]["benefit_source"] = "dashboard"
            path.write_text(json.dumps(raw), encoding="utf-8")
            with self.assertRaisesRegex(self.gate.GateViolation, "artifact-derived"):
                self.gate.run_dashboard_gate(self.gate.load_inputs(path))

    def test_overhead_budget_failure_blocks_gate(self) -> None:
        inputs = self.gate.default_dashboard_inputs()
        bad_inputs = self.gate.DashboardInputs(
            runtime_audit=inputs.runtime_audit,
            verification_artifacts=inputs.verification_artifacts,
            telemetry_exports=inputs.telemetry_exports,
            benchmark={
                **inputs.benchmark,
                "dashboard_render_ms": 80.0,
            },
        )
        result = self.gate.run_dashboard_gate(bad_inputs)

        self.assertEqual(result.metrics["overhead_budget"], "FAIL")
        self.assertEqual(result.verdict, "FAIL")

    def test_cli_writes_snapshot_export_and_report_artifacts(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            root = pathlib.Path(tmp)
            artifact_dir = root / "dashboard_gate"
            env = os.environ.copy()
            env.update(
                {
                    "AEGISAI_OBSERVABILITY_DASHBOARD_GATE_RUN_ID": "unit",
                    "AEGISAI_OBSERVABILITY_DASHBOARD_GATE_ARTIFACT_DIR": str(artifact_dir),
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
            self.assertIn("observability_dashboard_gate=PASS", result.stdout)
            snapshot_path = artifact_dir / "observability_dashboard_snapshot.json"
            export_path = artifact_dir / "observability_dashboard_export.csv"
            report_path = artifact_dir / "observability_dashboard_gate_report.md"
            self.assertTrue(snapshot_path.is_file())
            self.assertTrue(export_path.is_file())
            self.assertTrue(report_path.is_file())

            with export_path.open(newline="", encoding="utf-8") as handle:
                rows = list(csv.DictReader(handle))
            self.assertEqual(
                {row["row_type"] for row in rows},
                {"runtime_audit", "verification_artifact", "telemetry_export"},
            )
            snapshot = json.loads(snapshot_path.read_text(encoding="utf-8"))
            self.assertEqual(snapshot["snapshot"]["mode"], "read_only")
            self.assertEqual(snapshot["snapshot"]["control_path_count"], 0)
            report = report_path.read_text(encoding="utf-8")
            self.assertIn("- Runtime behavior: `not_connected`", report)
            self.assertIn("- Mode: `read_only`", report)


if __name__ == "__main__":
    unittest.main()
