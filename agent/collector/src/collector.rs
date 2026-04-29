use std::collections::{HashMap, VecDeque};
use std::fmt;

use crate::config::{AggregationScope, CollectorConfig};
use crate::event::{Event, EventKind, TimestampMicros};
use crate::summary::{CounterSummary, SampleAccumulator, ValueSummary};

/// Stable target identifier attached to an emitted feature window.
#[derive(Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum AggregationTarget {
    Thread { pid: u32, tid: u32 },
    Process { pid: u32 },
    Cgroup { cgroup_id: u64 },
}

/// Windowed feature view exposed to downstream modules.
#[derive(Clone, Debug, PartialEq)]
pub struct FeatureWindow {
    pub target: AggregationTarget,
    pub window_start: TimestampMicros,
    pub window_end: TimestampMicros,
    pub window_size_us: u64,
    pub observed_events: u64,
    pub run_queue_delay: ValueSummary,
    pub off_cpu: ValueSummary,
    pub io_latency: ValueSummary,
    pub subprocess_start_delay: ValueSummary,
    pub queue_wait: ValueSummary,
    pub cpu_migrations: CounterSummary,
    pub major_page_faults: CounterSummary,
}

impl FeatureWindow {
    pub fn empty(
        target: AggregationTarget,
        window_end: TimestampMicros,
        window_size_us: u64,
    ) -> Self {
        let window_start = window_end.saturating_sub(window_size_us);

        Self {
            target,
            window_start,
            window_end,
            window_size_us,
            observed_events: 0,
            run_queue_delay: ValueSummary::empty(),
            off_cpu: ValueSummary::empty(),
            io_latency: ValueSummary::empty(),
            subprocess_start_delay: ValueSummary::empty(),
            queue_wait: ValueSummary::empty(),
            cpu_migrations: CounterSummary::new(0, window_size_us),
            major_page_faults: CounterSummary::new(0, window_size_us),
        }
    }
}

/// Collector-level counters for observability and safety checks.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct CollectorStats {
    pub ingested_events: u64,
    pub filtered_noise_events: u64,
    pub dropped_late_events: u64,
    pub emitted_windows: u64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CollectorError {
    InvalidWindowSize,
    InvalidRecentEventRetention,
    EmptyScopeSet,
}

impl fmt::Display for CollectorError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidWindowSize => {
                write!(f, "collector window_size_us must be greater than 0")
            }
            Self::InvalidRecentEventRetention => {
                write!(
                    f,
                    "collector recent_event_retention_us must be greater than 0"
                )
            }
            Self::EmptyScopeSet => write!(f, "collector must have at least one aggregation scope"),
        }
    }
}

impl std::error::Error for CollectorError {}

/// Transforms raw probe events into fixed-size feature windows.
pub struct Collector {
    config: CollectorConfig,
    watermark: TimestampMicros,
    stats: CollectorStats,
    windows: HashMap<WindowKey, WindowAccumulator>,
    recent_events_by_pid: HashMap<u32, VecDeque<Event>>,
}

impl Collector {
    pub fn new(mut config: CollectorConfig) -> Result<Self, CollectorError> {
        if config.window_size_us == 0 {
            return Err(CollectorError::InvalidWindowSize);
        }

        if config.recent_event_retention_us == 0 {
            return Err(CollectorError::InvalidRecentEventRetention);
        }

        let mut scopes = Vec::new();
        for scope in config.scopes.drain(..) {
            if !scopes.contains(&scope) {
                scopes.push(scope);
            }
        }

        if scopes.is_empty() {
            return Err(CollectorError::EmptyScopeSet);
        }

        config.scopes = scopes;

        Ok(Self {
            config,
            watermark: TimestampMicros::default(),
            stats: CollectorStats::default(),
            windows: HashMap::new(),
            recent_events_by_pid: HashMap::new(),
        })
    }

    pub fn config(&self) -> &CollectorConfig {
        &self.config
    }

    pub fn stats(&self) -> CollectorStats {
        self.stats
    }

