/// Aggregation level emitted by the collector.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum AggregationScope {
    Thread,
    Process,
    Cgroup,
}

/// Lightweight noise filtering before events enter the window accumulator.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct NoiseFilter {
    pub min_run_queue_delay_us: u64,
    pub min_offcpu_us: u64,
    pub min_io_latency_us: u64,
}

/// Runtime configuration for the collector.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CollectorConfig {
    pub window_size_us: u64,
    pub allowed_lateness_us: u64,
    pub recent_event_retention_us: u64,
    pub scopes: Vec<AggregationScope>,
    pub noise_filter: NoiseFilter,
}

impl CollectorConfig {
    pub fn with_scopes(mut self, scopes: Vec<AggregationScope>) -> Self {
        self.scopes = scopes;
        self
    }

    pub fn with_recent_event_retention_us(mut self, recent_event_retention_us: u64) -> Self {
        self.recent_event_retention_us = recent_event_retention_us;
        self
    }
}

impl Default for CollectorConfig {
    fn default() -> Self {
        Self {
            window_size_us: 500_000,
            allowed_lateness_us: 0,
            recent_event_retention_us: 2_000_000,
            scopes: vec![
                AggregationScope::Process,
                AggregationScope::Thread,
                AggregationScope::Cgroup,
            ],
            noise_filter: NoiseFilter::default(),
        }
    }
}
