use std::collections::{BTreeMap, VecDeque};

use crate::config::{validate_config, MetricsConfig, MetricsConfigError};
use crate::model::{
    MetricKind, MetricRecord, MetricTrace, MetricTrend, RecordInput, ScenarioStats, TraceKind,
};

pub struct MetricsRecorder {
    config: MetricsConfig,
    histories: BTreeMap<(u32, MetricKind), VecDeque<f64>>,
    scenario_stats: BTreeMap<String, ScenarioStats>,
    records: Vec<MetricRecord>,
    traces: Vec<MetricTrace>,
}

impl Default for MetricsRecorder {
    fn default() -> Self {
        Self::new(MetricsConfig::default()).expect("default metrics config should be valid")
    }
}

impl MetricsRecorder {
    pub fn new(mut config: MetricsConfig) -> Result<Self, MetricsConfigError> {
        validate_config(&mut config)?;

        Ok(Self {
            config,
            histories: BTreeMap::new(),
            scenario_stats: BTreeMap::new(),
            records: Vec::new(),
            traces: Vec::new(),
        })
    }

    pub fn config(&self) -> &MetricsConfig {
        &self.config
    }

    pub fn record(&mut self, mut input: RecordInput) -> MetricRecord {
        normalize_input(&mut input);
        self.update_scenario_stats(&input);

        let traces = self.build_traces(&input);
        for trace in traces {
            self.push_trace(trace);
        }

        let tracked_metrics = self.build_metric_snapshot(&input);

        let mut notes = input.notes;
        if input.triggered_scenarios.is_empty() {
            notes.push("no_scenario_triggered".to_string());
        }
        if input.rollback_count > 0 {
            notes.push(format!("rolled_back:{} action(s)", input.rollback_count));
        }
        if input.action_count > 0 {
            notes.push(format!("applied:{} action(s)", input.action_count));
        }
        if !input.side_effects.is_empty() {
            notes.push(format!("side_effects:{}", input.side_effects.len()));
        }

        let record = MetricRecord {
            timestamp_ms: input.timestamp_ms,
            pid: input.pid,
            process_name: input.process_name,
            workload_tags: input.workload_tags,
            evaluated_scenarios: input.evaluated_scenarios,
            triggered_scenarios: input.triggered_scenarios,
            action_count: input.action_count,
            rollback_count: input.rollback_count,
            side_effect_count: input.side_effects.len(),
            tracked_metrics,
            notes,
        };

        self.push_record(record.clone());
        record
    }

    pub fn records(&self) -> &[MetricRecord] {
        &self.records
    }

    pub fn traces(&self) -> &[MetricTrace] {
        &self.traces
    }

    pub fn scenario_stats(&self, scenario: &str) -> Option<ScenarioStats> {
        self.scenario_stats.get(scenario).copied()
    }

    fn update_scenario_stats(&mut self, input: &RecordInput) {
        for scenario in &input.evaluated_scenarios {
            self.scenario_stats
                .entry(scenario.clone())
                .or_default()
                .evaluations += 1;
        }

        for scenario in &input.triggered_scenarios {
            self.scenario_stats
                .entry(scenario.clone())
                .or_default()
                .triggers += 1;
        }

        for trace in &input.traces {
            if trace.kind == TraceKind::ActionRolledBack {
                if let Some(scenario) = &trace.scenario {
                    self.scenario_stats
                        .entry(scenario.clone())
                        .or_default()
                        .rollbacks += 1;
                }
            }
        }
    }

