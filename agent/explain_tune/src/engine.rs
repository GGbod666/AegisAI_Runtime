use std::collections::{BTreeMap, BTreeSet};

use aegisai_metrics::{MetricKind, MetricRecord, MetricTrace, MetricTrend, TraceKind};
use aegisai_policy_engine::{ScenarioKind, ScenarioPolicy, TriggerThresholds};

use crate::config::{validate_config, ExplainTuneConfig, ExplainTuneConfigError};
use crate::model::{
    ExperimentReport, MetricInsight, MetricSummary, ScenarioReport, ToolCallChainReport,
    TraceEvidence, TriggerExplanation, TuneDirection, TuneSuggestion,
};

pub struct ExplainTuneEngine {
    config: ExplainTuneConfig,
}

impl Default for ExplainTuneEngine {
    fn default() -> Self {
        Self::new(ExplainTuneConfig::default())
            .expect("default explain/tune config should be valid")
    }
}

impl ExplainTuneEngine {
    pub fn new(mut config: ExplainTuneConfig) -> Result<Self, ExplainTuneConfigError> {
        validate_config(&mut config)?;
        Ok(Self { config })
    }

    pub fn config(&self) -> &ExplainTuneConfig {
        &self.config
    }

    pub fn analyze(
        &self,
        records: &[MetricRecord],
        traces: &[MetricTrace],
        policies: &BTreeMap<ScenarioKind, ScenarioPolicy>,
    ) -> ExperimentReport {
        let generated_at_ms = latest_timestamp(records, traces);
        let mut analyses = Vec::new();

        for scenario in scenario_names(records, traces, policies) {
            analyses.push(self.build_scenario_analysis(&scenario, records, traces));
        }

        let scenario_reports = analyses
            .iter()
            .map(|analysis| analysis.report.clone())
            .collect::<Vec<_>>();
        let tool_call_chain_reports = self.build_tool_call_chain_reports(traces);
        let trigger_explanations = self.collect_trigger_explanations(records, traces, policies);
        let tune_suggestions = self.build_tune_suggestions(&analyses, policies);
        let findings = self.build_findings(&scenario_reports, &tool_call_chain_reports);

        ExperimentReport {
            generated_at_ms,
            total_records: records.len(),
            total_traces: traces.len(),
            scenario_reports,
            tool_call_chain_reports,
            trigger_explanations,
            tune_suggestions,
            findings,
        }
    }