    pub fn active_window_count(&self) -> usize {
        self.windows.len()
    }

    pub fn process_window(
        &mut self,
        pid: u32,
        now: TimestampMicros,
        trailing_window_us: u64,
    ) -> FeatureWindow {
        let target = AggregationTarget::Process { pid };
        if trailing_window_us == 0 {
            return FeatureWindow::empty(target, now, 0);
        }

        let retention_us = self
            .config
            .recent_event_retention_us
            .max(trailing_window_us);
        let retention_cutoff = now.saturating_sub(retention_us);
        self.prune_recent_events_for_pid(pid, retention_cutoff);

        let Some(stream) = self.recent_events_by_pid.get(&pid) else {
            return FeatureWindow::empty(target, now, trailing_window_us);
        };

        let window_start = now.saturating_sub(trailing_window_us);
        let mut accumulator = WindowAccumulator::new(target, window_start);

        for event in stream {
            if event.timestamp >= window_start && event.timestamp <= now {
                accumulator.record(&event.kind);
            }
        }

        let mut feature_window = accumulator.finalize(trailing_window_us);
        feature_window.window_end = now;
        feature_window
    }

    /// Ingests a single event and emits any windows closed by the updated watermark.
    pub fn ingest(&mut self, event: Event) -> Vec<FeatureWindow> {
        self.stats.ingested_events += 1;

        if self.is_late(&event) {
            self.stats.dropped_late_events += 1;
            return self.flush_closed_windows();
        }

        self.advance_watermark(event.timestamp);

        if self.should_filter(&event.kind) {
            self.stats.filtered_noise_events += 1;
            return self.flush_closed_windows();
        }

        self.push_recent_event(event.clone());

        let window_start = self.window_start_for(event.timestamp);

        for scope in &self.config.scopes {
            if let Some(target) = AggregationTarget::from_event(*scope, &event) {
                let key = WindowKey {
                    target: target.clone(),
                    window_start,
                };
                let accumulator = self
                    .windows
                    .entry(key)
                    .or_insert_with(|| WindowAccumulator::new(target, window_start));

                accumulator.record(&event.kind);
            }
        }

        self.flush_closed_windows()
    }

    /// Emits all windows whose end time is less than or equal to the given watermark.
    pub fn flush_until(&mut self, watermark: TimestampMicros) -> Vec<FeatureWindow> {
        self.advance_watermark(watermark);
        self.flush_closed_windows()
    }

    /// Forces all open windows to be emitted, typically during shutdown.
    pub fn finish(&mut self) -> Vec<FeatureWindow> {
        let mut ready: Vec<_> = self
            .windows
            .drain()
            .map(|(_, accumulator)| accumulator.finalize(self.config.window_size_us))
            .collect();
        ready.sort_by(feature_window_sort_key);
        self.stats.emitted_windows += ready.len() as u64;
        ready
    }

    fn push_recent_event(&mut self, event: Event) {
        let pid = event.pid;
        self.recent_events_by_pid
            .entry(pid)
            .or_default()
            .push_back(event);

        let retention_cutoff = self
            .watermark
            .saturating_sub(self.config.recent_event_retention_us);
        self.prune_recent_events_for_pid(pid, retention_cutoff);
    }

    fn prune_recent_events_for_pid(&mut self, pid: u32, cutoff: TimestampMicros) {
        let mut should_remove = false;

        if let Some(stream) = self.recent_events_by_pid.get_mut(&pid) {
            while matches!(stream.front(), Some(event) if event.timestamp < cutoff) {
                stream.pop_front();
            }

            should_remove = stream.is_empty();
        }

        if should_remove {
            self.recent_events_by_pid.remove(&pid);
        }
    }

    fn is_late(&self, event: &Event) -> bool {
        if self.stats.ingested_events == 1 {
            return false;
        }

        let lateness_boundary = self
            .watermark
            .saturating_sub(self.config.allowed_lateness_us);
        let event_window_end = self
            .window_start_for(event.timestamp)
            .as_u64()
            .saturating_add(self.config.window_size_us);

        event_window_end <= lateness_boundary.as_u64()
    }

