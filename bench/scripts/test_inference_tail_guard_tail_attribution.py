#!/usr/bin/env python3
"""Regression tests for Tail Guard attribution reporting."""

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
SCRIPT = REPO_ROOT / "bench" / "scripts" / "inference_tail_guard_tail_attribution.py"


def load_module():
    spec = importlib.util.spec_from_file_location("tail_attribution", SCRIPT)
    assert spec is not None
    module = importlib.util.module_from_spec(spec)
    assert spec.loader is not None
    sys.modules[spec.name] = module
    spec.loader.exec_module(module)
    return module


def write_csv(path: pathlib.Path, rows: list[dict[str, str]]) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    with path.open("w", newline="", encoding="utf-8") as handle:
        writer = csv.DictWriter(handle, fieldnames=list(rows[0].keys()))
        writer.writeheader()
        writer.writerows(rows)


def phase4_row(root: pathlib.Path, round_id: int, mode: str, latency: float) -> dict[str, str]:
    round_dir = root / "phase4" / "cpu" / f"round_{round_id}"
    return {
        "scenario": "cpu",
        "scenario_label": "CPU interference",
        "round": str(round_id),
        "run_status": "0",
        "changed_variable": "sample_sizing",
        "mode": mode,
        "backend": "none" if mode == "baseline" else mode,
        "samples_ok": "8",
        "samples_total": "8",
        "ttft_p95_ms": "100.000",
        "ttft_p99_ms": "100.000",
        "latency_p95_ms": f"{latency:.3f}",
        "latency_p99_ms": f"{latency:.3f}",
        "jitter_ms": "10.000",
        "trigger_count": "0" if mode == "baseline" else "3",
        "rollback_count": "0" if mode == "baseline" else "3",
        "action_error_count": "0",
        "mode_contract": "PASS",
        "mode_contract_reason": "ok",
        "cpu_migration_total": "0",
        "cpu_migrations_per_sec_max": "0",
        "major_page_fault_total": "0",
        "major_page_faults_per_sec_max": "0",
        "offcpu_time_events": "0",
        "live_effective_action_count": "1" if mode == "live_guarded" else "0",
        "live_priority_limited_count": "0",
        "artifact_dir": str(round_dir),
    }


def write_daemon(round_dir: pathlib.Path, mode: str, *, runq_us: int, offcpu_us: int, io_us: int) -> None:
    mode_dir = round_dir / mode
    mode_dir.mkdir(parents=True, exist_ok=True)
    offcpu_events = 4 if offcpu_us > 0 else 0
    io_events = 5 if io_us > 0 else 0
    (mode_dir / "daemon.log").write_text(
        "\n".join(
            [
                "AegisAI Runtime Daemon Summary",
                "signal_observations:",
                f"  run_queue_delay: events=7 total={runq_us * 2} max={runq_us}",
                "  cpu_migration: events=3 total=9 max=3",
                "  major_page_fault: events=1 total=2 max=2",
                f"  offcpu_time: events={offcpu_events} total=100 max=50",
                f"  io_latency: events={io_events} total=100 max=50",
                "feature_window_maxima:",
                f"  run_queue_delay_us_max: {runq_us}",
                f"  offcpu_time_us_max: {offcpu_us}",
                f"  optional_io_latency_us_max: {io_us}",
                "audit_highlights:",
                "  pid=10;scenario=inference_tail_guard;backend.apply.apply.0.detail=command=renice 5 -p 10",
                "  pid=10;scenario=inference_tail_guard;backend.rollback.rollback.0.detail=command=renice 0 -p 10",
            ]
        )
        + "\n",
        encoding="utf-8",
    )


class TailAttributionTests(unittest.TestCase):
    def setUp(self) -> None:
        self.module = load_module()

    def test_duration_backed_signals_can_make_target_plausible(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            root = pathlib.Path(tmp)
            rows = [
                phase4_row(root, 1, "baseline", 1000.0),
                phase4_row(root, 1, "live_guarded", 900.0),
            ]
            detail = root / "phase4" / "phase4_runs.csv"
            write_csv(detail, rows)
            write_daemon(root / "phase4" / "cpu" / "round_1", "live_guarded", runq_us=100_000, offcpu_us=40_000, io_us=20_000)
            artifact_dir = root / "attr"
            report = root / "tail_report.md"

            result = subprocess.run(
                [
                    "python3",
                    str(SCRIPT),
                    "--phase4-runs",
                    str(detail),
                    "--run-id",
                    "unit",
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
            self.assertIn("tail_attribution=PLAUSIBLE", result.stdout)
            summary = json.loads((artifact_dir / "tail_attribution_summary.json").read_text(encoding="utf-8"))
            self.assertEqual(summary["p95_or_p99_15pct_plausibility"], "PLAUSIBLE")
            self.assertGreaterEqual(summary["scheduler_attributable_tail_pct_max"], 15.0)
            csv_text = (artifact_dir / "tail_attribution.csv").read_text(encoding="utf-8")
            self.assertIn("run_queue_delay_ms_max", csv_text)
            self.assertIn("audit_counts_only", csv_text)
            self.assertIn("P95/P99 >=15% plausibility: `PLAUSIBLE`", report.read_text(encoding="utf-8"))

    def test_helper_gap_is_explicit_when_duration_signals_are_too_small(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            root = pathlib.Path(tmp)
            rows = [
                phase4_row(root, 1, "baseline", 1000.0),
                phase4_row(root, 1, "live_guarded", 980.0),
            ]
            detail = root / "phase4" / "phase4_runs.csv"
            write_csv(detail, rows)
            write_daemon(root / "phase4" / "cpu" / "round_1", "live_guarded", runq_us=1_000, offcpu_us=0, io_us=0)

            args = self.module.parse_args(
                [
                    "--phase4-runs",
                    str(detail),
                    "--run-id",
                    "unit",
                    "--artifact-dir",
                    str(root / "attr"),
                    "--report",
                    str(root / "tail_report.md"),
                ]
            )
            summary = self.module.run(args)

            self.assertEqual(summary.p95_or_p99_15pct_plausibility, "NOT_PROVEN_HELPER_GAP")
            self.assertEqual(summary.helper_backed_signal_status, "excluded_or_zero")
            self.assertLess(summary.scheduler_attributable_tail_pct_max, 15.0)


if __name__ == "__main__":
    unittest.main()