    pub fn explain_record(
        &self,
        record: &MetricRecord,
        traces: &[MetricTrace],
        policies: &BTreeMap<ScenarioKind, ScenarioPolicy>,
    ) -> Vec<TriggerExplanation> {
        let relevant_traces = traces
            .iter()
            .filter(|trace| {
                trace.timestamp_ms == record.timestamp_ms
                    && trace.pid == record.pid
                    && trace.process_name == record.process_name
            })
            .collect::<Vec<_>>();

        let mut explanations = Vec::new();
        for scenario in &record.triggered_scenarios {
            let scenario_traces = relevant_traces
                .iter()
                .filter(|trace| {
                    trace.scenario.as_deref() == Some(scenario.as_str()) || trace.scenario.is_none()
                })
                .map(|trace| TraceEvidence {
                    kind: trace.kind.clone(),
                    summary: trace.summary.clone(),
                    fields: trace.fields.clone(),
                })
                .collect::<Vec<_>>();

            let mut rationale = Vec::new();
            let scenario_notes = scenario_specific_notes(record, scenario);
            if !scenario_notes.is_empty() {
                rationale.extend(
                    scenario_notes
                        .iter()
                        .map(|note| format!("trigger_reason:{note}")),
                );
            }

            if let Some(policy) = policy_for_scenario(policies, scenario) {
                rationale.push(format!(
                    "policy_window={}ms cooldown={}ms max_boost={}ms",
                    policy.evaluation_window_ms, policy.cooldown_ms, policy.max_boost_duration_ms
                ));

                let thresholds = format_thresholds(&policy.triggers);
                if !thresholds.is_empty() {
                    rationale.push(format!("configured_thresholds:{}", thresholds.join(",")));
                }
            }

            for field in [
                "tool_call_id",
                "tool_call_stage",
                "tool_call_subchain",
                "isolation_mode",
                "background_isolation",
            ] {
                for value in scenario_traces
                    .iter()
                    .filter_map(|trace| trace.fields.get(field))
                {
                    rationale.push(format!("{field}:{value}"));
                }
            }

            for trace in &scenario_traces {
                match trace.kind {
                    TraceKind::ActionApplied
                    | TraceKind::ActionRolledBack
                    | TraceKind::Evaluation => {
                        rationale.push(trace.summary.clone());
                    }
                    _ => {}
                }
            }

            if record.side_effect_count > 0 {
                rationale.push(format!(
                    "side_effects_observed:{}",
                    record.side_effect_count
                ));
            }
            if record.rollback_count > 0 {
                rationale.push(format!("rollback_count:{}", record.rollback_count));
            }

            let rationale = dedupe_preserving_order(rationale);
            let notes = dedupe_preserving_order(
                record
                    .notes
                    .iter()
                    .filter(|note| {
                        note.starts_with(&format!("{scenario}:"))
                            || note.starts_with("applied:")
                            || note.starts_with("rolled_back:")
                            || note.starts_with("side_effects:")
                            || note.as_str() == format!("rollback:{scenario}")
                    })
                    .cloned()
                    .collect(),
            );

            let metric_context = record
                .tracked_metrics
                .iter()
                .map(|(kind, trend)| (kind.clone(), self.metric_insight(trend)))
                .collect::<BTreeMap<_, _>>();

            let summary = if !scenario_notes.is_empty() {
                format!(
                    "scenario {scenario} triggered for {}(pid={}) because {}",
                    record.process_name,
                    record.pid,
                    scenario_notes.join("; ")
                )
            } else {
                format!(
                    "scenario {scenario} triggered for {}(pid={}) with {} action(s)",
                    record.process_name, record.pid, record.action_count
                )
            };

            explanations.push(TriggerExplanation {
                scenario: scenario.clone(),
                timestamp_ms: record.timestamp_ms,
                pid: record.pid,
                process_name: record.process_name.clone(),
                summary,
                rationale,
                trace_evidence: scenario_traces,
                metric_context,
                notes,
            });
        }

        explanations
    }

