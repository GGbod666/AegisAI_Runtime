use std::collections::BTreeMap;

use aegisai_metrics::{MetricKind, TraceKind};

#[derive(Clone, Debug, PartialEq)]
pub struct ExperimentReport {
    pub generated_at_ms: u64,
    pub total_records: usize,
    pub total_traces: usize,
    pub scenario_reports: Vec<ScenarioReport>,
    pub tool_call_chain_reports: Vec<ToolCallChainReport>,
    pub trigger_explanations: Vec<TriggerExplanation>,
    pub tune_suggestions: Vec<TuneSuggestion>,
    pub findings: Vec<String>,
}

impl ExperimentReport {
    pub fn scenario(&self, scenario: &str) -> Option<&ScenarioReport> {
        self.scenario_reports
            .iter()
            .find(|report| report.scenario == scenario)
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ToolCallChainReport {
    pub lifecycle_id: String,
    pub stages: BTreeMap<String, usize>,
    pub trigger_count: usize,
    pub rollback_count: usize,
    pub isolation_modes: BTreeMap<String, usize>,
    pub background_isolation: BTreeMap<String, usize>,
    pub target_pids: Vec<u32>,
    pub evidence: Vec<String>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct ScenarioReport {
    pub scenario: String,
    pub evaluations: usize,
    pub triggers: usize,
    pub trigger_rate: f64,
    pub rollbacks: usize,
    pub rollback_rate: f64,
    pub side_effect_records: usize,
    pub side_effect_rate: f64,
    pub average_actions_per_trigger: f64,
    pub metric_summaries: BTreeMap<MetricKind, MetricSummary>,
    pub notes: Vec<String>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct MetricSummary {
    pub samples: usize,
    pub baselined_samples: usize,
    pub average_baseline: Option<f64>,
    pub average_current: f64,
    pub average_delta: Option<f64>,
    pub average_improvement_ratio: Option<f64>,
    pub improved_samples: usize,
    pub regressed_samples: usize,
}

#[derive(Clone, Debug, PartialEq)]
pub struct TriggerExplanation {
    pub scenario: String,
    pub timestamp_ms: u64,
    pub pid: u32,
    pub process_name: String,
    pub summary: String,
    pub rationale: Vec<String>,
    pub trace_evidence: Vec<TraceEvidence>,
    pub metric_context: BTreeMap<MetricKind, MetricInsight>,
    pub notes: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TraceEvidence {
    pub kind: TraceKind,
    pub summary: String,
    pub fields: BTreeMap<String, String>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct MetricInsight {
    pub baseline: Option<f64>,
    pub current: f64,
    pub delta: Option<f64>,
    pub improvement_ratio: Option<f64>,
    pub assessment: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TuneDirection {
    Increase,
    Decrease,
    Enable,
    Disable,
    Keep,
}

#[derive(Clone, Debug, PartialEq)]
pub struct TuneSuggestion {
    pub scenario: String,
    pub target: String,
    pub direction: TuneDirection,
    pub suggested_value: Option<String>,
    pub confidence: f64,
    pub rationale: Vec<String>,
}
