use std::fmt;

use crate::MetricKind;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MetricsConfig {
    pub tracked_metrics: Vec<MetricKind>,
    pub baseline_window: usize,
    pub max_records: usize,
    pub max_traces: usize,
}

impl Default for MetricsConfig {
    fn default() -> Self {
        Self {
            tracked_metrics: vec![
                MetricKind::Ttft,
                MetricKind::P95Latency,
                MetricKind::P99Latency,
                MetricKind::Jitter,
                MetricKind::BoostHitRate,
                MetricKind::RollbackCount,
                MetricKind::SideEffectRate,
            ],
            baseline_window: 8,
            max_records: 1_024,
            max_traces: 4_096,
        }
    }
}

impl MetricsConfig {
    pub fn with_tracked_metrics(mut self, tracked_metrics: Vec<MetricKind>) -> Self {
        self.tracked_metrics = dedupe_metrics(tracked_metrics);
        self
    }

    pub fn tracks(&self, kind: &MetricKind) -> bool {
        self.tracked_metrics.is_empty() || self.tracked_metrics.contains(kind)
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum MetricsConfigError {
    InvalidBaselineWindow,
    InvalidRecordCapacity,
    InvalidTraceCapacity,
}

impl fmt::Display for MetricsConfigError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidBaselineWindow => {
                write!(f, "metrics baseline_window must be greater than 0")
            }
            Self::InvalidRecordCapacity => {
                write!(f, "metrics max_records must be greater than 0")
            }
            Self::InvalidTraceCapacity => {
                write!(f, "metrics max_traces must be greater than 0")
            }
        }
    }
}

impl std::error::Error for MetricsConfigError {}

pub(crate) fn validate_config(config: &mut MetricsConfig) -> Result<(), MetricsConfigError> {
    if config.baseline_window == 0 {
        return Err(MetricsConfigError::InvalidBaselineWindow);
    }
    if config.max_records == 0 {
        return Err(MetricsConfigError::InvalidRecordCapacity);
    }
    if config.max_traces == 0 {
        return Err(MetricsConfigError::InvalidTraceCapacity);
    }

    config.tracked_metrics = dedupe_metrics(std::mem::take(&mut config.tracked_metrics));
    Ok(())
}

fn dedupe_metrics(metrics: Vec<MetricKind>) -> Vec<MetricKind> {
    let mut unique = Vec::new();
    for metric in metrics {
        if !unique.contains(&metric) {
            unique.push(metric);
        }
    }
    unique
}
