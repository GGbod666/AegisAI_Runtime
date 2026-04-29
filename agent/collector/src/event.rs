use std::fmt;

use ebpf_probe::{Event as ProbeEvent, EventKind as ProbeEventKind, MetricUnit, ProbeKind};

/// Monotonic microsecond timestamp used by the collector.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct TimestampMicros(u64);

impl TimestampMicros {
    pub const fn new(value: u64) -> Self {
        Self(value)
    }

    pub const fn as_u64(self) -> u64 {
        self.0
    }

    pub const fn saturating_sub(self, micros: u64) -> Self {
        Self(self.0.saturating_sub(micros))
    }
}

impl From<u64> for TimestampMicros {
    fn from(value: u64) -> Self {
        Self::new(value)
    }
}

impl fmt::Display for TimestampMicros {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}us", self.0)
    }
}

/// Source probe that emitted the raw event.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ProbeSource {
    Sched,
    OffCpu,
    Fault,
    Io,
    Runtime,
}

/// Low-level event payload produced by eBPF probes.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum EventKind {
    RunQueueDelay { delay_us: u64 },
    OffCpu { duration_us: u64 },
    CpuMigration { count: u64 },
    MajorPageFault { count: u64 },
    IoLatency { latency_us: u64 },
    SubprocessStartDelay { delay_us: u64 },
    QueueWait { wait_us: u64 },
}

/// Unified event model consumed by the collector.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Event {
    pub timestamp: TimestampMicros,
    pub pid: u32,
    pub tid: u32,
    pub cgroup_id: Option<u64>,
    pub probe: ProbeSource,
    pub kind: EventKind,
}

impl Event {
    pub const fn new(
        timestamp: TimestampMicros,
        pid: u32,
        tid: u32,
        cgroup_id: Option<u64>,
        probe: ProbeSource,
        kind: EventKind,
    ) -> Self {
        Self {
            timestamp,
            pid,
            tid,
            cgroup_id,
            probe,
            kind,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EventConversionError {
    UnsupportedKind,
    InvalidMetricUnit,
}

impl TryFrom<ProbeEvent> for Event {
    type Error = EventConversionError;

    fn try_from(event: ProbeEvent) -> Result<Self, Self::Error> {
        let kind = match event.kind {
            ProbeEventKind::RunQueueDelay => match event.metric.unit {
                MetricUnit::DurationNs => EventKind::RunQueueDelay {
                    delay_us: round_up_ns_to_us(event.metric.value),
                },
                _ => return Err(EventConversionError::InvalidMetricUnit),
            },
            ProbeEventKind::CpuMigration => match event.metric.unit {
                MetricUnit::Count => EventKind::CpuMigration {
                    count: event.metric.value,
                },
                _ => return Err(EventConversionError::InvalidMetricUnit),
            },
            ProbeEventKind::OffCpuDuration => match event.metric.unit {
                MetricUnit::DurationNs => EventKind::OffCpu {
                    duration_us: round_up_ns_to_us(event.metric.value),
                },
                _ => return Err(EventConversionError::InvalidMetricUnit),
            },
            ProbeEventKind::MajorPageFault => match event.metric.unit {
                MetricUnit::Count | MetricUnit::Pages => EventKind::MajorPageFault {
                    count: event.metric.value,
                },
                _ => return Err(EventConversionError::InvalidMetricUnit),
            },
            ProbeEventKind::BlockIoLatency => match event.metric.unit {
                MetricUnit::DurationNs => EventKind::IoLatency {
                    latency_us: round_up_ns_to_us(event.metric.value),
                },
                _ => return Err(EventConversionError::InvalidMetricUnit),
            },
            ProbeEventKind::ContextSwitch
            | ProbeEventKind::MinorPageFault
            | ProbeEventKind::IoBytes => {
                return Err(EventConversionError::UnsupportedKind);
            }
        };

        Ok(Self::new(
            TimestampMicros::new(round_up_ns_to_us(event.timestamp_ns)),
            event.target.pid,
            event.target.tid,
            event.target.cgroup_id,
            probe_source(event.probe),
            kind,
        ))
    }
}

fn round_up_ns_to_us(value_ns: u64) -> u64 {
    if value_ns == 0 {
        0
    } else {
        value_ns.saturating_add(999) / 1_000
    }
}

fn probe_source(kind: ProbeKind) -> ProbeSource {
    match kind {
        ProbeKind::Sched => ProbeSource::Sched,
        ProbeKind::OffCpu => ProbeSource::OffCpu,
        ProbeKind::Fault => ProbeSource::Fault,
        ProbeKind::Io => ProbeSource::Io,
    }
}
