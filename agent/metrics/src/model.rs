use std::collections::{BTreeMap, BTreeSet};

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum MetricKind {
    Ttft,
    P95Latency,
    P99Latency,
    Jitter,
    BoostHitRate,
    RollbackCount,
    SideEffectRate,
    Custom(String),
}

impl MetricKind {
    pub fn parse(raw: &str) -> Self {
        match raw.trim() {
            "ttft" => Self::Ttft,
            "p95_latency" => Self::P95Latency,
            "p99_latency" => Self::P99Latency,
            "jitter" => Self::Jitter,
            "boost_hit_rate" => Self::BoostHitRate,
            "rollback_count" => Self::RollbackCount,
            "side_effect_rate" => Self::SideEffectRate,
            other => Self::Custom(other.to_string()),
        }
    }

    pub fn as_str(&self) -> &str {
        match self {
            Self::Ttft => "ttft",
            Self::P95Latency => "p95_latency",
            Self::P99Latency => "p99_latency",
            Self::Jitter => "jitter",
            Self::BoostHitRate => "boost_hit_rate",
            Self::RollbackCount => "rollback_count",
            Self::SideEffectRate => "side_effect_rate",
            Self::Custom(value) => value.as_str(),
        }
    }

    pub fn lower_is_better(&self) -> bool {
        !matches!(self, Self::BoostHitRate)
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct Measurement {
    pub kind: MetricKind,
    pub value: f64,
}

impl Measurement {
    pub fn new(kind: MetricKind, value: f64) -> Self {
        Self { kind, value }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum SideEffect {
    CpuContention,
    PriorityInversion,
    CacheThrash,
    Custom(String),
}

impl SideEffect {
    pub fn as_str(&self) -> &str {
        match self {
            Self::CpuContention => "cpu_contention",
            Self::PriorityInversion => "priority_inversion",
            Self::CacheThrash => "cache_thrash",
            Self::Custom(value) => value.as_str(),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TraceKind {
    Evaluation,
    ActionApplied,
    ActionRolledBack,
    MeasurementObserved,
    SideEffectObserved,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MetricTrace {
    pub timestamp_ms: u64,
    pub pid: u32,
    pub process_name: String,
    pub scenario: Option<String>,
    pub kind: TraceKind,
    pub summary: String,
    pub fields: BTreeMap<String, String>,
}

impl MetricTrace {
    pub fn new(
        timestamp_ms: u64,
        pid: u32,
        process_name: impl Into<String>,
        kind: TraceKind,
        summary: impl Into<String>,
    ) -> Self {
        Self {
            timestamp_ms,
            pid,
            process_name: process_name.into(),
            scenario: None,
            kind,
            summary: summary.into(),
            fields: BTreeMap::new(),
        }
    }

    pub fn with_scenario(mut self, scenario: impl Into<String>) -> Self {
        self.scenario = Some(scenario.into());
        self
    }

    pub fn with_field(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.fields.insert(key.into(), value.into());
        self
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct MetricTrend {
    pub baseline: Option<f64>,
    pub current: f64,
    pub delta: Option<f64>,
    pub improvement_ratio: Option<f64>,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ScenarioStats {
    pub evaluations: u64,
    pub triggers: u64,
    pub rollbacks: u64,
}

#[derive(Clone, Debug, PartialEq)]
pub struct MetricRecord {
    pub timestamp_ms: u64,
    pub pid: u32,
    pub process_name: String,
    pub workload_tags: BTreeSet<String>,
    pub evaluated_scenarios: Vec<String>,
    pub triggered_scenarios: Vec<String>,
    pub action_count: usize,
    pub rollback_count: usize,
    pub side_effect_count: usize,
    pub tracked_metrics: BTreeMap<MetricKind, MetricTrend>,
    pub notes: Vec<String>,
}

impl MetricRecord {
    pub fn metric(&self, kind: &MetricKind) -> Option<&MetricTrend> {
        self.tracked_metrics.get(kind)
    }
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct RecordInput {
    pub timestamp_ms: u64,
    pub pid: u32,
    pub process_name: String,
    pub workload_tags: BTreeSet<String>,
    pub evaluated_scenarios: Vec<String>,
    pub triggered_scenarios: Vec<String>,
    pub action_count: usize,
    pub rollback_count: usize,
    pub measurements: Vec<Measurement>,
    pub side_effects: Vec<SideEffect>,
    pub traces: Vec<MetricTrace>,
    pub notes: Vec<String>,
}

impl RecordInput {
    pub fn new(timestamp_ms: u64, pid: u32, process_name: impl Into<String>) -> Self {
        Self {
            timestamp_ms,
            pid,
            process_name: process_name.into(),
            ..Self::default()
        }
    }

    pub fn with_workload_tags<I, S>(mut self, tags: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: AsRef<str>,
    {
        self.workload_tags = tags
            .into_iter()
            .map(|tag| tag.as_ref().to_string())
            .collect();
        self
    }

    pub fn with_evaluated_scenarios<I, S>(mut self, scenarios: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: AsRef<str>,
    {
        self.evaluated_scenarios = dedupe_strings(scenarios);
        self
    }

    pub fn with_triggered_scenarios<I, S>(mut self, scenarios: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: AsRef<str>,
    {
        self.triggered_scenarios = dedupe_strings(scenarios);
        self
    }

    pub fn with_action_count(mut self, action_count: usize) -> Self {
        self.action_count = action_count;
        self
    }

    pub fn with_rollback_count(mut self, rollback_count: usize) -> Self {
        self.rollback_count = rollback_count;
        self
    }

    pub fn with_measurements<I>(mut self, measurements: I) -> Self
    where
        I: IntoIterator<Item = Measurement>,
    {
        self.measurements = measurements.into_iter().collect();
        self
    }

    pub fn with_side_effects<I>(mut self, side_effects: I) -> Self
    where
        I: IntoIterator<Item = SideEffect>,
    {
        self.side_effects = side_effects.into_iter().collect();
        self
    }

    pub fn with_traces<I>(mut self, traces: I) -> Self
    where
        I: IntoIterator<Item = MetricTrace>,
    {
        self.traces = traces.into_iter().collect();
        self
    }

    pub fn with_notes<I, S>(mut self, notes: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: AsRef<str>,
    {
        self.notes = notes
            .into_iter()
            .map(|note| note.as_ref().to_string())
            .collect();
        self
    }
}

fn dedupe_strings<I, S>(items: I) -> Vec<String>
where
    I: IntoIterator<Item = S>,
    S: AsRef<str>,
{
    let mut unique = Vec::new();
    for item in items {
        let item = item.as_ref().to_string();
        if !unique.contains(&item) {
            unique.push(item);
        }
    }
    unique
}
