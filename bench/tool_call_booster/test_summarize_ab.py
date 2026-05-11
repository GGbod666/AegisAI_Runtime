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
    is_live_guarded = mode == "live_guarded"
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
        "scheduler_command_count": "0" if is_baseline else "3",
        "effective_scheduler_action_count": "3" if is_live_guarded else "0",
        "stage_effective_scheduler_actions": "executor:1,retrieval:1,rerank:1"
        if is_live_guarded
        else "none",
        "warmup_side_effect_count": "0",
        "warmup_deferred_count": "0" if is_baseline else "3",
        "warmup_rollback_noop_count": "0" if is_baseline else "3",
        "guarded_noop_count": "0",
        "live_guard_scope": "affinity,nice" if is_live_guarded else "none",
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

    def test_contract_accepts_stage_action_attribution_as_stage_coverage(self) -> None:
        executor = {
            "durations": {"executor": 100.0, "retrieval": 95.0, "rerank": 90.0},
            "role_count": 4,
            "child_status_ok": True,
        }
        daemon = {
            "processed_events": 10,
            "applied_actions": 3,
            "tool_call_booster_triggers": 3,
            "total_rollbacks": 3,
            "stages": "retrieval:1,rerank:1",
            "stage_effective_scheduler_actions": {"executor": 1},
            "action_error_count": 0,
        }

        reasons = summarize_ab.contract_reasons("live_guarded", executor, daemon)

        self.assertNotIn("missing_executor_stage", reasons)

    def test_live_guard_noop_actions_are_not_counted_as_effective(self) -> None:
        daemon_text = "\n".join(
            [
                "actuator_backend: linux-command",
                "audit_highlights:",
                "  pid=42;scenario=tool_call_booster;backend.apply.live_guard.scope=nice",
                "  pid=42;scenario=tool_call_booster;backend.apply.apply.0.detail=runner=system-command-runner;command=renice 0 -p 42;output=42 (process ID) old priority 0, new priority 0;priority_raise_limited=true;requested_nice=-3;applied_nice=0",
                "  pid=42;scenario=tool_call_booster;backend.apply.apply.1.detail=affinity command disabled by live guard",
                "  pid=42;scenario=tool_call_booster;backend.apply.apply.result=ok",
            ]
        )

        effects = summarize_ab.parse_action_effects(daemon_text, "linux-command")

        self.assertEqual(effects["scheduler_command_count"], 1)
        self.assertEqual(effects["effective_scheduler_action_count"], 0)
        self.assertEqual(effects["warmup_side_effect_count"], 0)
        self.assertEqual(effects["guarded_noop_count"], 2)
        self.assertEqual(effects["live_guard_scope"], "nice")

    def test_live_guard_taskset_counts_as_effective_scheduler_action(self) -> None:
        daemon_text = "\n".join(
            [
                "actuator_backend: linux-command",
                "audit_highlights:",
                "  pid=42;scenario=tool_call_booster;tool_call_stage=retrieval",
                "  pid=42;scenario=tool_call_booster;backend.apply.live_guard.scope=nice,affinity",
                "  pid=42;scenario=tool_call_booster;backend.apply.apply.0.detail=runner=system-command-runner;command=taskset -pc 0,1 42;output=pid 42's current affinity list: 0-7\npid 42's new affinity list: 0,1",
            ]
        )

        effects = summarize_ab.parse_action_effects(daemon_text, "linux-command")

        self.assertEqual(effects["scheduler_command_count"], 1)
        self.assertEqual(effects["effective_scheduler_action_count"], 1)
        self.assertEqual(effects["stage_effective_scheduler_actions"], {"retrieval": 1})
        self.assertEqual(effects["guarded_noop_count"], 0)

    def test_apply_detail_attribution_drives_stage_effectiveness(self) -> None:
        daemon_text = "\n".join(
            [
                "actuator_backend: linux-command",
                "audit_highlights:",
                "  pid=42;scenario=tool_call_booster;backend.apply.apply.0.detail=tool_call_stage=rerank;tool_call_id=tc-001;action_kind=set_affinity;effective=true;runner=system-command-runner;command=taskset -pc 0,1 42;output=pid 42's current affinity list: 0-7",
                "pid 42's new affinity list: 0,1",
            ]
        )

        effects = summarize_ab.parse_action_effects(daemon_text, "linux-command")

        self.assertEqual(effects["scheduler_command_count"], 1)
        self.assertEqual(effects["effective_scheduler_action_count"], 1)
        self.assertEqual(effects["stage_effective_scheduler_actions"], {"rerank": 1})

    def test_warmup_side_effect_counts_separately_from_scheduler_actions(self) -> None:
        daemon_text = "\n".join(
            [
                "actuator_backend: linux-command",
                "audit_highlights:",
                "  pid=42;scenario=tool_call_booster;backend.apply.apply.0.detail=runner=system-warmup-runner;warmup executor applied;side_effect=command;elapsed_ms=7;command=prime-cache",
                "  pid=42;scenario=tool_call_booster;backend.rollback.rollback.0.detail=warmup rollback noop",
            ]
        )

        effects = summarize_ab.parse_action_effects(daemon_text, "linux-command")

        self.assertEqual(effects["scheduler_command_count"], 0)
        self.assertEqual(effects["effective_scheduler_action_count"], 0)
        self.assertEqual(effects["warmup_side_effect_count"], 1)
        self.assertEqual(effects["warmup_deferred_count"], 0)
        self.assertEqual(effects["warmup_rollback_noop_count"], 1)

    def test_short_lived_tool_call_rollback_no_such_process_is_benign(self) -> None:
        daemon_text = "\n".join(
            [
                "audit_highlights:",
                "  pid=42;scenario=tool_call_booster;backend.rollback.rollback.0.status=error",
                "  pid=42;scenario=tool_call_booster;backend.rollback.rollback.0.error=runner=system-command-runner;command=renice 0 -p 42;error=renice: failed to get priority for 42 (process ID): No such process",
                "  pid=42;scenario=tool_call_booster;backend.rollback.rollback.1.status=error",
                "  pid=42;scenario=tool_call_booster;backend.rollback.rollback.1.error=runner=system-command-runner;command=taskset -pc 0,1,2,3 42;error=taskset: failed to get pid 42's affinity: No such process",
            ]
        )

        self.assertEqual(summarize_ab.count_action_errors(daemon_text), 0)

    def test_stage_effectiveness_correlates_stage_actions_and_latency_delta(self) -> None:
        rows: list[dict[str, str]] = []
        for round_no in range(1, 4):
            baseline = detail_row(round_no, "baseline", 100.0)
            baseline["retrieval_ms"] = "100.000"
            baseline["rerank_ms"] = "100.000"
            rows.append(baseline)

            guarded = detail_row(round_no, "live_guarded", 90.0)
            guarded["retrieval_ms"] = "90.000"
            guarded["rerank_ms"] = "102.000"
            guarded["stage_effective_scheduler_actions"] = "retrieval:1"
            rows.append(guarded)

        stage_rows = summarize_ab.build_stage_effectiveness_rows(
            rows,
            ["baseline", "live_guarded"],
            rounds=3,
            min_benefit_pct=5.0,
        )
        by_mode_stage = {(row["mode"], row["stage"]): row for row in stage_rows}

        retrieval = by_mode_stage[("live_guarded", "retrieval")]
        self.assertEqual(retrieval["effective_scheduler_action_count_total"], "3")
        self.assertEqual(retrieval["improved_rounds"], "3")
        self.assertEqual(retrieval["avg_delta_vs_baseline_pct"], "-10.000")
        self.assertEqual(retrieval["stage_effectiveness"], "PASS")

        rerank = by_mode_stage[("live_guarded", "rerank")]
        self.assertEqual(rerank["effective_scheduler_action_count_total"], "0")
        self.assertEqual(rerank["stage_effectiveness"], "NO_EFFECTIVE_ACTION")

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
            stage_effectiveness_rows=summarize_ab.build_stage_effectiveness_rows(
                rows,
                ["baseline", "noop", "live_guarded"],
                rounds=3,
                min_benefit_pct=5.0,
            ),
        )
        by_mode = {row["mode"]: row for row in summary_rows}

        self.assertEqual(by_mode["noop"]["latency_trend_verdict"], "PASS")
        self.assertEqual(by_mode["noop"]["benefit_verdict"], "FAIL")
        self.assertIn("control mode only", by_mode["noop"]["verdict_reason"])
        self.assertEqual(by_mode["live_guarded"]["latency_trend_verdict"], "PASS")
        self.assertEqual(by_mode["live_guarded"]["benefit_verdict"], "PASS")
        self.assertEqual(by_mode["live_guarded"]["improved_rounds"], "2")
        self.assertEqual(by_mode["live_guarded"]["comparable_rounds"], "3")
        self.assertEqual(
            by_mode["live_guarded"]["effective_scheduler_action_count_total"], "9"
        )
        self.assertIn(
            "executor warmup is reported separately",
            by_mode["live_guarded"]["verdict_reason"],
        )

    def test_guarded_benefit_requires_stage_effectiveness_pass(self) -> None:
        rows: list[dict[str, str]] = []
        for round_no in range(1, 4):
            rows.append(detail_row(round_no, "baseline", 100.0))
            row = detail_row(round_no, "live_guarded", 90.0)
            row["stage_effective_scheduler_actions"] = "retrieval:1"
            rows.append(row)

        stage_rows = summarize_ab.build_stage_effectiveness_rows(
            rows,
            ["baseline", "live_guarded"],
            rounds=3,
            min_benefit_pct=5.0,
        )
        summary_rows = summarize_ab.build_summary_rows(
            rows,
            ["baseline", "live_guarded"],
            rounds=3,
            min_benefit_pct=5.0,
            stage_effectiveness_rows=stage_rows,
        )
        by_mode = {row["mode"]: row for row in summary_rows}
        by_mode_stage = {(row["mode"], row["stage"]): row for row in stage_rows}

        self.assertEqual(
            by_mode_stage[("live_guarded", "retrieval")]["stage_effectiveness"],
            "LATENCY_NOT_IMPROVED",
        )
        self.assertEqual(by_mode["live_guarded"]["latency_trend_verdict"], "PASS")
        self.assertEqual(by_mode["live_guarded"]["benefit_verdict"], "FAIL")
        self.assertIn(
            "no stage effectiveness PASS",
            by_mode["live_guarded"]["verdict_reason"],
        )

    def test_guarded_latency_improvement_without_effective_action_fails_benefit(self) -> None:
        rows: list[dict[str, str]] = []
        for round_no in range(1, 4):
            rows.append(detail_row(round_no, "baseline", 100.0))
            row = detail_row(round_no, "live_guarded", 90.0)
            row["effective_scheduler_action_count"] = "0"
            row["guarded_noop_count"] = "3"
            rows.append(row)

        summary_rows = summarize_ab.build_summary_rows(
            rows,
            ["baseline", "live_guarded"],
            rounds=3,
            min_benefit_pct=5.0,
        )
        by_mode = {row["mode"]: row for row in summary_rows}

        self.assertEqual(by_mode["live_guarded"]["latency_trend_verdict"], "PASS")
        self.assertEqual(by_mode["live_guarded"]["benefit_verdict"], "FAIL")
        self.assertIn(
            "no effective scheduler action",
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
                stage_effectiveness_rows=summarize_ab.build_stage_effectiveness_rows(
                    rows,
                    ["baseline", "live_guarded"],
                    rounds=3,
                    min_benefit_pct=5.0,
                ),
            )

            report = report_path.read_text(encoding="utf-8")

        self.assertIn("Benefit scope: guarded scheduler actions only", report)
        self.assertIn("## Stage Effectiveness", report)
        self.assertIn("`WarmupExecutor` defaults to deferred/no-side-effect audit", report)
        self.assertIn("warmup counts are reported separately", report)


if __name__ == "__main__":
    unittest.main()