    fn build_scenario_analysis(
        &self,
        scenario: &str,
        records: &[MetricRecord],
        traces: &[MetricTrace],
    ) -> ScenarioAnalysis {
        let mut metric_summaries = BTreeMap::new();
        let mut evaluations = 0_usize;
        let mut triggers = 0_usize;
        let mut side_effect_records = 0_usize;
        let mut action_share_total = 0.0_f64;
        let mut triggered_improvements = 0_usize;
        let mut triggered_regressions = 0_usize;
        let mut untriggered_regressions = 0_usize;

        for record in records {
            if !record
                .evaluated_scenarios
                .iter()
                .any(|item| item == scenario)
            {
                continue;
            }

            evaluations += 1;
            let triggered = record
                .triggered_scenarios
                .iter()
                .any(|item| item == scenario);
            if triggered {
                triggers += 1;
                if record.side_effect_count > 0 {
                    side_effect_records += 1;
                }
                action_share_total +=
                    record.action_count as f64 / record.triggered_scenarios.len().max(1) as f64;
            }

            let mut record_improved = false;
            let mut record_regressed = false;

            for (kind, trend) in &record.tracked_metrics {
                metric_summaries
                    .entry(kind.clone())
                    .or_insert_with(MetricAccumulator::default)
                    .observe(trend, &self.config);

                if is_performance_metric(kind) {
                    if let Some(ratio) = trend.improvement_ratio {
                        if ratio >= self.config.improvement_threshold {
                            record_improved = true;
                        }
                        if ratio <= -self.config.regression_threshold {
                            record_regressed = true;
                        }
                    }
                }
            }

            if triggered {
                if record_improved {
                    triggered_improvements += 1;
                }
                if record_regressed {
                    triggered_regressions += 1;
                }
            } else if record_regressed {
                untriggered_regressions += 1;
            }
        }

        let rollbacks = traces
            .iter()
            .filter(|trace| {
                trace.kind == TraceKind::ActionRolledBack
                    && trace.scenario.as_deref() == Some(scenario)
            })
            .count();

        let trigger_rate = ratio(triggers, evaluations);
        let rollback_rate = ratio(rollbacks, triggers);
        let side_effect_rate = ratio(side_effect_records, triggers);
        let average_actions_per_trigger = if triggers == 0 {
            0.0
        } else {
            action_share_total / triggers as f64
        };

        let metric_summaries = metric_summaries
            .into_iter()
            .map(|(kind, summary)| (kind, summary.finish()))
            .collect::<BTreeMap<_, _>>();

        let mut notes = Vec::new();
        if evaluations == 0 {
            notes.push("no_evaluated_windows".to_string());
        }
        if trigger_rate >= self.config.high_trigger_rate && evaluations > 0 {
            notes.push("trigger_rate_high".to_string());
        }
        if rollback_rate >= self.config.high_rollback_rate && triggers > 0 {
            notes.push("rollback_rate_high".to_string());
        }
        if side_effect_rate >= self.config.high_side_effect_rate && triggers > 0 {
            notes.push("side_effect_rate_high".to_string());
        }
        if let Some((metric, summary)) = best_metric_summary(&metric_summaries) {
            if let Some(ratio) = summary.average_improvement_ratio {
                notes.push(format!(
                    "best_metric:{}:{:.1}%",
                    metric.as_str(),
                    ratio * 100.0
                ));
            }
        }
        if triggered_improvements > 0 {
            notes.push(format!(
                "triggered_improvement_windows:{}",
                triggered_improvements
            ));
        }
        if triggered_regressions > 0 {
            notes.push(format!(
                "triggered_regression_windows:{}",
                triggered_regressions
            ));
        }
        if untriggered_regressions > 0 {
            notes.push(format!(
                "untriggered_regression_windows:{}",
                untriggered_regressions
            ));
        }

        ScenarioAnalysis {
            report: ScenarioReport {
                scenario: scenario.to_string(),
                evaluations,
                triggers,
                trigger_rate,
                rollbacks,
                rollback_rate,
                side_effect_records,
                side_effect_rate,
                average_actions_per_trigger,
                metric_summaries,
                notes,
            },
            triggered_improvements,
            triggered_regressions,
            untriggered_regressions,
        }
    }

    fn collect_trigger_explanations(
        &self,
        records: &[MetricRecord],
        traces: &[MetricTrace],
        policies: &BTreeMap<ScenarioKind, ScenarioPolicy>,
    ) -> Vec<TriggerExplanation> {
        let mut explanations = records
            .iter()
            .flat_map(|record| self.explain_record(record, traces, policies))
            .collect::<Vec<_>>();

        explanations.sort_by(|left, right| {
            right
                .timestamp_ms
                .cmp(&left.timestamp_ms)
                .then_with(|| left.scenario.cmp(&right.scenario))
                .then_with(|| left.pid.cmp(&right.pid))
        });
        explanations.truncate(self.config.max_trigger_explanations);
        explanations
    }

