#!/usr/bin/env python3
"""Regression tests for Tail Guard background demotion dry-run planning."""

from __future__ import annotations

import csv
import importlib.util
import json
import pathlib
import subprocess
import sys
import tempfile
import unittest


REPO_ROOT = pathlib.Path(__file__).resolve().parents[2]
SCRIPT = REPO_ROOT / "bench" / "scripts" / "inference_tail_guard_background_demotion_planner.py"


def load_module():
    spec = importlib.util.spec_from_file_location("background_demotion_planner", SCRIPT)
    assert spec is not None
    module = importlib.util.module_from_spec(spec)
    assert spec.loader is not None
    sys.modules[spec.name] = module
    spec.loader.exec_module(module)
    return module


def write_inventory(path: pathlib.Path) -> None:
    path.write_text(
        json.dumps(
            {
                "processes": [
                    {
                        "pid": 100,
                        "ppid": 1,
                        "name": "ollama",
                        "cmdline": "ollama serve",
                        "cgroup_path": "/user.slice",
                        "tags": ["AI_INFERENCE", "INTERACTIVE_LATENCY_SENSITIVE"],
                        "nice": 0,
                        "cpus_allowed_list": "0-3",
                    },
                    {
                        "pid": 200,
                        "ppid": 1,
                        "name": "stress-ng",
                        "cmdline": "stress-ng --cpu 2",
                        "cgroup_path": "/background.slice",
                        "tags": ["BACKGROUND_JOB"],
                        "nice": 0,
                        "cpus_allowed_list": "0-3",
                    },
                    {
                        "pid": 201,
                        "ppid": 1,
                        "name": "python",
                        "cmdline": "python batch_worker.py",
                        "cgroup_path": "/background.slice",
                        "tags": ["BACKGROUND_JOB"],
                        "nice": 1,
                        "cpus_allowed_list": "0-3",
                    },
                    {
                        "pid": 300,
                        "ppid": 1,
                        "name": "python",
                        "cmdline": "python service.py",
                        "cgroup_path": "/user.slice",
                        "tags": [],
                        "nice": 0,
                        "cpus_allowed_list": "0-3",
                    },
                    {
                        "pid": 400,
                        "ppid": 1,
                        "name": "bash",
                        "cmdline": "bash",
                        "cgroup_path": "/user.slice",
                        "tags": [],
                        "nice": 0,
                        "cpus_allowed_list": "0-3",
                        "tty_nr": 34816,
                    },
                ]
            },
            indent=2,
        )
        + "\n",
        encoding="utf-8",
    )


class BackgroundDemotionPlannerTests(unittest.TestCase):
    def setUp(self) -> None:
        self.module = load_module()

    def test_planner_rejects_unknown_and_interactive_processes(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            root = pathlib.Path(tmp)
            inventory = root / "inventory.json"
            write_inventory(inventory)
            records = self.module.load_inventory(inventory)
            rows = self.module.build_plan(records, {100}, 1)
            summary = self.module.summarize("unit", rows, 1)

            self.assertEqual(summary.verdict, "PASS")
            self.assertEqual(summary.live_mutation_count, 0)
            self.assertEqual(summary.protected_inference_count, 1)
            self.assertEqual(summary.candidate_background_count, 1)
            self.assertEqual(summary.rejected_unknown_count, 1)
            self.assertEqual(summary.rejected_interactive_count, 1)
            self.assertEqual(summary.rejected_limit_count, 1)
            candidate = [row for row in rows if row.decision == "candidate"][0]
            self.assertEqual(candidate.pid, 200)
            self.assertEqual(candidate.proposed_nice, "5")
            self.assertEqual(candidate.proposed_cpu_weight, "50")
            self.assertFalse(candidate.live_mutation)

    def test_cli_writes_json_csv_and_report_without_live_mutation(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            root = pathlib.Path(tmp)
            inventory = root / "inventory.json"
            artifact_dir = root / "artifacts"
            report = root / "background_plan.md"
            write_inventory(inventory)

            result = subprocess.run(
                [
                    "python3",
                    str(SCRIPT),
                    "--run-id",
                    "unit",
                    "--inventory",
                    str(inventory),
                    "--protected-pids",
                    "100",
                    "--max-candidates",
                    "1",
                    "--artifact-dir",
                    str(artifact_dir),
                    "--report",
                    str(report),
                ],
                cwd=REPO_ROOT,
                text=True,
                capture_output=True,
                check=False,
            )

            self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
            self.assertIn("background_demotion_planner=PASS", result.stdout)
            payload = json.loads((artifact_dir / "background_demotion_plan.json").read_text(encoding="utf-8"))
            self.assertFalse(payload["live_mutation"])
            self.assertEqual(payload["summary"]["affected_set_count"], 1)
            with (artifact_dir / "background_demotion_candidates.csv").open(newline="", encoding="utf-8") as handle:
                rows = list(csv.DictReader(handle))
            self.assertEqual(len(rows), 5)
            self.assertTrue(all(row["live_mutation"] == "false" for row in rows))
            report_text = report.read_text(encoding="utf-8")
            self.assertIn("- Mode: `dry_run_only`", report_text)
            self.assertIn("Unknown processes are rejected", report_text)


if __name__ == "__main__":
    unittest.main()
