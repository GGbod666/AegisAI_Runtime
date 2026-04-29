use std::collections::{BTreeMap, BTreeSet};

pub use aegisai_classifier::{
    LatencySensitivity, OwnershipScope, StageLabel, WorkloadClass, WorkloadTag,
};

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum ScenarioKind {
    AiWorkloadAwareness,
    InferenceTailGuard,
    ToolCallBooster,
    Unknown(String),
}

impl ScenarioKind {
    pub fn parse(raw: &str) -> Self {
        match raw.trim() {
            "ai_workload_awareness" => Self::AiWorkloadAwareness,
            "inference_tail_guard" => Self::InferenceTailGuard,
            "tool_call_booster" => Self::ToolCallBooster,
            other => Self::Unknown(other.to_string()),
        }
    }

    pub fn as_str(&self) -> &str {
        match self {
            Self::AiWorkloadAwareness => "ai_workload_awareness",
            Self::InferenceTailGuard => "inference_tail_guard",
            Self::ToolCallBooster => "tool_call_booster",
            Self::Unknown(value) => value.as_str(),
        }
    }

    pub fn priority(&self) -> u16 {
        match self {
            Self::InferenceTailGuard => 300,
            Self::ToolCallBooster => 200,
            Self::AiWorkloadAwareness => 100,
            Self::Unknown(_) => 0,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum SignalKind {
    RunQueueDelay,
    OffCpuTime,
    CpuMigration,
    MajorPageFault,
    SubprocessStartDelay,
    QueueWait,
    IoLatency,
    Unknown(String),
}

impl SignalKind {
    pub fn parse(raw: &str) -> Self {
        match raw.trim() {
            "run_queue_delay" => Self::RunQueueDelay,
            "offcpu_time" => Self::OffCpuTime,
            "cpu_migration" => Self::CpuMigration,
            "major_page_fault" => Self::MajorPageFault,
            "subprocess_start_delay" => Self::SubprocessStartDelay,
            "queue_wait" => Self::QueueWait,
            "io_latency" => Self::IoLatency,
            other => Self::Unknown(other.to_string()),
        }
    }

    pub fn as_str(&self) -> &str {
        match self {
            Self::RunQueueDelay => "run_queue_delay",
            Self::OffCpuTime => "offcpu_time",
            Self::CpuMigration => "cpu_migration",
            Self::MajorPageFault => "major_page_fault",
            Self::SubprocessStartDelay => "subprocess_start_delay",
            Self::QueueWait => "queue_wait",
            Self::IoLatency => "io_latency",
            Self::Unknown(value) => value.as_str(),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PinStrategy {
    PreferReservedCores,
    PreferLowContentionCores,
    Unknown(String),
}

impl PinStrategy {
    pub fn parse(raw: &str) -> Self {
        match raw.trim() {
            "prefer_reserved_cores" => Self::PreferReservedCores,
            "prefer_low_contention_cores" => Self::PreferLowContentionCores,
            other => Self::Unknown(other.to_string()),
        }
    }

    pub fn as_str(&self) -> &str {
        match self {
            Self::PreferReservedCores => "prefer_reserved_cores",
            Self::PreferLowContentionCores => "prefer_low_contention_cores",
            Self::Unknown(value) => value.as_str(),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Event {
    pub timestamp_ms: u64,
    pub pid: u32,
    pub tid: Option<u32>,
    pub process_name: String,
    pub cmdline: String,
    pub cgroup: Option<String>,
    pub tag_markers: BTreeSet<String>,
    pub parent_pid: Option<u32>,
    pub parent_process_name: Option<String>,
    pub parent_cmdline: Option<String>,
    pub signal: SignalKind,
    pub value: u64,
}

impl Event {
    pub fn new(
        timestamp_ms: u64,
        pid: u32,
        process_name: impl Into<String>,
        signal: SignalKind,
        value: u64,
    ) -> Self {
        Self {
            timestamp_ms,
            pid,
            tid: None,
            process_name: process_name.into(),
            cmdline: String::new(),
            cgroup: None,
            tag_markers: BTreeSet::new(),
            parent_pid: None,
            parent_process_name: None,
            parent_cmdline: None,
            signal,
            value,
        }
    }

    pub fn with_cmdline(mut self, cmdline: impl Into<String>) -> Self {
        self.cmdline = cmdline.into();
        self
    }

    pub fn with_cgroup(mut self, cgroup: impl Into<String>) -> Self {
        self.cgroup = Some(cgroup.into());
        self
    }

    pub fn with_parent_process_name(mut self, process_name: impl Into<String>) -> Self {
        self.parent_process_name = Some(process_name.into());
        self
    }

    pub fn with_parent_pid(mut self, pid: u32) -> Self {
        self.parent_pid = Some(pid);
        self
    }

    pub fn with_parent_cmdline(mut self, cmdline: impl Into<String>) -> Self {
        self.parent_cmdline = Some(cmdline.into());
        self
    }

    pub fn with_tag_marker(mut self, tag: impl Into<String>) -> Self {
        self.tag_markers.insert(tag.into());
        self
    }

    pub fn with_tag_markers<I, S>(mut self, tags: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        self.tag_markers = tags.into_iter().map(Into::into).collect();
        self
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EventContext {
    pub timestamp_ms: u64,
    pub pid: u32,
    pub process_name: String,
}

impl EventContext {
    pub fn new(timestamp_ms: u64, pid: u32, process_name: impl Into<String>) -> Self {
        Self {
            timestamp_ms,
            pid,
            process_name: process_name.into(),
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct FeatureWindow {
    pub pid: u32,
    pub started_at_ms: u64,
    pub ended_at_ms: u64,
    pub sample_count: usize,
    pub run_queue_delay_us_max: u64,
    pub offcpu_time_us_max: u64,
    pub cpu_migrations_per_sec: u64,
    pub major_page_faults_per_sec: u64,
    pub subprocess_start_delay_us_max: u64,
    pub queue_wait_us_max: u64,
    pub optional_io_latency_us_max: u64,
}

impl FeatureWindow {
    pub fn empty(pid: u32, timestamp_ms: u64) -> Self {
        Self {
            pid,
            started_at_ms: timestamp_ms,
            ended_at_ms: timestamp_ms,
            ..Self::default()
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WorkloadProfile {
    pub pid: u32,
    pub tid: Option<u32>,
    pub process_name: String,
    pub scope: OwnershipScope,
    pub workload_class: WorkloadClass,
    pub stage: StageLabel,
    pub latency_sensitivity: LatencySensitivity,
    pub tags: BTreeSet<WorkloadTag>,
    pub matched_rules: Vec<String>,
}

impl WorkloadProfile {
    pub fn from_classifier(
        process_name: impl Into<String>,
        profile: aegisai_classifier::WorkloadProfile,
    ) -> Self {
        Self {
            pid: profile.pid,
            tid: profile.tid,
            process_name: process_name.into(),
            scope: profile.scope,
            workload_class: profile.workload_class,
            stage: profile.stage,
            latency_sensitivity: profile.latency_sensitivity,
            tags: profile.tags,
            matched_rules: profile.matched_rules,
        }
    }

    pub fn from_tags(
        pid: u32,
        tid: Option<u32>,
        process_name: impl Into<String>,
        tags: BTreeSet<WorkloadTag>,
        matched_rules: Vec<String>,
    ) -> Self {
        Self {
            pid,
            tid,
            process_name: process_name.into(),
            scope: OwnershipScope::Process,
            workload_class: WorkloadClass::from_tags(&tags),
            stage: StageLabel::from_tags(&tags),
            latency_sensitivity: LatencySensitivity::from_tags(&tags),
            tags,
            matched_rules,
        }
    }

    pub fn has_tag(&self, tag: &WorkloadTag) -> bool {
        self.tags.contains(tag)
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PolicyContext {
    pub scenario: ScenarioKind,
    pub event: EventContext,
    pub feature_window: FeatureWindow,
    pub profile: WorkloadProfile,
    pub audit_fields: BTreeMap<String, String>,
}

#[derive(Clone, Debug, PartialEq)]
pub enum Action {
    RaiseNice {
        delta: i32,
    },
    SetAffinity {
        strategy: PinStrategy,
        max_cpu_ratio: f32,
    },
    UseCpuset {
        enabled: bool,
    },
    WarmupExecutor,
}

#[derive(Clone, Debug, PartialEq)]
pub struct ActionPlan {
    pub scenario: ScenarioKind,
    pub target_pid: u32,
    pub target_process_name: String,
    pub actions: Vec<Action>,
    pub duration_ms: u64,
    pub rationale: Vec<String>,
    pub audit_fields: BTreeMap<String, String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum AppliedActionState {
    Applied,
    RolledBack,
}

#[derive(Clone, Debug, PartialEq)]
pub struct AppliedAction {
    pub scenario: ScenarioKind,
    pub target_pid: u32,
    pub target_process_name: String,
    pub actions: Vec<Action>,
    pub applied_at_ms: u64,
    pub expires_at_ms: u64,
    pub state: AppliedActionState,
    pub audit_fields: BTreeMap<String, String>,
}