    fn build_traces(&self, input: &RecordInput) -> Vec<MetricTrace> {
        let mut traces = Vec::new();

        for scenario in &input.evaluated_scenarios {
            let triggered = input
                .triggered_scenarios
                .iter()
                .any(|item| item == scenario);
            traces.push(
                MetricTrace::new(
                    input.timestamp_ms,
                    input.pid,
                    input.process_name.clone(),
                    TraceKind::Evaluation,
                    if triggered {
                        format!("scenario {scenario} triggered")
                    } else {
                        format!("scenario {scenario} evaluated without trigger")
                    },
                )
                .with_scenario(scenario.clone())
                .with_field("triggered", triggered.to_string()),
            );
        }

        for measurement in &input.measurements {
            traces.push(
                MetricTrace::new(
                    input.timestamp_ms,
                    input.pid,
                    input.process_name.clone(),
                    TraceKind::MeasurementObserved,
                    format!("observed {}", measurement.kind.as_str()),
                )
                .with_field("metric", measurement.kind.as_str())
                .with_field("value", format!("{:.6}", measurement.value)),
            );
        }

        for effect in &input.side_effects {
            traces.push(
                MetricTrace::new(
                    input.timestamp_ms,
                    input.pid,
                    input.process_name.clone(),
                    TraceKind::SideEffectObserved,
                    format!("observed side effect {}", effect.as_str()),
                )
                .with_field("side_effect", effect.as_str()),
            );
        }

        traces.extend(input.traces.iter().cloned());
        traces
    }

    fn build_metric_snapshot(&mut self, input: &RecordInput) -> BTreeMap<MetricKind, MetricTrend> {
        let mut values = BTreeMap::new();
        for measurement in &input.measurements {
            if self.config.tracks(&measurement.kind) {
                values.insert(measurement.kind.clone(), measurement.value);
            }
        }

        if self.config.tracks(&MetricKind::BoostHitRate) && !input.evaluated_scenarios.is_empty() {
            values.insert(
                MetricKind::BoostHitRate,
                input.triggered_scenarios.len() as f64 / input.evaluated_scenarios.len() as f64,
            );
        }

        if self.config.tracks(&MetricKind::RollbackCount) {
            values.insert(MetricKind::RollbackCount, input.rollback_count as f64);
        }

        if self.config.tracks(&MetricKind::SideEffectRate) {
            let denominator = input.action_count.max(1) as f64;
            values.insert(
                MetricKind::SideEffectRate,
                input.side_effects.len() as f64 / denominator,
            );
        }

        let mut snapshot = BTreeMap::new();
        for (kind, current) in values {
            let history = self.histories.entry((input.pid, kind.clone())).or_default();
            let baseline = mean(history);
            let delta = baseline.map(|previous| current - previous);
            let improvement_ratio = baseline.and_then(|previous| {
                if previous == 0.0 {
                    None
                } else if kind.lower_is_better() {
                    Some((previous - current) / previous)
                } else {
                    Some((current - previous) / previous)
                }
            });

            if history.len() == self.config.baseline_window {
                history.pop_front();
            }
            history.push_back(current);

            snapshot.insert(
                kind,
                MetricTrend {
                    baseline,
                    current,
                    delta,
                    improvement_ratio,
                },
            );
        }

        snapshot
    }

    fn push_record(&mut self, record: MetricRecord) {
        if self.records.len() == self.config.max_records {
            self.records.remove(0);
        }
        self.records.push(record);
    }

    fn push_trace(&mut self, trace: MetricTrace) {
        if self.traces.len() == self.config.max_traces {
            self.traces.remove(0);
        }
        self.traces.push(trace);
    }
}

fn normalize_input(input: &mut RecordInput) {
    input.evaluated_scenarios = dedupe_preserving_order(input.evaluated_scenarios.drain(..));
    input.triggered_scenarios = dedupe_preserving_order(input.triggered_scenarios.drain(..));
    input.notes = dedupe_preserving_order(input.notes.drain(..));
}

fn dedupe_preserving_order<I>(items: I) -> Vec<String>
where
    I: IntoIterator<Item = String>,
{
    let mut unique = Vec::new();
    for item in items {
        if !unique.contains(&item) {
            unique.push(item);
        }
    }
    unique
}

fn mean(history: &VecDeque<f64>) -> Option<f64> {
    if history.is_empty() {
        None
    } else {
        Some(history.iter().sum::<f64>() / history.len() as f64)
    }
}
