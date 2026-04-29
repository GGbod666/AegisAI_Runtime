use std::error::Error;
use std::fmt::{self, Display, Formatter};

use crate::event::EventKind;
use crate::filter::ProbeFilter;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum ProbeKind {
    Sched,
    OffCpu,
    Fault,
    Io,
}

impl ProbeKind {
    pub const ALL: [Self; 4] = [Self::Sched, Self::OffCpu, Self::Fault, Self::Io];

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Sched => "sched_probe",
            Self::OffCpu => "offcpu_probe",
            Self::Fault => "fault_probe",
            Self::Io => "io_probe",
        }
    }
}

impl Display for ProbeKind {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AttachPoint {
    TracePoint {
        category: &'static str,
        name: &'static str,
    },
    KProbe {
        function: &'static str,
    },
    KRetProbe {
        function: &'static str,
    },
    RawTracePoint {
        name: &'static str,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum ProbeCapability {
    TargetPid,
    TargetTid,
    TargetCgroup,
    SampleRateControl,
    RingBufferControl,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct OverheadBudget {
    pub ring_buffer_pages: u32,
    pub sample_every_n: u32,
    pub max_events_per_second: Option<u32>,
}

impl Default for OverheadBudget {
    fn default() -> Self {
        Self {
            ring_buffer_pages: 64,
            sample_every_n: 1,
            max_events_per_second: Some(10_000),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProbeConfig {
    pub enabled: bool,
    pub filter: ProbeFilter,
    pub budget: OverheadBudget,
}

impl Default for ProbeConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            filter: ProbeFilter::default(),
            budget: OverheadBudget::default(),
        }
    }
}

impl ProbeConfig {
    pub fn validate(&self) -> Result<(), ProbeConfigError> {
        if self.budget.ring_buffer_pages == 0 {
            return Err(ProbeConfigError::InvalidRingBufferPages);
        }

        if self.budget.sample_every_n == 0 {
            return Err(ProbeConfigError::InvalidSampleRate);
        }

        Ok(())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProbeConfigError {
    InvalidRingBufferPages,
    InvalidSampleRate,
}

impl Display for ProbeConfigError {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidRingBufferPages => {
                f.write_str("ring_buffer_pages must be greater than zero")
            }
            Self::InvalidSampleRate => f.write_str("sample_every_n must be greater than zero"),
        }
    }
}

impl Error for ProbeConfigError {}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProbeDescriptor {
    pub kind: ProbeKind,
    pub name: &'static str,
    pub attach_points: Vec<AttachPoint>,
    pub supported_events: Vec<EventKind>,
    pub capabilities: Vec<ProbeCapability>,
    pub default_config: ProbeConfig,
}

impl ProbeDescriptor {
    pub fn default_for(kind: ProbeKind) -> Self {
        match kind {
            ProbeKind::Sched => Self {
                kind,
                name: "sched_probe",
                attach_points: vec![
                    AttachPoint::TracePoint {
                        category: "sched",
                        name: "sched_wakeup",
                    },
                    AttachPoint::TracePoint {
                        category: "sched",
                        name: "sched_switch",
                    },
                    AttachPoint::TracePoint {
                        category: "sched",
                        name: "sched_migrate_task",
                    },
                ],
                supported_events: vec![
                    EventKind::RunQueueDelay,
                    EventKind::CpuMigration,
                    EventKind::ContextSwitch,
                ],
                capabilities: vec![
                    ProbeCapability::TargetPid,
                    ProbeCapability::TargetTid,
                    ProbeCapability::TargetCgroup,
                    ProbeCapability::SampleRateControl,
                    ProbeCapability::RingBufferControl,
                ],
                default_config: ProbeConfig::default(),
            },
            ProbeKind::OffCpu => Self {
                kind,
                name: "offcpu_probe",
                attach_points: vec![AttachPoint::TracePoint {
                    category: "sched",
                    name: "sched_switch",
                }],
                supported_events: vec![EventKind::OffCpuDuration],
                capabilities: vec![
                    ProbeCapability::TargetPid,
                    ProbeCapability::TargetTid,
                    ProbeCapability::TargetCgroup,
                    ProbeCapability::SampleRateControl,
                    ProbeCapability::RingBufferControl,
                ],
                default_config: ProbeConfig {
                    budget: OverheadBudget {
                        max_events_per_second: Some(20_000),
                        ..OverheadBudget::default()
                    },
                    ..ProbeConfig::default()
                },
            },
            ProbeKind::Fault => Self {
                kind,
                name: "fault_probe",
                attach_points: vec![
                    AttachPoint::KProbe {
                        function: "handle_mm_fault",
                    },
                    AttachPoint::KRetProbe {
                        function: "handle_mm_fault",
                    },
                ],
                supported_events: vec![EventKind::MajorPageFault, EventKind::MinorPageFault],
                capabilities: vec![
                    ProbeCapability::TargetPid,
                    ProbeCapability::TargetTid,
                    ProbeCapability::TargetCgroup,
                    ProbeCapability::SampleRateControl,
                    ProbeCapability::RingBufferControl,
                ],
                default_config: ProbeConfig {
                    budget: OverheadBudget {
                        max_events_per_second: Some(5_000),
                        ..OverheadBudget::default()
                    },
                    ..ProbeConfig::default()
                },
            },
            ProbeKind::Io => Self {
                kind,
                name: "io_probe",
                attach_points: vec![
                    AttachPoint::TracePoint {
                        category: "block",
                        name: "block_rq_issue",
                    },
                    AttachPoint::TracePoint {
                        category: "block",
                        name: "block_rq_complete",
                    },
                ],
                supported_events: vec![EventKind::BlockIoLatency, EventKind::IoBytes],
                capabilities: vec![
                    ProbeCapability::TargetPid,
                    ProbeCapability::TargetTid,
                    ProbeCapability::TargetCgroup,
                    ProbeCapability::SampleRateControl,
                    ProbeCapability::RingBufferControl,
                ],
                default_config: ProbeConfig {
                    budget: OverheadBudget {
                        max_events_per_second: Some(8_000),
                        ..OverheadBudget::default()
                    },
                    ..ProbeConfig::default()
                },
            },
        }
    }

    pub fn supports_event(&self, event: EventKind) -> bool {
        self.supported_events.contains(&event)
    }
}

#[cfg(test)]
mod tests {
    use super::{ProbeConfig, ProbeDescriptor, ProbeKind};
    use crate::event::EventKind;

    #[test]
    fn probe_config_rejects_zero_sample_rate() {
        let config = ProbeConfig {
            budget: super::OverheadBudget {
                sample_every_n: 0,
                ..super::OverheadBudget::default()
            },
            ..ProbeConfig::default()
        };

        assert!(config.validate().is_err());
    }

    #[test]
    fn sched_descriptor_contains_expected_event() {
        let descriptor = ProbeDescriptor::default_for(ProbeKind::Sched);

        assert!(descriptor.supports_event(EventKind::RunQueueDelay));
    }
}