    fn build_tune_suggestions(
        &self,
        analyses: &[ScenarioAnalysis],
        policies: &BTreeMap<ScenarioKind, ScenarioPolicy>,
    ) -> Vec<TuneSuggestion> {
        let mut suggestions = Vec::new();

        for analysis in analyses {
            let scenario = &analysis.report.scenario;
            let Some(policy) = policy_for_scenario(policies, scenario) else {
                continue;
            };

            let mean_improvement = best_metric_summary(&analysis.report.metric_summaries)
                .and_then(|(_, summary)| summary.average_improvement_ratio)
                .unwrap_or(0.0);

            let noisy_policy = analysis.report.rollback_rate >= self.config.high_rollback_rate
                || analysis.report.side_effect_rate >= self.config.high_side_effect_rate
                || (analysis.report.trigger_rate >= self.config.high_trigger_rate
                    && mean_improvement <= self.config.improvement_threshold);

            if noisy_policy {
                suggestions.push(TuneSuggestion {
                    scenario: scenario.clone(),
                    target: "policy.max_boost_duration_ms".to_string(),
                    direction: TuneDirection::Decrease,
                    suggested_value: Some(
                        scale_u64(
                            policy.max_boost_duration_ms,
                            1.0 - self.config.action_step_ratio,
                        )
                        .to_string(),
                    ),
                    confidence: 0.82,
                    rationale: vec![
                        format!(
                            "rollback_rate={:.2} side_effect_rate={:.2} trigger_rate={:.2}",
                            analysis.report.rollback_rate,
                            analysis.report.side_effect_rate,
                            analysis.report.trigger_rate
                        ),
                        format!("mean_improvement={:.2}%", mean_improvement * 100.0),
                        format!(
                            "triggered_improvements={} triggered_regressions={}",
                            analysis.triggered_improvements, analysis.triggered_regressions
                        ),
                    ],
                });

                if let Some(delta) = policy.actions.raise_nice {
                    suggestions.push(TuneSuggestion {
                        scenario: scenario.clone(),
                        target: "actions.raise_nice".to_string(),
                        direction: TuneDirection::Decrease,
                        suggested_value: Some(
                            relax_priority_delta(delta, self.config.action_step_ratio).to_string(),
                        ),
                        confidence: 0.76,
                        rationale: vec![
                            "priority boost may be too aggressive for observed stability"
                                .to_string(),
                        ],
                    });
                } else if !format_thresholds(&policy.triggers).is_empty() {
                    suggestions.push(TuneSuggestion {
                        scenario: scenario.clone(),
                        target: "triggers".to_string(),
                        direction: TuneDirection::Increase,
                        suggested_value: Some(scale_thresholds(
                            &policy.triggers,
                            1.0 + self.config.threshold_step_ratio,
                        )),
                        confidence: 0.71,
                        rationale: vec![
                            "raising thresholds should reduce noisy activations".to_string()
                        ],
                    });
                }

                continue;
            }

            if analysis.report.trigger_rate <= self.config.low_trigger_rate
                && analysis.untriggered_regressions > analysis.report.triggers
                && !format_thresholds(&policy.triggers).is_empty()
            {
                suggestions.push(TuneSuggestion {
                    scenario: scenario.clone(),
                    target: "triggers".to_string(),
                    direction: TuneDirection::Decrease,
                    suggested_value: Some(scale_thresholds(
                        &policy.triggers,
                        1.0 - self.config.threshold_step_ratio,
                    )),
                    confidence: 0.74,
                    rationale: vec![
                        format!(
                            "untriggered_regressions={} exceeded triggers={}",
                            analysis.untriggered_regressions, analysis.report.triggers
                        ),
                        format!("trigger_rate={:.2}", analysis.report.trigger_rate),
                    ],
                });

                if policy.cooldown_ms > 0 {
                    suggestions.push(TuneSuggestion {
                        scenario: scenario.clone(),
                        target: "policy.cooldown_ms".to_string(),
                        direction: TuneDirection::Decrease,
                        suggested_value: Some(
                            scale_u64(policy.cooldown_ms, 1.0 - self.config.action_step_ratio)
                                .to_string(),
                        ),
                        confidence: 0.61,
                        rationale: vec![
                            "shorter cooldown can help catch repeat regressions earlier"
                                .to_string(),
                        ],
                    });
                }
            }
        }

        suggestions
    }

