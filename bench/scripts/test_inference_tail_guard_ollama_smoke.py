#!/usr/bin/env python3
"""Regression tests for deterministic Ollama smoke provenance artifacts."""

from __future__ import annotations

import os
import pathlib
import subprocess
import tempfile
import unittest


REPO_ROOT = pathlib.Path(__file__).resolve().parents[2]
SMOKE_SCRIPT = REPO_ROOT / "bench" / "scripts" / "inference_tail_guard_ollama_smoke.sh"


def read_env(path: pathlib.Path) -> dict[str, str]:
    values: dict[str, str] = {}
    for line in path.read_text(encoding="utf-8").splitlines():
        key, _, value = line.partition("=")
        values[key] = value
    return values


class InferenceTailGuardOllamaSmokeTests(unittest.TestCase):
    def run_smoke(
        self,
        root: pathlib.Path,
        *,
        samples: str = "8",
        modes: str = "baseline,noop_observation,dry_run,live_guarded",
        live_confirm: str = "1",
        live_pid_allowlist: str = "1234",
    ) -> subprocess.CompletedProcess[str]:
        artifact_dir = root / "artifacts"
        env = os.environ.copy()
        env.update(
            {
                "AEGISAI_AB_RUN_ENV_ONLY": "1",
                "AEGISAI_AB_RUN_ID": "run_env_unit",
                "AEGISAI_AB_ARTIFACT_DIR": str(artifact_dir),
                "AEGISAI_VERIFY_LOG": str(root / "verification_log.md"),
                "AEGISAI_AB_MODES": modes,
                "AEGISAI_AB_SAMPLES": samples,
                "AEGISAI_AB_CONCURRENCY": "2",
                "AEGISAI_OLLAMA_MODEL": "unit:model",
                "AEGISAI_OLLAMA_PROMPT": "unit prompt",
                "AEGISAI_OLLAMA_NUM_PREDICT": "32",
                "AEGISAI_STRESS_CPU": "3",
                "AEGISAI_STRESS_IO": "1",
                "AEGISAI_STRESS_HDD": "1",
                "AEGISAI_STRESS_HDD_BYTES": "64M",
                "AEGISAI_STRESS_TIMEOUT": "9",
                "AEGISAI_CONFIRM_LIVE_ACTUATOR": live_confirm,
                "AEGISAI_LIVE_PID_ALLOWLIST": live_pid_allowlist,
                "AEGISAI_ENABLE_LIVE_AFFINITY": "1",
            }
        )
        return subprocess.run(
            ["bash", str(SMOKE_SCRIPT)],
            cwd=REPO_ROOT,
            env=env,
            text=True,
            capture_output=True,
            check=False,
        )

    def test_run_env_only_writes_provenance_without_live_workload(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            root = pathlib.Path(tmp)
            result = self.run_smoke(root)

            self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
            artifact_dir = root / "artifacts"
            run_env = read_env(artifact_dir / "run.env")
            acceptance_baseline = read_env(artifact_dir / "acceptance_baseline.env")
            log = (root / "verification_log.md").read_text(encoding="utf-8")

            self.assertEqual(run_env["run_id"], "run_env_unit")
            self.assertEqual(run_env["run_env_only"], "1")
            self.assertEqual(run_env["modes"], "baseline noop_observation dry_run live_guarded")
            self.assertEqual(run_env["model"], "unit:model")
            self.assertEqual(run_env["prompt"], "unit prompt")
            self.assertEqual(run_env["num_predict"], "32")
            self.assertEqual(run_env["samples_per_mode"], "8")
            self.assertEqual(run_env["concurrency"], "2")
            self.assertEqual(run_env["stress_cpu"], "3")
            self.assertEqual(run_env["stress_io"], "1")
            self.assertEqual(run_env["stress_hdd"], "1")
            self.assertEqual(run_env["stress_hdd_bytes"], "64M")
            self.assertEqual(run_env["stress_timeout_s"], "9")
            self.assertIn("--cpu 3", run_env["stress_command"])
            self.assertIn("--io 1", run_env["stress_command"])
            self.assertIn("--hdd 1", run_env["stress_command"])
            self.assertEqual(run_env["live_confirm"], "1")
            self.assertEqual(run_env["live_pid_allowlist"], "1234")
            self.assertEqual(run_env["live_enable_affinity"], "1")
            self.assertEqual(run_env["live_scope"], "nice,affinity")
            self.assertEqual(run_env["artifact_dir"], str(artifact_dir))
            self.assertEqual(run_env["acceptance_baseline"], str(artifact_dir / "acceptance_baseline.env"))
            self.assertEqual(run_env["cpu_topology_artifact"], str(artifact_dir / "cpu_topology.txt"))
            self.assertEqual(run_env["permission_state_artifact"], str(artifact_dir / "permission_state.txt"))
            self.assertEqual(run_env["mode_contract_csv"], str(artifact_dir / "mode_contract.csv"))

            self.assertEqual(acceptance_baseline["run_id"], "run_env_unit")
            self.assertEqual(acceptance_baseline["acceptance_goal"], "fixed_controls_and_separate_mode_contracts")
            self.assertEqual(acceptance_baseline["model"], "unit:model")
            self.assertEqual(acceptance_baseline["samples_per_mode"], "8")
            self.assertEqual(acceptance_baseline["live_affinity_enabled"], "1")
            self.assertTrue((artifact_dir / "payload.stream.json").is_file())
            self.assertTrue((artifact_dir / "payload.warmup.json").is_file())
            self.assertIn("- Live workload: `not_run`", log)
            self.assertIn("- Result: `RUN_ENV_ONLY`", log)
            self.assertNotIn("Overall result: `PASS`", log)

    def test_invalid_config_does_not_write_misleading_pass_artifacts(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            root = pathlib.Path(tmp)
            result = self.run_smoke(root, samples="3", modes="baseline,dry_run")

            self.assertNotEqual(result.returncode, 0, result.stdout + result.stderr)
            log = (root / "verification_log.md").read_text(encoding="utf-8")
            self.assertIn("- Invalid sample count: `3`", log)
            self.assertNotIn("`PASS`", log)
            self.assertFalse((root / "artifacts" / "run.env").exists())
            self.assertFalse((root / "artifacts" / "acceptance_baseline.env").exists())
            self.assertFalse((root / "artifacts" / "mode_contract.csv").exists())


if __name__ == "__main__":
    unittest.main()
