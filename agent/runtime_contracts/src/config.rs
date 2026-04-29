use crate::model::{PinStrategy, ScenarioKind};

#[derive(Clone, Debug, PartialEq)]
pub struct SafetyConfig {
    pub require_revert: bool,
    pub allow_background_throttle: bool,
    pub max_priority_delta: i32,
    pub max_boost_duration_ms: u64,
    pub max_affinity_change_ratio: f32,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct TriggerThresholds {
    pub run_queue_delay_us: Option<u64>,
    pub offcpu_spike_us: Option<u64>,
    pub cpu_migrations_per_sec: Option<u64>,
    pub major_page_faults_per_sec: Option<u64>,
    pub subprocess_start_delay_us: Option<u64>,
    pub queue_wait_us: Option<u64>,
    pub optional_io_latency_us: Option<u64>,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct ScenarioActions {
    pub raise_nice: Option<i32>,
    pub pin_strategy: Option<PinStrategy>,
    pub use_cpuset: Option<bool>,
    pub warmup_executor: Option<bool>,
}

impl ScenarioActions {
    pub fn is_empty(&self) -> bool {
        self.raise_nice.is_none()
            && self.pin_strategy.is_none()
            && self.use_cpuset.is_none()
            && !self.warmup_executor.unwrap_or(false)
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct ScenarioPolicy {
    pub scenario: ScenarioKind,
    pub enabled: bool,
    pub evaluation_window_ms: u64,
    pub cooldown_ms: u64,
    pub max_boost_duration_ms: u64,
    pub triggers: TriggerThresholds,
    pub actions: ScenarioActions,
}

impl ScenarioPolicy {
    pub fn priority(&self) -> u16 {
        self.scenario.priority()
    }
}