    fn should_filter(&self, kind: &EventKind) -> bool {
        match kind {
            EventKind::RunQueueDelay { delay_us } => {
                *delay_us < self.config.noise_filter.min_run_queue_delay_us
            }
            EventKind::OffCpu { duration_us } => {
                *duration_us < self.config.noise_filter.min_offcpu_us
            }
            EventKind::IoLatency { latency_us } => {
                *latency_us < self.config.noise_filter.min_io_latency_us
            }
            EventKind::CpuMigration { .. }
            | EventKind::MajorPageFault { .. }
            | EventKind::SubprocessStartDelay { .. }
            | EventKind::QueueWait { .. } => false,
        }
    }

    fn advance_watermark(&mut self, timestamp: TimestampMicros) {
        if timestamp > self.watermark {
            self.watermark = timestamp;
        }
    }

    fn window_start_for(&self, timestamp: TimestampMicros) -> TimestampMicros {
        TimestampMicros::new(
            (timestamp.as_u64() / self.config.window_size_us) * self.config.window_size_us,
        )
    }

    fn flush_closed_windows(&mut self) -> Vec<FeatureWindow> {
        let close_before = self
            .watermark
            .saturating_sub(self.config.allowed_lateness_us)
            .as_u64();

        let mut ready_keys = Vec::new();
        for key in self.windows.keys() {
            let window_end = key
                .window_start
                .as_u64()
                .saturating_add(self.config.window_size_us);
            if window_end <= close_before {
                ready_keys.push(key.clone());
            }
        }

        let mut ready = Vec::with_capacity(ready_keys.len());
        for key in ready_keys {
            if let Some(accumulator) = self.windows.remove(&key) {
                ready.push(accumulator.finalize(self.config.window_size_us));
            }
        }

        ready.sort_by(feature_window_sort_key);
        self.stats.emitted_windows += ready.len() as u64;
        ready
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
struct WindowKey {
    target: AggregationTarget,
    window_start: TimestampMicros,
}

struct WindowAccumulator {
    target: AggregationTarget,
    window_start: TimestampMicros,
    observed_events: u64,
    run_queue_delay: SampleAccumulator,
    off_cpu: SampleAccumulator,
    io_latency: SampleAccumulator,
    subprocess_start_delay: SampleAccumulator,
    queue_wait: SampleAccumulator,
    cpu_migrations: u64,
    major_page_faults: u64,
}

impl WindowAccumulator {
    fn new(target: AggregationTarget, window_start: TimestampMicros) -> Self {
        Self {
            target,
            window_start,
            observed_events: 0,
            run_queue_delay: SampleAccumulator::default(),
            off_cpu: SampleAccumulator::default(),
            io_latency: SampleAccumulator::default(),
            subprocess_start_delay: SampleAccumulator::default(),
            queue_wait: SampleAccumulator::default(),
            cpu_migrations: 0,
            major_page_faults: 0,
        }
    }

    fn record(&mut self, kind: &EventKind) {
        self.observed_events += 1;

        match kind {
            EventKind::RunQueueDelay { delay_us } => self.run_queue_delay.record(*delay_us),
            EventKind::OffCpu { duration_us } => self.off_cpu.record(*duration_us),
            EventKind::CpuMigration { count } => self.cpu_migrations += *count,
            EventKind::MajorPageFault { count } => self.major_page_faults += *count,
            EventKind::IoLatency { latency_us } => self.io_latency.record(*latency_us),
            EventKind::SubprocessStartDelay { delay_us } => {
                self.subprocess_start_delay.record(*delay_us)
            }
            EventKind::QueueWait { wait_us } => self.queue_wait.record(*wait_us),
        }
    }

    fn finalize(self, window_size_us: u64) -> FeatureWindow {
        FeatureWindow {
            target: self.target,
            window_start: self.window_start,
            window_end: TimestampMicros::new(
                self.window_start.as_u64().saturating_add(window_size_us),
            ),
            window_size_us,
            observed_events: self.observed_events,
            run_queue_delay: self.run_queue_delay.finalize(),
            off_cpu: self.off_cpu.finalize(),
            io_latency: self.io_latency.finalize(),
            subprocess_start_delay: self.subprocess_start_delay.finalize(),
            queue_wait: self.queue_wait.finalize(),
            cpu_migrations: CounterSummary::new(self.cpu_migrations, window_size_us),
            major_page_faults: CounterSummary::new(self.major_page_faults, window_size_us),
        }
    }
}

impl AggregationTarget {
    fn from_event(scope: AggregationScope, event: &Event) -> Option<Self> {
        match scope {
            AggregationScope::Thread => Some(Self::Thread {
                pid: event.pid,
                tid: event.tid,
            }),
            AggregationScope::Process => Some(Self::Process { pid: event.pid }),
            AggregationScope::Cgroup => event.cgroup_id.map(|cgroup_id| Self::Cgroup { cgroup_id }),
        }
    }
}

fn feature_window_sort_key(left: &FeatureWindow, right: &FeatureWindow) -> std::cmp::Ordering {
    left.window_start
        .cmp(&right.window_start)
        .then_with(|| left.target.cmp(&right.target))
}

#[cfg(test)]
mod tests {
    use super::{AggregationTarget, Collector, CollectorError};
    use crate::{
        AggregationScope, CollectorConfig, Event, EventKind, NoiseFilter, ProbeSource,
        TimestampMicros,
    };

    #[test]
    fn rejects_invalid_configuration() {
        let config = CollectorConfig {
            window_size_us: 0,
            ..CollectorConfig::default()
        };
        assert!(matches!(
            Collector::new(config),
            Err(CollectorError::InvalidWindowSize)
        ));

        let config = CollectorConfig {
            recent_event_retention_us: 0,
            ..CollectorConfig::default()
        };
        assert!(matches!(
            Collector::new(config),
            Err(CollectorError::InvalidRecentEventRetention)
        ));

        let config = CollectorConfig::default().with_scopes(Vec::new());
        assert!(matches!(
            Collector::new(config),
            Err(CollectorError::EmptyScopeSet)
        ));
    }

    #[test]
    fn aggregates_and_flushes_across_scopes() {
        let config = CollectorConfig::default().with_scopes(vec![
            AggregationScope::Process,
            AggregationScope::Thread,
            AggregationScope::Cgroup,
        ]);
        let mut collector = Collector::new(config).expect("valid collector");

        assert!(collector
            .ingest(event(1_000, EventKind::RunQueueDelay { delay_us: 100 }))
            .is_empty());
        assert!(collector
            .ingest(event(2_000, EventKind::RunQueueDelay { delay_us: 300 }))
            .is_empty());
        assert!(collector
            .ingest(event(3_000, EventKind::OffCpu { duration_us: 800 }))
            .is_empty());
        assert!(collector
            .ingest(event(4_000, EventKind::CpuMigration { count: 2 }))
            .is_empty());
        assert!(collector
            .ingest(event(5_000, EventKind::MajorPageFault { count: 1 }))
            .is_empty());
        assert!(collector
            .ingest(event(6_000, EventKind::IoLatency { latency_us: 1_200 }))
            .is_empty());

        let flushed = collector.ingest(event(500_000, EventKind::RunQueueDelay { delay_us: 999 }));

        assert_eq!(flushed.len(), 3);
        for window in &flushed {
            assert_eq!(window.window_start, TimestampMicros::new(0));
            assert_eq!(window.window_end, TimestampMicros::new(500_000));
            assert_eq!(window.observed_events, 6);
            assert_eq!(window.run_queue_delay.samples, 2);
            assert_eq!(window.run_queue_delay.mean, Some(200));
            assert_eq!(window.run_queue_delay.p95, Some(300));
            assert_eq!(window.off_cpu.max, Some(800));
            assert_eq!(window.io_latency.mean, Some(1_200));
            assert_eq!(window.cpu_migrations.total, 2);
            assert!((window.cpu_migrations.per_second - 4.0).abs() < f64::EPSILON);
            assert_eq!(window.major_page_faults.total, 1);
            assert!((window.major_page_faults.per_second - 2.0).abs() < f64::EPSILON);
            assert_eq!(window.subprocess_start_delay.samples, 0);
            assert_eq!(window.queue_wait.samples, 0);
        }

        let targets: Vec<_> = flushed.iter().map(|window| window.target.clone()).collect();
        assert!(targets.contains(&AggregationTarget::Cgroup { cgroup_id: 42 }));
        assert!(targets.contains(&AggregationTarget::Process { pid: 1000 }));
        assert!(targets.contains(&AggregationTarget::Thread {
            pid: 1000,
            tid: 2000,
        }));

        let remaining = collector.finish();
        assert_eq!(remaining.len(), 3);
        assert_eq!(collector.stats().emitted_windows, 6);
    }

    #[test]
    fn filters_noise_and_drops_late_events() {
        let mut config = CollectorConfig::default().with_scopes(vec![AggregationScope::Process]);
        config.window_size_us = 1_000;
        config.allowed_lateness_us = 100;
        config.noise_filter = NoiseFilter {
            min_run_queue_delay_us: 200,
            min_offcpu_us: 0,
            min_io_latency_us: 0,
        };

        let mut collector = Collector::new(config).expect("valid collector");

        assert!(collector
            .ingest(event(1_500, EventKind::RunQueueDelay { delay_us: 500 }))
            .is_empty());
        assert!(collector
            .ingest(event(1_600, EventKind::RunQueueDelay { delay_us: 100 }))
            .is_empty());

        let flushed = collector.ingest(event(100, EventKind::MajorPageFault { count: 1 }));
        assert_eq!(flushed.len(), 0);

        let stats = collector.stats();
        assert_eq!(stats.ingested_events, 3);
        assert_eq!(stats.filtered_noise_events, 1);
        assert_eq!(stats.dropped_late_events, 1);

        let finished = collector.finish();
        assert_eq!(finished.len(), 1);
        assert_eq!(finished[0].observed_events, 1);
        assert_eq!(finished[0].run_queue_delay.samples, 1);
        assert_eq!(finished[0].major_page_faults.total, 0);
    }

    #[test]
    fn projects_trailing_process_window_for_runtime_control_loop() {
        let config = CollectorConfig::default()
            .with_scopes(vec![AggregationScope::Process])
            .with_recent_event_retention_us(1_000_000);
        let mut collector = Collector::new(config).expect("valid collector");

        collector.ingest(event(
            50_000,
            EventKind::SubprocessStartDelay { delay_us: 1_400 },
        ));
        collector.ingest(event(120_000, EventKind::QueueWait { wait_us: 2_200 }));
        collector.ingest(event(180_000, EventKind::IoLatency { latency_us: 3_300 }));

        let window = collector.process_window(1000, TimestampMicros::new(200_000), 150_000);

        assert_eq!(window.target, AggregationTarget::Process { pid: 1000 });
        assert_eq!(window.observed_events, 3);
        assert_eq!(window.queue_wait.max, Some(2_200));
        assert_eq!(window.subprocess_start_delay.max, Some(1_400));
        assert_eq!(window.io_latency.max, Some(3_300));
    }

    fn event(timestamp: u64, kind: EventKind) -> Event {
        let probe = match &kind {
            EventKind::RunQueueDelay { .. } | EventKind::CpuMigration { .. } => ProbeSource::Sched,
            EventKind::OffCpu { .. } => ProbeSource::OffCpu,
            EventKind::MajorPageFault { .. } => ProbeSource::Fault,
            EventKind::IoLatency { .. } => ProbeSource::Io,
            EventKind::SubprocessStartDelay { .. } | EventKind::QueueWait { .. } => {
                ProbeSource::Runtime
            }
        };

        Event::new(
            TimestampMicros::new(timestamp),
            1000,
            2000,
            Some(42),
            probe,
            kind,
        )
    }
}
