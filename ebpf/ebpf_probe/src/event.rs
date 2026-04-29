use std::error::Error;
use std::fmt::{self, Display, Formatter};

use crate::probe::ProbeKind;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum EventKind {
    RunQueueDelay,
    CpuMigration,
    ContextSwitch,
    OffCpuDuration,
    MajorPageFault,
    MinorPageFault,
    BlockIoLatency,
    IoBytes,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MetricUnit {
    DurationNs,
    Count,
    Bytes,
    Pages,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EventMetric {
    pub unit: MetricUnit,
    pub value: u64,
}

impl EventMetric {
    pub fn duration_ns(value: u64) -> Self {
        Self {
            unit: MetricUnit::DurationNs,
            value,
        }
    }

    pub fn count(value: u64) -> Self {
        Self {
            unit: MetricUnit::Count,
            value,
        }
    }

    pub fn bytes(value: u64) -> Self {
        Self {
            unit: MetricUnit::Bytes,
            value,
        }
    }

    pub fn pages(value: u64) -> Self {
        Self {
            unit: MetricUnit::Pages,
            value,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EventTarget {
    pub pid: u32,
    pub tid: u32,
    pub cgroup_id: Option<u64>,
    pub comm: String,
}

impl EventTarget {
    pub fn new(pid: u32, tid: u32, comm: impl Into<String>) -> Self {
        Self {
            pid,
            tid,
            cgroup_id: None,
            comm: comm.into(),
        }
    }

    pub fn with_cgroup_id(mut self, cgroup_id: u64) -> Self {
        self.cgroup_id = Some(cgroup_id);
        self
    }

    pub fn validate(&self) -> Result<(), EventValidationError> {
        if self.pid == 0 {
            return Err(EventValidationError::InvalidPid);
        }

        if self.tid == 0 {
            return Err(EventValidationError::InvalidTid);
        }

        if self.comm.trim().is_empty() {
            return Err(EventValidationError::MissingCommandName);
        }

        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Event {
    pub timestamp_ns: u64,
    pub cpu: Option<u32>,
    pub probe: ProbeKind,
    pub kind: EventKind,
    pub target: EventTarget,
    pub metric: EventMetric,
    pub sample_count: u32,
}

impl Event {
    pub fn new(
        timestamp_ns: u64,
        probe: ProbeKind,
        kind: EventKind,
        target: EventTarget,
        metric: EventMetric,
    ) -> Self {
        Self {
            timestamp_ns,
            cpu: None,
            probe,
            kind,
            target,
            metric,
            sample_count: 1,
        }
    }

    pub fn on_cpu(mut self, cpu: u32) -> Self {
        self.cpu = Some(cpu);
        self
    }

    pub fn with_sample_count(mut self, sample_count: u32) -> Self {
        self.sample_count = sample_count;
        self
    }

    pub fn validate(&self) -> Result<(), EventValidationError> {
        if self.timestamp_ns == 0 {
            return Err(EventValidationError::MissingTimestamp);
        }

        if self.sample_count == 0 {
            return Err(EventValidationError::InvalidSampleCount);
        }

        self.target.validate()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EventValidationError {
    MissingTimestamp,
    InvalidPid,
    InvalidTid,
    MissingCommandName,
    InvalidSampleCount,
}

impl Display for EventValidationError {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        match self {
            Self::MissingTimestamp => f.write_str("event timestamp must be greater than zero"),
            Self::InvalidPid => f.write_str("event target pid must be greater than zero"),
            Self::InvalidTid => f.write_str("event target tid must be greater than zero"),
            Self::MissingCommandName => {
                f.write_str("event target comm must contain a command name")
            }
            Self::InvalidSampleCount => f.write_str("event sample_count must be greater than zero"),
        }
    }
}

impl Error for EventValidationError {}

#[cfg(test)]
mod tests {
    use super::{Event, EventKind, EventMetric, EventTarget};
    use crate::probe::ProbeKind;

    #[test]
    fn event_validation_accepts_complete_event() {
        let event = Event::new(
            1,
            ProbeKind::Sched,
            EventKind::RunQueueDelay,
            EventTarget::new(100, 101, "ollama").with_cgroup_id(42),
            EventMetric::duration_ns(900),
        )
        .on_cpu(3)
        .with_sample_count(2);

        assert!(event.validate().is_ok());
    }

    #[test]
    fn event_validation_rejects_missing_timestamp() {
        let event = Event::new(
            0,
            ProbeKind::Sched,
            EventKind::RunQueueDelay,
            EventTarget::new(100, 101, "ollama"),
            EventMetric::duration_ns(900),
        );

        assert!(event.validate().is_err());
    }
}
