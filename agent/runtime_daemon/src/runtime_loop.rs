use std::collections::{BTreeMap, BTreeSet};
use std::fmt;

use crate::{enrich_source_event, EventSource, MetadataError, MetadataProvider, SourceError};
use runtime_orchestrator::RuntimeOrchestrator;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RuntimeLoopConfig {
    pub batch_size: usize,
    pub tick_interval_ms: u64,
    pub drain_after_source_ms: u64,
}

impl Default for RuntimeLoopConfig {
    fn default() -> Self {
        Self {
            batch_size: 32,
            tick_interval_ms: 200,
            drain_after_source_ms: 5_000,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RuntimeLoopError {
    InvalidBatchSize,
    Source(SourceError),
    Metadata(MetadataError),
}

impl fmt::Display for RuntimeLoopError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidBatchSize => write!(f, "runtime loop batch_size must be greater than 0"),
            Self::Source(error) => write!(f, "{error}"),
            Self::Metadata(error) => write!(f, "{error}"),
        }
    }
}

impl std::error::Error for RuntimeLoopError {}

impl From<SourceError> for RuntimeLoopError {
    fn from(value: SourceError) -> Self {
        Self::Source(value)
    }
}

impl From<MetadataError> for RuntimeLoopError {
    fn from(value: MetadataError) -> Self {
        Self::Metadata(value)
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct RuntimeRunSummary {
    pub source_name: String,
    pub metadata_provider_name: String,
    pub actuator_backend_name: String,
    pub processed_events: u64,
    pub applied_actions: u64,
    pub inline_rollbacks: u64,
    pub tick_rollbacks: u64,
    pub triggered_scenarios: BTreeMap<String, u64>,
    pub metric_records: usize,
    pub trace_records: usize,
    pub last_timestamp_ms: Option<u64>,
    pub audit_highlights: Vec<String>,
}

impl RuntimeRunSummary {
    pub fn total_rollbacks(&self) -> u64 {
        self.inline_rollbacks + self.tick_rollbacks
    }
}

pub struct RuntimeLoop {
    config: RuntimeLoopConfig,
}

impl RuntimeLoop {
    pub fn new(config: RuntimeLoopConfig) -> Result<Self, RuntimeLoopError> {
        if config.batch_size == 0 {
            return Err(RuntimeLoopError::InvalidBatchSize);
        }

        Ok(Self { config })
    }

    pub fn config(&self) -> &RuntimeLoopConfig {
        &self.config
    }

    pub fn run<S: EventSource, P: MetadataProvider>(
        &self,
        orchestrator: &mut RuntimeOrchestrator,
        source: &mut S,
        metadata_provider: &mut P,
    ) -> Result<RuntimeRunSummary, RuntimeLoopError> {
        let mut summary = RuntimeRunSummary {
            source_name: source.source_name().to_string(),
            metadata_provider_name: metadata_provider.provider_name().to_string(),
            actuator_backend_name: orchestrator.actuator_backend_name().to_string(),
            ..RuntimeRunSummary::default()
        };
        let mut audit_highlights = BTreeSet::new();
        let mut next_tick_at_ms = None;

        loop {
            let batch = source.poll_batch(self.config.batch_size)?;
            if batch.is_empty() {
                break;
            }

            for raw_event in batch {
                if self.config.tick_interval_ms > 0 {
                    if let Some(mut next_tick) = next_tick_at_ms {
                        while next_tick <= raw_event.timestamp_ms {
                            let rollbacks = orchestrator.tick(next_tick);
                            summary.tick_rollbacks += rollbacks.len() as u64;
                            collect_audit_highlights(&mut audit_highlights, &rollbacks);
                            next_tick = next_tick.saturating_add(self.config.tick_interval_ms);
                        }
                        next_tick_at_ms = Some(next_tick);
                    } else {
                        next_tick_at_ms = Some(
                            raw_event
                                .timestamp_ms
                                .saturating_add(self.config.tick_interval_ms),
                        );
                    }
                }

                let runtime_event = enrich_source_event(raw_event, metadata_provider)?;
                let timestamp_ms = runtime_event.timestamp_ms;
                let outcome = orchestrator.process_event(runtime_event);

                summary.processed_events += 1;
                summary.applied_actions += outcome.applied_actions.len() as u64;
                summary.inline_rollbacks += outcome.rollbacks.len() as u64;
                summary.last_timestamp_ms = Some(timestamp_ms);
                collect_audit_highlights(&mut audit_highlights, &outcome.applied_actions);
                collect_audit_highlights(&mut audit_highlights, &outcome.rollbacks);

                for action in outcome.applied_actions {
                    *summary
                        .triggered_scenarios
                        .entry(action.scenario.as_str().to_string())
                        .or_default() += 1;
                }
            }
        }

        if let Some(last_timestamp_ms) = summary.last_timestamp_ms {
            let rollbacks = orchestrator
                .tick(last_timestamp_ms.saturating_add(self.config.drain_after_source_ms));
            summary.tick_rollbacks += rollbacks.len() as u64;
            collect_audit_highlights(&mut audit_highlights, &rollbacks);
        }

        summary.metric_records = orchestrator.metrics().len();
        summary.trace_records = orchestrator.traces().len();
        summary.audit_highlights = audit_highlights.into_iter().collect();

        Ok(summary)
    }
}

fn collect_audit_highlights(
    highlights: &mut BTreeSet<String>,
    actions: &[runtime_orchestrator::AppliedAction],
) {
    const AUDIT_PREFIXES: [&str; 3] = [
        "backend.apply.capture.",
        "backend.apply.apply.",
        "backend.rollback.rollback.",
    ];

    for action in actions {
        for (key, value) in &action.audit_fields {
            if AUDIT_PREFIXES.iter().any(|prefix| key.starts_with(prefix)) {
                highlights.insert(format!(
                    "pid={};scenario={};{}={}",
                    action.target_pid,
                    action.scenario.as_str(),
                    key,
                    value
                ));
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use std::path::{Path, PathBuf};

    use aegisai_actuator::{
        Actuator, CommandLinuxSyscallApplier, LinuxActuatorBackend,
        PlannedOnlyLinuxSyscallExecutor, UnavailableLinuxProcessStateProvider,
    };
    use crate::{
        MockEventSource, NoopMetadataProvider, RuntimeLoop, RuntimeLoopConfig,
        StaticMetadataProvider,
    };
    use runtime_orchestrator::{RuntimeOrchestrator, RuntimeOrchestratorConfig};

    fn repo_root() -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .ancestors()
            .nth(2)
            .expect("crate lives under agent/runtime_daemon")
            .to_path_buf()
    }

    #[test]
    fn mock_runtime_loop_drives_orchestrator_end_to_end() {
        let mut orchestrator =
            RuntimeOrchestrator::from_repo_root(repo_root()).expect("config should load");
        let mut source = MockEventSource::demo_sequence();
        let mut metadata = StaticMetadataProvider::demo();
        let runtime_loop = RuntimeLoop::new(RuntimeLoopConfig::default()).expect("valid config");

        let summary = runtime_loop
            .run(&mut orchestrator, &mut source, &mut metadata)
            .expect("runtime loop should succeed");

        assert_eq!(summary.processed_events, 3);
        assert!(summary.applied_actions >= 2);
        assert!(summary.total_rollbacks() >= 1);
        assert!(summary
            .triggered_scenarios
            .contains_key("inference_tail_guard"));
        assert!(summary
            .triggered_scenarios
            .contains_key("tool_call_booster"));
        assert_eq!(summary.metric_records, orchestrator.metrics().len());
        assert_eq!(summary.trace_records, orchestrator.traces().len());
    }

    #[test]
    fn self_describing_mock_source_runs_without_metadata_enrichment() {
        let mut orchestrator =
            RuntimeOrchestrator::from_repo_root(repo_root()).expect("config should load");
        let mut source = MockEventSource::demo_sequence();
        let mut metadata = NoopMetadataProvider;
        let runtime_loop = RuntimeLoop::new(RuntimeLoopConfig::default()).expect("valid config");

        let summary = runtime_loop
            .run(&mut orchestrator, &mut source, &mut metadata)
            .expect("runtime loop should succeed");

        assert_eq!(summary.processed_events, 3);
        assert!(summary
            .triggered_scenarios
            .contains_key("inference_tail_guard"));
        assert!(summary
            .triggered_scenarios
            .contains_key("tool_call_booster"));
    }

    #[test]
    fn runtime_loop_collects_audit_highlights_from_backend_execution() {
        let config =
            RuntimeOrchestratorConfig::load_from_repo_root(repo_root()).expect("config should load");
        let executor = PlannedOnlyLinuxSyscallExecutor::with_state_provider_and_applier(
            UnavailableLinuxProcessStateProvider,
            CommandLinuxSyscallApplier::dry_run(),
        );
        let actuator = Actuator::with_backend(LinuxActuatorBackend::with_named_executor(
            "linux-command-dry-run",
            executor,
        ));
        let mut orchestrator =
            RuntimeOrchestrator::with_actuator(config, actuator).expect("orchestrator should init");
        let mut source = MockEventSource::demo_sequence();
        let mut metadata = StaticMetadataProvider::demo();
        let runtime_loop = RuntimeLoop::new(RuntimeLoopConfig::default()).expect("valid config");

        let summary = runtime_loop
            .run(&mut orchestrator, &mut source, &mut metadata)
            .expect("runtime loop should succeed");

        assert_eq!(summary.actuator_backend_name, "linux-command-dry-run");
        assert!(summary
            .audit_highlights
            .iter()
            .any(|highlight| highlight.contains("backend.apply.apply.result=")));
    }
}
