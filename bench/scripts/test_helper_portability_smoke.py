#!/usr/bin/env python3
"""Regression tests for helper portability smoke bucket classification."""

from __future__ import annotations

import os
import pathlib
import json
import stat
import subprocess
import tempfile
import textwrap
import unittest


REPO_ROOT = pathlib.Path(__file__).resolve().parents[2]
SMOKE_SCRIPT = REPO_ROOT / "bench" / "scripts" / "helper_portability_smoke.sh"


def write_executable(path: pathlib.Path, contents: str) -> None:
    path.write_text(textwrap.dedent(contents).lstrip(), encoding="utf-8")
    path.chmod(path.stat().st_mode | stat.S_IXUSR)


class HelperPortabilitySmokeTests(unittest.TestCase):
    def run_smoke(
        self,
        root: pathlib.Path,
        *,
        helper_status: str,
        daemon_status: str = "compatible",
        raw_events: str = "1",
        normalized_events: str = "1",
    ) -> subprocess.CompletedProcess[str]:
        artifact_dir = root / "artifacts"
        helper = root / "fake-helper"
        daemon = root / "fake-daemon"

        write_executable(
            helper,
            f"""
            #!/usr/bin/env bash
            set -eu
            if [[ "${{1:-}}" == "compatibility" ]]; then
              printf 'helper compatibility: status={helper_status}; kernel=6.8.0-test; bpftrace=fake-bpftrace; tracefs=/sys/kernel/tracing; requested_probes=tracepoint:sched:sched_switch; required_fields=tracepoint:sched:sched_switch:prev_state\\n'
              exit 0
            fi
            if [[ "${{1:-}}" == "stream" ]]; then
              signal="offcpu_time"
              for arg in "$@"; do
                if [[ "${{arg}}" == "--io" ]]; then
                  signal="io_latency"
                fi
              done
              for _ in $(seq 1 {raw_events}); do
                printf 'aegisai_probe signal=%s pid=1 tid=1 value=1 timestamp_ns=1\\n' "${{signal}}"
              done
              exit 0
            fi
            exit 2
            """,
        )
        write_executable(
            daemon,
            f"""
            #!/usr/bin/env bash
            set -eu
            signal="offcpu_time"
            previous=""
            for arg in "$@"; do
              if [[ "${{previous}}" == "--verification-log" ]]; then
                mkdir -p "$(dirname -- "${{arg}}")"
                printf 'fake daemon verification\\n' >"${{arg}}"
              fi
              previous="${{arg}}"
            done
            for arg in "$@"; do
              if [[ "${{arg}}" == *"daemon_io_latency"* ]]; then
                signal="io_latency"
              fi
            done
            printf 'processed_events: {normalized_events}\\n'
            printf '%s: events={normalized_events} total={normalized_events} max=1\\n' "${{signal}}"
            printf '  helper compatibility: status={daemon_status}; kernel=6.8.0-test; bpftrace=fake-bpftrace; tracefs=/sys/kernel/tracing; requested_probes=tracepoint:sched:sched_switch; required_fields=tracepoint:sched:sched_switch:prev_state\\n'
            exit 0
            """,
        )

        env = os.environ.copy()
        env.update(
            {
                "AEGISAI_EBPF_HELPER": str(helper),
                "AEGISAI_RUNTIME_DAEMON": str(daemon),
                "AEGISAI_BPFTRACE": "/tmp/aegisai-unit-missing-bpftrace",
                "AEGISAI_HELPER_PORTABILITY_RUN_ID": "unit",
                "AEGISAI_HELPER_PORTABILITY_ARTIFACT_DIR": str(artifact_dir),
                "AEGISAI_VERIFY_LOG": str(artifact_dir / "helper_portability.md"),
                "AEGISAI_HELPER_PORTABILITY_RAW_OFFCPU_SECONDS": "1",
                "AEGISAI_HELPER_PORTABILITY_RAW_IO_SECONDS": "1",
                "AEGISAI_HELPER_PORTABILITY_DAEMON_TIMEOUT_SECONDS": "1",
                "AEGISAI_HELPER_PORTABILITY_DAEMON_MAX_EVENTS": "1",
                "AEGISAI_HELPER_PORTABILITY_DAEMON_POLL_TIMEOUT_MS": "1",
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

    def test_helper_unavailable_is_not_reported_as_no_workload_events(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            root = pathlib.Path(tmp)
            result = self.run_smoke(root, helper_status="helper unavailable")

            self.assertNotEqual(result.returncode, 0, result.stdout + result.stderr)
            self.assertIn("helper_portability_smoke=helper unavailable", result.stdout)
            log = (root / "artifacts" / "helper_portability.md").read_text(encoding="utf-8")
            self.assertIn("- Final bucket: `helper unavailable`", log)
            self.assertIn("- Phase 5 helper-backed signals: `excluded`", log)
            self.assertIn("- Overall result: `FAIL`", log)
            self.assertNotIn("- Final bucket: `no workload events`", log)
            self.assertNotIn("- Overall result: `PASS`", log)
            availability = json.loads(
                (root / "artifacts" / "helper_signal_availability.json").read_text(encoding="utf-8")
            )
            self.assertEqual(availability["bucket"], "helper unavailable")
            self.assertEqual(
                availability["phase5_helper_backed_signals"]["offcpu_time"]["phase5_planning_status"],
                "excluded",
            )

    def test_tracepoint_incompatible_is_not_reported_as_no_workload_events(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            root = pathlib.Path(tmp)
            result = self.run_smoke(root, helper_status="tracepoint incompatible")

            self.assertNotEqual(result.returncode, 0, result.stdout + result.stderr)
            self.assertIn("helper_portability_smoke=tracepoint incompatible", result.stdout)
            log = (root / "artifacts" / "helper_portability.md").read_text(encoding="utf-8")
            self.assertIn("- Final bucket: `tracepoint incompatible`", log)
            self.assertIn("- Phase 5 helper-backed signals: `excluded`", log)
            self.assertIn("- Overall result: `FAIL`", log)
            self.assertNotIn("- Final bucket: `no workload events`", log)
            self.assertNotIn("- Overall result: `PASS`", log)

    def test_no_workload_events_requires_compatible_diagnostics(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            root = pathlib.Path(tmp)
            result = self.run_smoke(
                root,
                helper_status="compatible",
                daemon_status="compatible",
                raw_events="0",
                normalized_events="0",
            )

            self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
            self.assertIn("helper_portability_smoke=no workload events", result.stdout)
            log = (root / "artifacts" / "helper_portability.md").read_text(encoding="utf-8")
            self.assertIn("- Final bucket: `no workload events`", log)
            self.assertIn("- Phase 5 helper-backed signals: `excluded`", log)
            self.assertIn("- Overall result: `PASS`", log)
            availability_csv = (
                root / "artifacts" / "helper_signal_availability.csv"
            ).read_text(encoding="utf-8")
            self.assertIn("offcpu_time,excluded,0,0", availability_csv)
            self.assertIn("io_latency,excluded,0,0", availability_csv)

    def test_daemon_compatibility_failure_is_authoritative(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            root = pathlib.Path(tmp)
            result = self.run_smoke(
                root,
                helper_status="compatible",
                daemon_status="helper unavailable",
            )

            self.assertNotEqual(result.returncode, 0, result.stdout + result.stderr)
            self.assertIn("helper_portability_smoke=helper unavailable", result.stdout)
            log = (root / "artifacts" / "helper_portability.md").read_text(encoding="utf-8")
            self.assertIn("- Final bucket: `helper unavailable`", log)
            self.assertNotIn("- Final bucket: `no workload events`", log)
            self.assertNotIn("- Overall result: `PASS`", log)

    def test_validated_signal_marks_phase5_signals_included(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            root = pathlib.Path(tmp)
            result = self.run_smoke(
                root,
                helper_status="compatible",
                daemon_status="compatible",
                raw_events="2",
                normalized_events="3",
            )

            self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
            self.assertIn("helper_portability_smoke=validated signal", result.stdout)
            log = (root / "artifacts" / "helper_portability.md").read_text(encoding="utf-8")
            self.assertIn("- Phase 5 helper-backed signals: `included`", log)
            availability = json.loads(
                (root / "artifacts" / "helper_signal_availability.json").read_text(encoding="utf-8")
            )
            self.assertEqual(availability["bucket"], "validated signal")
            for signal in ("offcpu_time", "io_latency"):
                self.assertEqual(
                    availability["phase5_helper_backed_signals"][signal]["phase5_planning_status"],
                    "included",
                )


if __name__ == "__main__":
    unittest.main()