    fn build_tool_call_chain_reports(&self, traces: &[MetricTrace]) -> Vec<ToolCallChainReport> {
        let mut chains = BTreeMap::<String, ToolCallChainAccumulator>::new();

        for trace in traces {
            if trace.scenario.as_deref() != Some("tool_call_booster") {
                continue;
            }

            let Some(lifecycle_id) = trace.fields.get("tool_call_id").cloned() else {
                continue;
            };

            let entry =
                chains
                    .entry(lifecycle_id.clone())
                    .or_insert_with(|| ToolCallChainAccumulator {
                        lifecycle_id,
                        ..ToolCallChainAccumulator::default()
                    });
            entry.target_pids.insert(trace.pid);

            if trace.kind == TraceKind::ActionApplied {
                entry.trigger_count += 1;
            }
            if trace.kind == TraceKind::ActionRolledBack {
                entry.rollback_count += 1;
            }
            if let Some(stage) = trace.fields.get("tool_call_stage") {
                *entry.stages.entry(stage.clone()).or_default() += 1;
            }
            if let Some(subchain) = trace.fields.get("tool_call_subchain") {
                entry.evidence.push(format!("subchain:{subchain}"));
            }
            if let Some(mode) = trace.fields.get("isolation_mode") {
                *entry.isolation_modes.entry(mode.clone()).or_default() += 1;
            }
            if let Some(background) = trace.fields.get("background_isolation") {
                *entry
                    .background_isolation
                    .entry(background.clone())
                    .or_default() += 1;
            }
        }

        chains
            .into_values()
            .map(ToolCallChainAccumulator::finish)
            .collect()
    }

    fn build_findings(
        &self,
        reports: &[ScenarioReport],
        tool_call_chains: &[ToolCallChainReport],
    ) -> Vec<String> {
        let mut findings = Vec::new();

        for report in reports {
            if report.evaluations == 0 {
                continue;
            }

            if let Some((metric, summary)) = best_metric_summary(&report.metric_summaries) {
                if let Some(improvement) = summary.average_improvement_ratio {
                    if improvement.abs() >= self.config.improvement_threshold {
                        findings.push(format!(
                            "{} {} changed by {:.1}% on average across {} baselined samples",
                            report.scenario,
                            metric.as_str(),
                            improvement * 100.0,
                            summary.baselined_samples
                        ));
                    }
                }
            }

            if report.rollback_rate >= self.config.high_rollback_rate && report.triggers > 0 {
                findings.push(format!(
                    "{} rollback rate reached {:.1}%",
                    report.scenario,
                    report.rollback_rate * 100.0
                ));
            }

            if report.side_effect_rate >= self.config.high_side_effect_rate && report.triggers > 0 {
                findings.push(format!(
                    "{} side effects appeared in {:.1}% of triggered windows",
                    report.scenario,
                    report.side_effect_rate * 100.0
                ));
            }

            if report.trigger_rate >= self.config.high_trigger_rate {
                findings.push(format!(
                    "{} triggered in {:.1}% of evaluated windows",
                    report.scenario,
                    report.trigger_rate * 100.0
                ));
            }
        }

        for chain in tool_call_chains {
            let stages = chain.stages.keys().cloned().collect::<Vec<_>>().join(",");
            findings.push(format!(
                "tool_call_booster lifecycle {} covered stages [{}] with {} isolation event(s)",
                chain.lifecycle_id,
                stages,
                chain.isolation_modes.values().sum::<usize>()
            ));

            if chain
                .background_isolation
                .get("blocked_by_safety")
                .copied()
                .unwrap_or(0)
                > 0
            {
                findings.push(format!(
                    "tool_call_booster lifecycle {} kept background isolation blocked by safety",
                    chain.lifecycle_id
                ));
            }
        }

        dedupe_preserving_order(findings)
    }

    fn metric_insight(&self, trend: &MetricTrend) -> MetricInsight {
        let assessment = match trend.improvement_ratio {
            Some(ratio) if ratio >= self.config.improvement_threshold => "improved".to_string(),
            Some(ratio) if ratio <= -self.config.regression_threshold => "regressed".to_string(),
            Some(_) => "neutral".to_string(),
            None => "baseline_pending".to_string(),
        };

        MetricInsight {
            baseline: trend.baseline,
            current: trend.current,
            delta: trend.delta,
            improvement_ratio: trend.improvement_ratio,
            assessment,
        }
    }
}

#[derive(Default)]
struct ToolCallChainAccumulator {
    lifecycle_id: String,
    stages: BTreeMap<String, usize>,
    trigger_count: usize,
    rollback_count: usize,
    isolation_modes: BTreeMap<String, usize>,
    background_isolation: BTreeMap<String, usize>,
    target_pids: BTreeSet<u32>,
    evidence: Vec<String>,
}

