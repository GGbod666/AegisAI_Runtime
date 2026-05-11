#!/usr/bin/env python3
"""Unit tests for the Tool Call Booster A/B report summarizer."""

from __future__ import annotations

import importlib.util
import pathlib
import tempfile
import unittest


MODULE_PATH = pathlib.Path(__file__).with_name("summarize_ab.py")
SPEC = importlib.util.spec_from_file_location("summarize_ab", MODULE_PATH)
assert SPEC is not None
summarize_ab = importlib.util.module_from_spec(SPEC)
assert SPEC.loader is not None
SPEC.loader.exec_module(summarize_ab)


def detail_row(round_no: int, mode: str, latency_ms: float) -> dict[str, str]:
    is_baseline = mode == "baseline"
    return {
        "round": str(round_no),
        "mode": mode,
        "backend": "none" if is_baseline else mode,
        "contract": "PASS",
        "tool_call_id": f"tc-r{round_no}-{mode}",
        "tool_call_latency_ms": f"{latency_ms:.3f}",
        "executor_ms": f"{latency_ms:.3f}",
        "retrieval_ms": "",
        "rerank_ms": "",
        "background_ms": "",
        "daemon_lifecycle_ms": "" if is_baseline else "100.000",
        "processed_events": "0" if is_baseline else "10",
        "applied_actions": "0" if is_baseline else "3",
        "total_rollbacks": "0" if is_baseline else "3",
        "tool_call_booster_triggers": "0" if is_baseline else "3",
        "executor_roles": "4",
        "stages": "none" if is_baseline else "executor:1,retrieval:1,rerank:1",
        "action_error_count": "0",
        "artifact_prefix": f"round{round_no}.{mode}",
        "contract_reason": "ok",
    }


class SummarizeAbTests(unittest.TestCase):
    def test_detail_latency_uses_critical_chain_not_background(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            artifact_dir = pathlib.Path(tmp)
            (artifact_dir / "executor.round1.baseline.stdout").write_text(
                "\n".join(
                    [
                        '{"duration_ms": 150.0, "role": "retrieval-worker", "tool_call_id": "tc-001"}',
                        '{"duration_ms": 120.0, "role": "rerank-worker", "tool_call_id": "tc-001"}',
                        '{"duration_ms": 170.0, "role": "background-worker", "tool_call_id": "tc-001"}',
                        '{"child_statuses": [0, 0, 0], "duration_ms": 100.0, "role": "tool-executor", "tool_call_id": "tc-001"}',
                    ]
                ),
                encoding="utf-8",
            )

            rows = summarize_ab.build_detail_rows(artifact_dir, ["baseline"], rounds=1)

        self.assertEqual(rows[0]["contract"], "PASS")
        self.assertEqual(rows[0]["tool_call_latency_ms"], "150.000")

    def test_contract_requires_critical_stage_latencies(self) -> None:
        executor = {
            "durations": {"executor": 100.0, "rerank": 95.0},
            "role_count": 4,
            "child_status_ok": True,
        }
        daemon = {
            "processed_events": 10,
            "applied_actions": 3,
            "tool_call_booster_triggers": 3,
            "total_rollbacks": 3,
            "stages": "executor:1,retrieval:1,rerank:1",
            "action_error_count": 0,
        }

        reasons = summarize_ab.contract_reasons("live_guarded", executor, daemon)

        self.assertIn("missing_retrieval_latency", reasons)

    def test_only_guarded_repeated_improvement_counts_as_benefit(self) -> None:
        rows: list[dict[str, str]] = []
        for round_no in range(1, 4):
            rows.append(detail_row(round_no, "baseline", 100.0))
        for round_no, latency in enumerate((90.0, 95.0, 96.0), start=1):
            rows.append(detail_row(round_no, "noop", latency))
        for round_no, latency in enumerate((90.0, 95.0, 96.0), start=1):
            rows.append(detail_row(round_no, "live_guarded", latency))

        summary_rows = summarize_ab.build_summary_rows(
            rows,
            ["baseline", "noop", "live_guarded"],
            rounds=3,
            min_benefit_pct=5.0,
        )
        by_mode = {row["mode"]: row for row in summary_rows}

        self.assertEqual(by_mode["noop"]["latency_trend_verdict"], "PASS")
        self.assertEqual(by_mode["noop"]["benefit_verdict"], "FAIL")
        self.assertIn("control mode only", by_mode["noop"]["verdict_reason"])
        self.assertEqual(by_mode["live_guarded"]["latency_trend_verdict"], "PASS")
        self.assertEqual(by_mode["live_guarded"]["benefit_verdict"], "PASS")
        self.assertEqual(by_mode["live_guarded"]["improved_rounds"], "2")
        self.assertEqual(by_mode["live_guarded"]["comparable_rounds"], "3")
        self.assertIn(
            "executor warmup is plan/audit-only",
            by_mode["live_guarded"]["verdict_reason"],
        )

    def test_report_states_warmup_executor_boundary(self) -> None:
        rows: list[dict[str, str]] = []
        for round_no in range(1, 4):
            rows.append(detail_row(round_no, "baseline", 100.0))
            rows.append(detail_row(round_no, "live_guarded", 90.0))

        summary_rows = summarize_ab.build_summary_rows(
            rows,
            ["baseline", "live_guarded"],
            rounds=3,
            min_benefit_pct=5.0,
        )

        with tempfile.TemporaryDirectory() as tmp:
            report_path = pathlib.Path(tmp) / "report.md"
            summarize_ab.write_report(
                report_path,
                "test-run",
                pathlib.Path(tmp),
                ["baseline", "live_guarded"],
                rounds=3,
                min_benefit_pct=5.0,
                detail_rows=rows,
                summary_rows=summary_rows,
            )

            report = report_path.read_text(encoding="utf-8")

        self.assertIn("Benefit scope: guarded scheduler actions only", report)
        self.assertIn("`WarmupExecutor` is plan/audit-only", report)
        self.assertIn("No live executor/cache warmup side effect is implemented", report)


if __name__ == "__main__":
    unittest.main()
