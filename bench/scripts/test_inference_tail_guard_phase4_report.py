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


def summary_row(mode: str, metric_ms: float, samples: int = 1) -> dict[str, str]:
    baseline = mode == "baseline"
    return {
        "mode": mode,
        "backend": "none" if baseline else mode,
        "samples_ok": str(samples),
        "samples_total": str(samples),
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


def contract_row(mode: str, status: str = "PASS", reason: str = "ok") -> dict[str, str]:
    return {
        "mode": mode,
        "acceptance_gate": "control_latency" if mode == "baseline" else mode,
        "backend": "none" if mode == "baseline" else mode,
        "request_contract": "PASS",
        "recognition_contract": "n/a" if mode == "baseline" else "PASS",
        "observation_signal_contract": "n/a" if mode == "baseline" else "PASS",
        "audit_contract": "n/a" if mode in ("baseline", "noop_observation") else "PASS",
        "live_nice_only_contract": "PASS" if mode == "live_guarded" else "n/a",
        "live_affinity_contract": "n/a",
        "live_cpuset_disabled_contract": "PASS" if mode == "live_guarded" else "n/a",
        "actuator_quality_contract": "PASS" if mode == "live_guarded" else "n/a",
        "live_permission_contract": "PASS" if mode == "live_guarded" else "n/a",
        "live_command_contract": "PASS" if mode == "live_guarded" else "n/a",
        "mode_contract": status,
        "reason": reason,
    }


def live_daemon_log(action_evidence: str) -> str:
    if action_evidence == "effective_taskset":
        return (
            "backend.apply.apply.1.detail=command=taskset -pc 0-1 1234;"
            "output=pid 1234's current affinity list: 0-3\n"
            "pid 1234's new affinity list: 0-1\n"
        )
    if action_evidence == "noop_taskset":
        return (
            "backend.apply.apply.1.detail=command=taskset -pc 0-1 1234;"
            "output=pid 1234's current affinity list: 0-1\n"
            "pid 1234's new affinity list: 0-1\n"
        )
    if action_evidence == "priority_limited":
        return (
            "backend.apply.apply.1.detail=command=taskset -pc 0-1 1234;"
            "output=pid 1234's current affinity list: 0-1\n"
            "pid 1234's new affinity list: 0-1\n"
            "backend.apply.priority_raise_limited=true\n"
        )
    raise ValueError(f"unknown live action evidence: {action_evidence}")


def write_run_env(
    path: pathlib.Path,
    *,
    artifact_dir: pathlib.Path,
    modes: list[str],
    samples: int,
    live_confirm: str,
    live_pid_allowlist: str,
    overrides: dict[str, str] | None = None,
) -> None:
    values = {
        "artifact_dir": str(artifact_dir),
        "modes": " ".join(modes),
        "model": "qwen2.5:0.5b",
        "num_predict": "96",
        "samples_per_mode": str(samples),
        "concurrency": "1",
        "stress_cpu": "2",
        "stress_io": "0",
        "stress_hdd": "0",
        "stress_hdd_bytes": "128M",
        "live_confirm": live_confirm,
        "live_pid_allowlist": live_pid_allowlist,
        "live_enable_affinity": "0",
    }
    values.update(overrides or {})
    path.write_text(
        "".join(f"{key}={value}\n" for key, value in values.items()),
        encoding="utf-8",
    )


class Phase4ReportGateTests(unittest.TestCase):
    def run_report(
        self,
        root: pathlib.Path,
        *,
        control_metric_ms: float,
        live_metric_ms: float,
        live_action_evidence: str,
        samples: int = 3,
        rounds: int = 3,
        live_metrics_by_round: list[float] | None = None,
        modes: list[str] | None = None,
        live_confirm: str = "1",
        live_pid_allowlist: str = "1234",
        contract_status_by_mode: dict[str, str] | None = None,
        write_provenance: bool = True,
        run_env_overrides: dict[str, str] | None = None,
    ) -> subprocess.CompletedProcess[str]:
        artifact_dir = root / "artifacts"
        modes = modes or ["baseline", "noop_observation", "dry_run", "live_guarded"]
        contract_status_by_mode = contract_status_by_mode or {}
        for round_no in range(1, rounds + 1):
            round_dir = artifact_dir / "cpu" / f"round_{round_no}"
            current_live_metric_ms = (
                live_metrics_by_round[round_no - 1]
                if live_metrics_by_round is not None
                else live_metric_ms
            )
            metric_by_mode = {
                "baseline": 100.0,
                "noop_observation": control_metric_ms,
                "dry_run": control_metric_ms,
                "live_guarded": current_live_metric_ms,
            }
            write_csv(
                round_dir / "summary.csv",
                list(summary_row("baseline", 100.0).keys()),
                [summary_row(mode, metric_by_mode[mode], samples) for mode in modes],
            )
            write_csv(
                round_dir / "mode_counts.csv",
                list(counts_row("baseline").keys()),
                [counts_row(mode) for mode in modes],
            )
            write_csv(
                round_dir / "mode_contract.csv",
                list(contract_row("baseline").keys()),
                [
                    contract_row(
                        mode,
                        contract_status_by_mode.get(mode, "PASS"),
                        "forced test contract failure" if contract_status_by_mode.get(mode) == "FAIL" else "ok",
                    )
                    for mode in modes
                ],
            )
            if "live_guarded" in modes:
                live_dir = round_dir / "live_guarded"
                live_dir.mkdir(parents=True, exist_ok=True)
                (live_dir / "daemon.log").write_text(
                    live_daemon_log(live_action_evidence),
                    encoding="utf-8",
                )
            if write_provenance:
                write_run_env(
                    round_dir / "run.env",
                    artifact_dir=round_dir,
                    modes=modes,
                    samples=samples,
                    live_confirm=live_confirm,
                    live_pid_allowlist=live_pid_allowlist,
                    overrides=run_env_overrides,
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
                "AEGISAI_PHASE4_ROUNDS": str(rounds),
                "AEGISAI_AB_SAMPLES": str(samples),
                "AEGISAI_AB_CONCURRENCY": "1",
                "AEGISAI_CONFIRM_LIVE_ACTUATOR": live_confirm,
                "AEGISAI_LIVE_PID_ALLOWLIST": live_pid_allowlist,
                "AEGISAI_ENABLE_LIVE_AFFINITY": "0",
                "AEGISAI_PHASE4_TUNED_VARIABLE": "stress_shape",
                "AEGISAI_PHASE4_TUNED_VARIABLE_DETAIL": "Changed CPU workers from 1 to 2; all other controls held constant.",
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
                live_action_evidence="effective_taskset",
            )

            report = (root / "mvp_benefit_report.md").read_text(encoding="utf-8")
            self.assertNotEqual(result.returncode, 0, result.stdout + result.stderr)
            self.assertIn("Result: `FAIL`", report)
            self.assertNotIn("Result: `PASS`", report)
            self.assertIn("Apparent improvements were limited to observation or dry-run modes", report)
            self.assertIn("Selected mode contracts: `PASS`", report)
            self.assertIn("Live effective host-level actuator changes: `3`", report)
            self.assertIn("Interference shape: `cpu_workers=2; io_workers=1; hdd_workers=1; hdd_bytes=128M`", report)
            self.assertIn("Changed variable: `stress_shape`", report)
            self.assertIn("Failure cause: `no_measurable_benefit`", report)
            aggregate = (root / "artifacts" / "phase4_aggregate.csv").read_text(encoding="utf-8")
            self.assertIn("changed_variable", aggregate)
            self.assertIn("stress_shape", aggregate)

    def test_live_action_count_zero_does_not_pass_even_with_live_trend(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            root = pathlib.Path(tmp)
            result = self.run_report(
                root,
                control_metric_ms=100.0,
                live_metric_ms=80.0,
                live_action_evidence="noop_taskset",
            )

            report = (root / "mvp_benefit_report.md").read_text(encoding="utf-8")
            self.assertNotEqual(result.returncode, 0, result.stdout + result.stderr)
            self.assertIn("Result: `FAIL`", report)
            self.assertNotIn("Result: `PASS`", report)
            self.assertIn("Live effective host-level actuator changes: `0`", report)
            self.assertIn("Live priority-limited/no-op nice applications: `0`", report)
            self.assertIn("no effective live actuator changes were observed", report)
            self.assertIn("Failure cause: `action_effectiveness`", report)

    def test_effective_live_action_with_failed_trend_does_not_pass(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            root = pathlib.Path(tmp)
            result = self.run_report(
                root,
                control_metric_ms=100.0,
                live_metric_ms=100.0,
                live_action_evidence="effective_taskset",
            )

            report = (root / "mvp_benefit_report.md").read_text(encoding="utf-8")
            self.assertNotEqual(result.returncode, 0, result.stdout + result.stderr)
            self.assertIn("Result: `FAIL`", report)
            self.assertNotIn("Result: `PASS`", report)
            self.assertIn("Live effective host-level actuator changes: `3`", report)
            self.assertIn("Failure cause: `no_measurable_benefit`", report)

    def test_priority_limited_actions_do_not_count_as_effective_live_actions(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            root = pathlib.Path(tmp)
            result = self.run_report(
                root,
                control_metric_ms=100.0,
                live_metric_ms=80.0,
                live_action_evidence="priority_limited",
            )

            report = (root / "mvp_benefit_report.md").read_text(encoding="utf-8")
            self.assertNotEqual(result.returncode, 0, result.stdout + result.stderr)
            self.assertIn("Result: `FAIL`", report)
            self.assertNotIn("Result: `PASS`", report)
            self.assertIn("Live effective host-level actuator changes: `0`", report)
            self.assertIn("Live priority-limited/no-op nice applications: `3`", report)
            self.assertIn("live actuator changes were priority-limited or no-op", report)
            self.assertIn("Failure cause: `action_effectiveness`", report)

    def test_live_trend_without_effective_live_action_does_not_pass(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            root = pathlib.Path(tmp)
            result = self.run_report(
                root,
                control_metric_ms=100.0,
                live_metric_ms=80.0,
                live_action_evidence="priority_limited",
            )

            report = (root / "mvp_benefit_report.md").read_text(encoding="utf-8")
            self.assertNotEqual(result.returncode, 0, result.stdout + result.stderr)
            self.assertIn("Result: `FAIL`", report)
            self.assertNotIn("Result: `PASS`", report)
            self.assertIn("no effective live actuator changes were observed", report)
            self.assertIn("Failure cause: `action_effectiveness`", report)

    def test_live_trend_with_effective_live_action_passes(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            root = pathlib.Path(tmp)
            result = self.run_report(
                root,
                control_metric_ms=100.0,
                live_metric_ms=80.0,
                live_action_evidence="effective_taskset",
            )

            report = (root / "mvp_benefit_report.md").read_text(encoding="utf-8")
            self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
            self.assertIn("Result: `PASS`", report)
            self.assertIn("stable improvement trend with effective host-level actuator changes", report)
            self.assertIn("Selected mode contracts: `PASS`", report)
            self.assertIn("Live effective host-level actuator changes: `3`", report)
            self.assertIn("Failure cause: `none`", report)
            self.assertIn("Evidence batch contract: `PASS`", report)
            self.assertIn("Live metadata contract: `PASS`", report)
            self.assertIn("Artifact provenance contract: `PASS`", report)

    def test_intermittent_live_improvement_is_classified_as_noisy_workload(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            root = pathlib.Path(tmp)
            result = self.run_report(
                root,
                control_metric_ms=100.0,
                live_metric_ms=100.0,
                live_action_evidence="effective_taskset",
                live_metrics_by_round=[120.0, 80.0, 120.0],
            )

            report = (root / "mvp_benefit_report.md").read_text(encoding="utf-8")
            self.assertNotEqual(result.returncode, 0, result.stdout + result.stderr)
            self.assertIn("Result: `FAIL`", report)
            self.assertIn("Failure cause: `noisy_workload`", report)

    def test_live_trend_with_too_few_samples_is_classified_as_insufficient_sample_size(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            root = pathlib.Path(tmp)
            result = self.run_report(
                root,
                control_metric_ms=100.0,
                live_metric_ms=80.0,
                live_action_evidence="effective_taskset",
                samples=1,
            )

            report = (root / "mvp_benefit_report.md").read_text(encoding="utf-8")
            self.assertNotEqual(result.returncode, 0, result.stdout + result.stderr)
            self.assertIn("Result: `FAIL`", report)
            self.assertIn("Failure cause: `insufficient_sample_size`", report)

    def test_missing_required_control_mode_cannot_pass(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            root = pathlib.Path(tmp)
            result = self.run_report(
                root,
                control_metric_ms=100.0,
                live_metric_ms=80.0,
                live_action_evidence="effective_taskset",
                modes=["baseline", "noop_observation", "live_guarded"],
            )

            report = (root / "mvp_benefit_report.md").read_text(encoding="utf-8")
            self.assertNotEqual(result.returncode, 0, result.stdout + result.stderr)
            self.assertIn("Result: `FAIL`", report)
            self.assertNotIn("Result: `PASS`", report)
            self.assertIn("Evidence batch contract: `FAIL`", report)
            self.assertIn("configured modes missing dry_run", report)
            self.assertIn("Failure cause: `insufficient_sample_size`", report)

    def test_missing_live_metadata_cannot_pass_cached_live_trend(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            root = pathlib.Path(tmp)
            result = self.run_report(
                root,
                control_metric_ms=100.0,
                live_metric_ms=80.0,
                live_action_evidence="effective_taskset",
                live_confirm="0",
                live_pid_allowlist="",
            )

            report = (root / "mvp_benefit_report.md").read_text(encoding="utf-8")
            self.assertNotEqual(result.returncode, 0, result.stdout + result.stderr)
            self.assertIn("Result: `FAIL`", report)
            self.assertNotIn("Result: `PASS`", report)
            self.assertIn("Live metadata contract: `FAIL`", report)
            self.assertIn("AEGISAI_CONFIRM_LIVE_ACTUATOR must be 1", report)
            self.assertIn("AEGISAI_LIVE_PID_ALLOWLIST must contain one or more positive PIDs", report)
            self.assertIn("Failure cause: `action_effectiveness`", report)

    def test_failed_mode_contract_cannot_pass_cached_live_trend(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            root = pathlib.Path(tmp)
            result = self.run_report(
                root,
                control_metric_ms=100.0,
                live_metric_ms=80.0,
                live_action_evidence="effective_taskset",
                contract_status_by_mode={"live_guarded": "FAIL"},
            )

            report = (root / "mvp_benefit_report.md").read_text(encoding="utf-8")
            self.assertNotEqual(result.returncode, 0, result.stdout + result.stderr)
            self.assertIn("Result: `FAIL`", report)
            self.assertNotIn("Result: `PASS`", report)
            self.assertIn("Mode contract rollup: `FAIL`", report)
            self.assertIn("mode contract FAIL: forced test contract failure", report)
            self.assertIn("Failure cause: `action_effectiveness`", report)

    def test_missing_run_env_cannot_pass_cached_live_trend(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            root = pathlib.Path(tmp)
            result = self.run_report(
                root,
                control_metric_ms=100.0,
                live_metric_ms=80.0,
                live_action_evidence="effective_taskset",
                write_provenance=False,
            )

            report = (root / "mvp_benefit_report.md").read_text(encoding="utf-8")
            self.assertNotEqual(result.returncode, 0, result.stdout + result.stderr)
            self.assertIn("Result: `FAIL`", report)
            self.assertIn("Artifact provenance contract: `FAIL`", report)
            self.assertIn("run.env missing", report)
            self.assertIn("Failure cause: `action_effectiveness`", report)

    def test_run_env_control_mismatch_cannot_pass_cached_live_trend(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            root = pathlib.Path(tmp)
            result = self.run_report(
                root,
                control_metric_ms=100.0,
                live_metric_ms=80.0,
                live_action_evidence="effective_taskset",
                run_env_overrides={"samples_per_mode": "9", "model": "different:model"},
            )

            report = (root / "mvp_benefit_report.md").read_text(encoding="utf-8")
            self.assertNotEqual(result.returncode, 0, result.stdout + result.stderr)
            self.assertIn("Result: `FAIL`", report)
            self.assertIn("Artifact provenance contract: `FAIL`", report)
            self.assertIn("run.env samples_per_mode=9, expected 3", report)
            self.assertIn("run.env model=different:model, expected qwen2.5:0.5b", report)
            self.assertIn("Failure cause: `action_effectiveness`", report)


if __name__ == "__main__":
    unittest.main()