impl ToolCallChainAccumulator {
    fn finish(self) -> ToolCallChainReport {
        ToolCallChainReport {
            lifecycle_id: self.lifecycle_id,
            stages: self.stages,
            trigger_count: self.trigger_count,
            rollback_count: self.rollback_count,
            isolation_modes: self.isolation_modes,
            background_isolation: self.background_isolation,
            target_pids: self.target_pids.into_iter().collect(),
            evidence: dedupe_preserving_order(self.evidence),
        }
    }
}

struct ScenarioAnalysis {
    report: ScenarioReport,
    triggered_improvements: usize,
    triggered_regressions: usize,
    untriggered_regressions: usize,
}

#[derive(Default)]
struct MetricAccumulator {
    samples: usize,
    baselined_samples: usize,
    improvement_samples: usize,
    baseline_total: f64,
    current_total: f64,
    delta_total: f64,
    improvement_total: f64,
    improved_samples: usize,
    regressed_samples: usize,
}

impl MetricAccumulator {
    fn observe(&mut self, trend: &MetricTrend, config: &ExplainTuneConfig) {
        self.samples += 1;
        self.current_total += trend.current;

        if let Some(baseline) = trend.baseline {
            self.baselined_samples += 1;
            self.baseline_total += baseline;
        }

        if let Some(delta) = trend.delta {
            self.delta_total += delta;
        }

        if let Some(improvement_ratio) = trend.improvement_ratio {
            self.improvement_samples += 1;
            self.improvement_total += improvement_ratio;
            if improvement_ratio >= config.improvement_threshold {
                self.improved_samples += 1;
            }
            if improvement_ratio <= -config.regression_threshold {
                self.regressed_samples += 1;
            }
        }
    }

    fn finish(self) -> MetricSummary {
        MetricSummary {
            samples: self.samples,
            baselined_samples: self.baselined_samples,
            average_baseline: average(self.baseline_total, self.baselined_samples),
            average_current: average(self.current_total, self.samples).unwrap_or(0.0),
            average_delta: average(self.delta_total, self.baselined_samples),
            average_improvement_ratio: average(self.improvement_total, self.improvement_samples),
            improved_samples: self.improved_samples,
            regressed_samples: self.regressed_samples,
        }
    }
}

fn scenario_names(
    records: &[MetricRecord],
    traces: &[MetricTrace],
    policies: &BTreeMap<ScenarioKind, ScenarioPolicy>,
) -> BTreeSet<String> {
    let mut scenarios = BTreeSet::new();

    for record in records {
        scenarios.extend(record.evaluated_scenarios.iter().cloned());
        scenarios.extend(record.triggered_scenarios.iter().cloned());
    }

    for trace in traces {
        if let Some(scenario) = &trace.scenario {
            scenarios.insert(scenario.clone());
        }
    }

    scenarios.extend(
        policies
            .keys()
            .map(|scenario| scenario.as_str().to_string()),
    );
    scenarios
}

fn latest_timestamp(records: &[MetricRecord], traces: &[MetricTrace]) -> u64 {
    let records_max = records
        .iter()
        .map(|record| record.timestamp_ms)
        .max()
        .unwrap_or(0);
    let traces_max = traces
        .iter()
        .map(|trace| trace.timestamp_ms)
        .max()
        .unwrap_or(0);
    records_max.max(traces_max)
}

fn average(total: f64, samples: usize) -> Option<f64> {
    if samples == 0 {
        None
    } else {
        Some(total / samples as f64)
    }
}

fn ratio(numerator: usize, denominator: usize) -> f64 {
    if denominator == 0 {
        0.0
    } else {
        numerator as f64 / denominator as f64
    }
}

fn best_metric_summary(
    summaries: &BTreeMap<MetricKind, MetricSummary>,
) -> Option<(&MetricKind, &MetricSummary)> {
    summaries
        .iter()
        .filter(|(kind, _)| is_performance_metric(kind))
        .filter(|(_, summary)| summary.average_improvement_ratio.is_some())
        .max_by(|left, right| {
            left.1
                .average_improvement_ratio
                .partial_cmp(&right.1.average_improvement_ratio)
                .unwrap_or(std::cmp::Ordering::Equal)
        })
}

