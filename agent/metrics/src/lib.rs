#![forbid(unsafe_code)]

//! Metrics and trace recording for the AegisAI Runtime control loop.
//!
//! This crate records two complementary views:
//! - stable metric snapshots for offline experiment analysis
//! - trace records that explain why a scenario triggered or rolled back

mod config;
mod model;
mod recorder;

pub use config::{MetricsConfig, MetricsConfigError};
pub use model::{
    Measurement, MetricKind, MetricRecord, MetricTrace, MetricTrend, RecordInput, ScenarioStats,
    SideEffect, TraceKind,
};
pub use recorder::MetricsRecorder;

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;

    use crate::{
        Measurement, MetricKind, MetricTrace, MetricsConfig, MetricsConfigError, MetricsRecorder,
        RecordInput, SideEffect, TraceKind,
    };

    #[test]
    fn rejects_invalid_config() {
        let config = MetricsConfig {
            baseline_window: 0,
            ..MetricsConfig::default()
        };
        assert!(matches!(
            MetricsRecorder::new(config),
            Err(MetricsConfigError::InvalidBaselineWindow)
        ));
    }

    #[test]
    fn records_synthesized_metrics_and_default_traces() {
        let config = MetricsConfig::default().with_tracked_metrics(vec![
            MetricKind::BoostHitRate,
            MetricKind::RollbackCount,
            MetricKind::SideEffectRate,
        ]);
        let mut recorder = MetricsRecorder::new(config).expect("valid recorder");

        let record = recorder.record(
            RecordInput::new(1_000, 42, "ollama")
                .with_workload_tags(["AI_INFERENCE", "INTERACTIVE_LATENCY_SENSITIVE"])
                .with_evaluated_scenarios(["inference_tail_guard", "tool_call_booster"])
                .with_triggered_scenarios(["inference_tail_guard"])
                .with_action_count(2),
        );

        assert_eq!(record.action_count, 2);
        assert_eq!(record.triggered_scenarios, vec!["inference_tail_guard"]);
        assert_eq!(
            record
                .metric(&MetricKind::BoostHitRate)
                .expect("boost hit rate should be synthesized")
                .current,
            0.5
        );
        assert_eq!(
            record
                .metric(&MetricKind::RollbackCount)
                .expect("rollback count should be synthesized")
                .current,
            0.0
        );
        assert_eq!(
            record
                .metric(&MetricKind::SideEffectRate)
                .expect("side effect rate should be synthesized")
                .current,
            0.0
        );
        assert_eq!(recorder.traces().len(), 2);
        assert!(recorder
            .traces()
            .iter()
            .all(|trace| trace.kind == TraceKind::Evaluation));
    }

    #[test]
    fn computes_metric_baseline_and_improvement_ratio() {
        let config = MetricsConfig::default().with_tracked_metrics(vec![MetricKind::Ttft]);
        let mut recorder = MetricsRecorder::new(config).expect("valid recorder");

        let first = recorder.record(
            RecordInput::new(1_000, 7, "ollama")
                .with_measurements([Measurement::new(MetricKind::Ttft, 120.0)]),
        );
        assert!(first.metric(&MetricKind::Ttft).unwrap().baseline.is_none());

        let second = recorder.record(
            RecordInput::new(2_000, 7, "ollama")
                .with_measurements([Measurement::new(MetricKind::Ttft, 90.0)]),
        );
        let ttft = second.metric(&MetricKind::Ttft).expect("ttft metric");
        assert_eq!(ttft.baseline, Some(120.0));
        assert_eq!(ttft.delta, Some(-30.0));
        assert_eq!(ttft.improvement_ratio, Some(0.25));
    }

    #[test]
    fn records_explicit_action_and_rollback_traces() {
        let config = MetricsConfig::default().with_tracked_metrics(vec![MetricKind::RollbackCount]);
        let mut recorder = MetricsRecorder::new(config).expect("valid recorder");

        let trace = MetricTrace::new(5_000, 99, "python", TraceKind::ActionRolledBack, "rollback")
            .with_scenario("tool_call_booster")
            .with_field("reason", "lease_expired");

        let record = recorder.record(
            RecordInput::new(5_000, 99, "python")
                .with_evaluated_scenarios(["tool_call_booster"])
                .with_triggered_scenarios(["tool_call_booster"])
                .with_rollback_count(1)
                .with_side_effects([SideEffect::PriorityInversion])
                .with_traces([trace]),
        );

        assert_eq!(
            record
                .metric(&MetricKind::RollbackCount)
                .expect("rollback metric")
                .current,
            1.0
        );
        assert_eq!(
            recorder.scenario_stats("tool_call_booster"),
            Some(crate::ScenarioStats {
                evaluations: 1,
                triggers: 1,
                rollbacks: 1,
            })
        );
        assert!(recorder
            .traces()
            .iter()
            .any(|item| item.kind == TraceKind::ActionRolledBack));
    }

    #[test]
    fn enforces_record_and_trace_capacity() {
        let config = MetricsConfig {
            max_records: 2,
            max_traces: 2,
            ..MetricsConfig::default()
        };

        let mut recorder = MetricsRecorder::new(config).expect("valid recorder");

        for timestamp in [1_000, 2_000, 3_000] {
            recorder.record(
                RecordInput::new(timestamp, 1, "ollama")
                    .with_evaluated_scenarios(["inference_tail_guard"])
                    .with_traces([MetricTrace::new(
                        timestamp,
                        1,
                        "ollama",
                        TraceKind::ActionApplied,
                        "applied",
                    )]),
            );
        }

        assert_eq!(recorder.records().len(), 2);
        assert_eq!(recorder.records()[0].timestamp_ms, 2_000);
        assert_eq!(recorder.traces().len(), 2);
        assert_eq!(recorder.traces()[0].timestamp_ms, 3_000);
    }

    #[test]
    fn record_input_builders_deduplicate_lists() {
        let record = RecordInput::new(1_000, 10, "worker")
            .with_workload_tags(["TOOL_CALL", "TOOL_CALL"])
            .with_evaluated_scenarios(["tool_call_booster", "tool_call_booster"]);

        let mut expected_tags = BTreeSet::new();
        expected_tags.insert("TOOL_CALL".to_string());

        assert_eq!(record.workload_tags, expected_tags);
        assert_eq!(record.evaluated_scenarios, vec!["tool_call_booster"]);
    }
}
