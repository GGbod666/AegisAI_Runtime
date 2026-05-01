#![forbid(unsafe_code)]

//! Offline explain and tuning support for the AegisAI Runtime control loop.
//!
//! This crate consumes metric snapshots, trace trails, and scenario policies to
//! produce:
//! - experiment-level reports
//! - per-trigger explanations
//! - conservative tuning suggestions for thresholds and action parameters

mod config;
mod engine;
mod model;

pub use config::{ExplainTuneConfig, ExplainTuneConfigError};
pub use engine::ExplainTuneEngine;
pub use model::{
    ExperimentReport, MetricInsight, MetricSummary, ScenarioReport, ToolCallChainReport,
    TraceEvidence, TriggerExplanation, TuneDirection, TuneSuggestion,
};

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use aegisai_metrics::{
        Measurement, MetricKind, MetricTrace, MetricsConfig, MetricsRecorder, RecordInput,
        SideEffect, TraceKind,
    };
    use aegisai_policy_engine::{
        PinStrategy, ScenarioActions, ScenarioKind, ScenarioPolicy, TriggerThresholds,
    };

    use crate::{ExplainTuneConfig, ExplainTuneConfigError, ExplainTuneEngine, TuneDirection};

    #[test]
    fn rejects_invalid_config() {
        let config = ExplainTuneConfig {
            max_trigger_explanations: 0,
            ..ExplainTuneConfig::default()
        };

        assert!(matches!(
            ExplainTuneEngine::new(config),
            Err(ExplainTuneConfigError::InvalidMaxTriggerExplanations)
        ));
    }

    #[test]
    fn builds_reports_and_trigger_explanations() {
        let mut recorder = MetricsRecorder::new(
            MetricsConfig::default()
                .with_tracked_metrics(vec![MetricKind::Ttft, MetricKind::P99Latency]),
        )
        .expect("valid recorder");

        recorder.record(
            RecordInput::new(1_000, 42, "ollama")
                .with_evaluated_scenarios(["inference_tail_guard"])
                .with_measurements([
                    Measurement::new(MetricKind::Ttft, 120.0),
                    Measurement::new(MetricKind::P99Latency, 220.0),
                ]),
        );

        recorder.record(
            RecordInput::new(2_000, 42, "ollama")
                .with_evaluated_scenarios(["inference_tail_guard"])
                .with_triggered_scenarios(["inference_tail_guard"])
                .with_action_count(2)
                .with_measurements([
                    Measurement::new(MetricKind::Ttft, 84.0),
                    Measurement::new(MetricKind::P99Latency, 180.0),
                ])
                .with_traces([MetricTrace::new(
                    2_000,
                    42,
                    "ollama",
                    TraceKind::ActionApplied,
                    "applied 2 action(s)",
                )
                .with_scenario("inference_tail_guard")
                .with_field(
                    "actions",
                    "raise_nice:-5,set_affinity:prefer_reserved_cores:0.5",
                )])
                .with_notes(["inference_tail_guard:run_queue_delay_us:2500>=2000".to_string()]),
        );

        let engine = ExplainTuneEngine::default();
        let report = engine.analyze(recorder.records(), recorder.traces(), &sample_policies());

        let scenario = report
            .scenario("inference_tail_guard")
            .expect("scenario report should exist");
        assert_eq!(scenario.evaluations, 2);
        assert_eq!(scenario.triggers, 1);
        assert!(scenario.trigger_rate > 0.4 && scenario.trigger_rate < 0.6);

        let ttft = scenario
            .metric_summaries
            .get(&MetricKind::Ttft)
            .expect("ttft summary");
        assert_eq!(ttft.improved_samples, 1);
        assert_eq!(ttft.regressed_samples, 0);
        assert_eq!(report.trigger_explanations.len(), 1);
        assert!(report.trigger_explanations[0]
            .summary
            .contains("run_queue_delay_us:2500>=2000"));
        assert!(report
            .findings
            .iter()
            .any(|finding| finding.contains("ttft")));
    }

    #[test]
    fn suggests_relaxing_noisy_policy() {
        let mut recorder = MetricsRecorder::new(
            MetricsConfig::default().with_tracked_metrics(vec![MetricKind::Ttft]),
        )
        .expect("valid recorder");

        recorder.record(
            RecordInput::new(1_000, 7, "python")
                .with_evaluated_scenarios(["tool_call_booster"])
                .with_measurements([Measurement::new(MetricKind::Ttft, 100.0)]),
        );

        recorder.record(
            RecordInput::new(2_000, 7, "python")
                .with_evaluated_scenarios(["tool_call_booster"])
                .with_triggered_scenarios(["tool_call_booster"])
                .with_action_count(1)
                .with_measurements([Measurement::new(MetricKind::Ttft, 108.0)])
                .with_side_effects([SideEffect::CpuContention])
                .with_traces([MetricTrace::new(
                    2_000,
                    7,
                    "python",
                    TraceKind::ActionRolledBack,
                    "rollback",
                )
                .with_scenario("tool_call_booster")
                .with_field("reason", "lease_expired")]),
        );

        let engine = ExplainTuneEngine::default();
        let report = engine.analyze(recorder.records(), recorder.traces(), &sample_policies());

        assert!(report.tune_suggestions.iter().any(|suggestion| {
            suggestion.scenario == "tool_call_booster"
                && suggestion.target == "policy.max_boost_duration_ms"
                && suggestion.direction == TuneDirection::Decrease
        }));
    }

    #[test]
    fn suggests_tightening_conservative_policy_when_regressions_go_unhandled() {
        let mut recorder = MetricsRecorder::new(
            MetricsConfig::default().with_tracked_metrics(vec![MetricKind::P99Latency]),
        )
        .expect("valid recorder");

        recorder.record(
            RecordInput::new(1_000, 9, "ollama")
                .with_evaluated_scenarios(["inference_tail_guard"])
                .with_measurements([Measurement::new(MetricKind::P99Latency, 200.0)]),
        );

        recorder.record(
            RecordInput::new(2_000, 9, "ollama")
                .with_evaluated_scenarios(["inference_tail_guard"])
                .with_measurements([Measurement::new(MetricKind::P99Latency, 260.0)]),
        );

        recorder.record(
            RecordInput::new(3_000, 9, "ollama")
                .with_evaluated_scenarios(["inference_tail_guard"])
                .with_measurements([Measurement::new(MetricKind::P99Latency, 250.0)]),
        );

        let engine = ExplainTuneEngine::default();
        let report = engine.analyze(recorder.records(), recorder.traces(), &sample_policies());

        assert!(report.tune_suggestions.iter().any(|suggestion| {
            suggestion.scenario == "inference_tail_guard"
                && suggestion.target == "triggers"
                && suggestion.direction == TuneDirection::Decrease
        }));
    }

    #[test]
    fn reports_tool_call_lifecycle_subchains_and_isolation_evidence() {
        let mut recorder = MetricsRecorder::new(
            MetricsConfig::default()
                .with_tracked_metrics(vec![MetricKind::Custom("tool_call_latency".to_string())]),
        )
        .expect("valid recorder");

        recorder.record(
            RecordInput::new(1_000, 61, "python")
                .with_evaluated_scenarios(["tool_call_booster"])
                .with_triggered_scenarios(["tool_call_booster"])
                .with_action_count(2)
                .with_measurements([Measurement::new(
                    MetricKind::Custom("tool_call_latency".to_string()),
                    120.0,
                )])
                .with_traces([MetricTrace::new(
                    1_000,
                    61,
                    "python",
                    TraceKind::ActionApplied,
                    "applied retrieval boost",
                )
                .with_scenario("tool_call_booster")
                .with_field("tool_call_id", "tc-001")
                .with_field("tool_call_stage", "retrieval")
                .with_field("tool_call_subchain", "retrieval_io")
                .with_field("isolation_mode", "retrieval_affinity_only")
                .with_field("background_isolation", "blocked_by_safety")])
                .with_notes(["tool_call_booster:queue_wait_us:2600>=2000".to_string()]),
        );

        recorder.record(
            RecordInput::new(1_200, 62, "python")
                .with_evaluated_scenarios(["tool_call_booster"])
                .with_triggered_scenarios(["tool_call_booster"])
                .with_action_count(1)
                .with_traces([MetricTrace::new(
                    1_200,
                    62,
                    "python",
                    TraceKind::ActionApplied,
                    "applied rerank boost",
                )
                .with_scenario("tool_call_booster")
                .with_field("tool_call_id", "tc-001")
                .with_field("tool_call_stage", "rerank")
                .with_field("tool_call_subchain", "rerank_queue")
                .with_field("isolation_mode", "rerank_affinity_only")
                .with_field("background_isolation", "blocked_by_safety")]),
        );

        let engine = ExplainTuneEngine::default();
        let report = engine.analyze(recorder.records(), recorder.traces(), &sample_policies());

        assert_eq!(report.tool_call_chain_reports.len(), 1);
        let chain = &report.tool_call_chain_reports[0];
        assert_eq!(chain.lifecycle_id, "tc-001");
        assert_eq!(chain.stages.get("retrieval"), Some(&1));
        assert_eq!(chain.stages.get("rerank"), Some(&1));
        assert_eq!(
            chain.isolation_modes.get("retrieval_affinity_only"),
            Some(&1)
        );
        assert_eq!(
            chain.background_isolation.get("blocked_by_safety"),
            Some(&2)
        );
        assert!(report.findings.iter().any(|finding| {
            finding.contains("lifecycle tc-001") && finding.contains("isolation")
        }));
        assert!(report.trigger_explanations.iter().any(|explanation| {
            explanation
                .rationale
                .contains(&"tool_call_subchain:retrieval_io".to_string())
        }));
    }

    fn sample_policies() -> BTreeMap<ScenarioKind, ScenarioPolicy> {
        BTreeMap::from([
            (
                ScenarioKind::InferenceTailGuard,
                ScenarioPolicy {
                    scenario: ScenarioKind::InferenceTailGuard,
                    enabled: true,
                    evaluation_window_ms: 500,
                    cooldown_ms: 1_500,
                    max_boost_duration_ms: 800,
                    triggers: TriggerThresholds {
                        run_queue_delay_us: Some(2_000),
                        offcpu_spike_us: Some(3_000),
                        cpu_migrations_per_sec: Some(10),
                        major_page_faults_per_sec: Some(3),
                        ..TriggerThresholds::default()
                    },
                    actions: ScenarioActions {
                        raise_nice: Some(-5),
                        pin_strategy: Some(PinStrategy::PreferReservedCores),
                        ..ScenarioActions::default()
                    },
                },
            ),
            (
                ScenarioKind::ToolCallBooster,
                ScenarioPolicy {
                    scenario: ScenarioKind::ToolCallBooster,
                    enabled: true,
                    evaluation_window_ms: 300,
                    cooldown_ms: 800,
                    max_boost_duration_ms: 1_200,
                    triggers: TriggerThresholds {
                        subprocess_start_delay_us: Some(1_500),
                        queue_wait_us: Some(2_000),
                        optional_io_latency_us: Some(4_000),
                        ..TriggerThresholds::default()
                    },
                    actions: ScenarioActions {
                        raise_nice: Some(-3),
                        pin_strategy: Some(PinStrategy::PreferLowContentionCores),
                        warmup_executor: Some(true),
                        ..ScenarioActions::default()
                    },
                },
            ),
        ])
    }
}