fn is_performance_metric(kind: &MetricKind) -> bool {
    matches!(
        kind,
        MetricKind::Ttft
            | MetricKind::P95Latency
            | MetricKind::P99Latency
            | MetricKind::Jitter
            | MetricKind::Custom(_)
    )
}

fn scenario_specific_notes(record: &MetricRecord, scenario: &str) -> Vec<String> {
    record
        .notes
        .iter()
        .filter_map(|note| note.strip_prefix(&format!("{scenario}:")))
        .map(str::to_string)
        .collect()
}

fn policy_for_scenario<'a>(
    policies: &'a BTreeMap<ScenarioKind, ScenarioPolicy>,
    scenario: &str,
) -> Option<&'a ScenarioPolicy> {
    policies.get(&ScenarioKind::parse(scenario))
}

fn format_thresholds(triggers: &TriggerThresholds) -> Vec<String> {
    let mut values = Vec::new();

    if let Some(value) = triggers.run_queue_delay_us {
        values.push(format!("run_queue_delay_us={value}"));
    }
    if let Some(value) = triggers.offcpu_spike_us {
        values.push(format!("offcpu_spike_us={value}"));
    }
    if let Some(value) = triggers.cpu_migrations_per_sec {
        values.push(format!("cpu_migrations_per_sec={value}"));
    }
    if let Some(value) = triggers.major_page_faults_per_sec {
        values.push(format!("major_page_faults_per_sec={value}"));
    }
    if let Some(value) = triggers.subprocess_start_delay_us {
        values.push(format!("subprocess_start_delay_us={value}"));
    }
    if let Some(value) = triggers.queue_wait_us {
        values.push(format!("queue_wait_us={value}"));
    }
    if let Some(value) = triggers.optional_io_latency_us {
        values.push(format!("optional_io_latency_us={value}"));
    }

    values
}

fn scale_u64(value: u64, factor: f64) -> u64 {
    ((value as f64 * factor).round() as u64).max(1)
}

fn relax_priority_delta(delta: i32, step_ratio: f64) -> i32 {
    if delta == 0 {
        return 0;
    }

    let scaled = delta as f64 * (1.0 - step_ratio);
    if delta < 0 {
        scaled.ceil() as i32
    } else {
        scaled.floor() as i32
    }
}

fn scale_thresholds(triggers: &TriggerThresholds, factor: f64) -> String {
    let mut values = Vec::new();

    if let Some(value) = triggers.run_queue_delay_us {
        values.push(format!("run_queue_delay_us={}", scale_u64(value, factor)));
    }
    if let Some(value) = triggers.offcpu_spike_us {
        values.push(format!("offcpu_spike_us={}", scale_u64(value, factor)));
    }
    if let Some(value) = triggers.cpu_migrations_per_sec {
        values.push(format!(
            "cpu_migrations_per_sec={}",
            scale_u64(value, factor)
        ));
    }
    if let Some(value) = triggers.major_page_faults_per_sec {
        values.push(format!(
            "major_page_faults_per_sec={}",
            scale_u64(value, factor)
        ));
    }
    if let Some(value) = triggers.subprocess_start_delay_us {
        values.push(format!(
            "subprocess_start_delay_us={}",
            scale_u64(value, factor)
        ));
    }
    if let Some(value) = triggers.queue_wait_us {
        values.push(format!("queue_wait_us={}", scale_u64(value, factor)));
    }
    if let Some(value) = triggers.optional_io_latency_us {
        values.push(format!(
            "optional_io_latency_us={}",
            scale_u64(value, factor)
        ));
    }

    values.join(",")
}

fn dedupe_preserving_order(items: Vec<String>) -> Vec<String> {
    let mut unique = Vec::new();
    for item in items {
        if !unique.contains(&item) {
            unique.push(item);
        }
    }
    unique
}
