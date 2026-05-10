#!/usr/bin/env python3
"""Regression tests for the Phase 4 MVP benefit report gate."""

from __future__ import annotations

import csv
import os
import pathlib
import subprocess
import tempfile
import unittest


REPO_ROOT = pathlib.Path(__file__).resolve().parents[2]
REPORT_SCRIPT = REPO_ROOT / "bench" / "scripts" / "inference_tail_guard_phase4_report.sh"


def write_csv(path: pathlib.Path, fieldnames: list[str], rows: list[dict[str, str]]) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    with path.open("w", newline="", encoding="utf-8") as handle:
        writer = csv.DictWriter(handle, fieldnames=fieldnames)
        writer.writeheader()
        writer.writerows(rows)


def summary_row(mode: str, metric_ms: float) -> dict[str, str]:
    baseline = mode == "baseline"
    return {
        "mode": mode,
        "backend": "none" if baseline else mode,
        "samples_ok": "1",
        "samples_total": "1",
        "ttft_p95_ms": f"{metric_ms:.3f}",
        "ttft_p99_ms": f"{metric_ms:.3f}",
        "latency_p95_ms": f"{metric_ms:.3f}",
        "latency_p99_ms": f"{metric_ms:.3f}",
        "jitter_ms": f"{metric_ms:.3f}",
        "trigger_count": "0" if baseline else "1",
        "rollback_count": "0" if baseline else "1",
        "cpu_migration_total": "0",
        "cpu_migrations_per_sec_max": "0",
        "major_page_fault_total": "0",
        "major_page_faults_per_sec_max": "0",
        "offcpu_time_events": "0",
    }


def counts_row(mode: str) -> dict[str, str]:
    return {
        "mode": mode,
        "action_error_count": "0",
        "daemon_status": "0",
        "stress_exhausted": "0",
        "cpu_migration_total": "0",
        "cpu_migrations_per_sec_max": "0",
        "major_page_fault_total": "0",
        "major_page_faults_per_sec_max": "0",
        "offcpu_time_events": "0",
    }


class Phase4ReportGateTests(unittest.TestCase):
    def run_report(
        self,
        root: pathlib.Path,
        *,
        control_metric_ms: float,
        live_metric_ms: float,
        effective_live_actions: bool,
    ) -> subprocess.CompletedProcess[str]:
        artifact_dir = root / "artifacts"
        modes = ["baseline", "noop_observation", "dry_run", "live_guarded"]
        for round_no in range(1, 4):
            round_dir = artifact_dir / "cpu" / f"round_{round_no}"
            write_csv(
                round_dir / "summary.csv",
                list(summary_row("baseline", 100.0).keys()),
                [
                    summary_row("baseline", 100.0),
                    summary_row("noop_observation", control_metric_ms),
                    summary_row("dry_run", control_metric_ms),
                    summary_row("live_guarded", live_metric_ms),
                ],
            )
            write_csv(
                round_dir / "mode_counts.csv",
                list(counts_row("baseline").keys()),
                [counts_row(mode) for mode in modes],
            )
            live_dir = round_dir / "live_guarded"
            live_dir.mkdir(parents=True, exist_ok=True)
            if effective_live_actions:
                (live_dir / "daemon.log").write_text(
                    "backend.apply.apply.1.detail=command=taskset -pc 0-1 1234;"
                    "output=pid 1234's current affinity list: 0-3\n"
                    "pid 1234's new affinity list: 0-1\n",
                    encoding="utf-8",
                )
            else:
                (live_dir / "daemon.log").write_text(
                    "backend.apply.apply.1.detail=command=taskset -pc 0-1 1234;"
                    "output=pid 1234's current affinity list: 0-1\n"
                    "pid 1234's new affinity list: 0-1\n"
                    "backend.apply.priority_raise_limited=true\n",
                    encoding="utf-8",
                )

        env = os.environ.copy()
        env.update(
            {
                "AEGISAI_PHASE4_ARTIFACT_DIR": str(artifact_dir),
                "AEGISAI_PHASE4_REPORT": str(root / "mvp_benefit_report.md"),
                "AEGISAI_VERIFY_LOG": str(root / "verification_log.md"),
                "AEGISAI_PHASE4_REUSE_ARTIFACTS": "1",
                "AEGISAI_PHASE4_RUN_ID": "phase4_gate_unit",
                "AEGISAI_PHASE4_SCENARIOS": "cpu",
                "AEGISAI_PHASE4_MODES": ",".join(modes),
                "AEGISAI_PHASE4_ROUNDS": "3",
                "AEGISAI_AB_SAMPLES": "1",
                "AEGISAI_AB_CONCURRENCY": "1",
            }
        )
        return subprocess.run(
            ["bash", str(REPORT_SCRIPT)],
            cwd=REPO_ROOT,
            env=env,
            text=True,
            capture_output=True,
            check=False,
        )

    def test_noop_and_dry_run_trends_do_not_pass_without_live_trend(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            root = pathlib.Path(tmp)
            result = self.run_report(
                root,
                control_metric_ms=80.0,
                live_metric_ms=100.0,
                effective_live_actions=True,
            )

            report = (root / "mvp_benefit_report.md").read_text(encoding="utf-8")
            self.assertNotEqual(result.returncode, 0, result.stdout + result.stderr)
            self.assertIn("Result: `FAIL`", report)
            self.assertIn("Apparent improvements were limited to observation or dry-run modes", report)

    def test_live_trend_without_effective_live_action_does_not_pass(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            root = pathlib.Path(tmp)
            result = self.run_report(
                root,
                control_metric_ms=100.0,
                live_metric_ms=80.0,
                effective_live_actions=False,
            )

            report = (root / "mvp_benefit_report.md").read_text(encoding="utf-8")
            self.assertNotEqual(result.returncode, 0, result.stdout + result.stderr)
            self.assertIn("Result: `FAIL`", report)
            self.assertIn("no effective live actuator changes were observed", report)

    def test_live_trend_with_effective_live_action_passes(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            root = pathlib.Path(tmp)
            result = self.run_report(
                root,
                control_metric_ms=100.0,
                live_metric_ms=80.0,
                effective_live_actions=True,
            )

            report = (root / "mvp_benefit_report.md").read_text(encoding="utf-8")
            self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
            self.assertIn("Result: `PASS`", report)
            self.assertIn("stable improvement trend with effective host-level actuator changes", report)


if __name__ == "__main__":
    unittest.main()
