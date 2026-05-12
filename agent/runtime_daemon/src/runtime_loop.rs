use std::collections::{BTreeMap, BTreeSet};
use std::fmt;

use crate::{enrich_source_event, EventSource, MetadataError, MetadataProvider, SourceError};
use runtime_orchestrator::RuntimeOrchestrator;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RuntimeLoopConfig {
    pub batch_size: usize,
    pub tick_interval_ms: u64,
    pub drain_after_source_ms: u64,
    pub max_events: Option<u64>,
}

impl Default for RuntimeLoopConfig {
    fn default() -> Self {
        Self {
            batch_size: 32,
            tick_interval_ms: 200,
            drain_after_source_ms: 5_000,
            max_events: None,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RuntimeLoopError {
    InvalidBatchSize,
    InvalidMaxEvents,
    Source(SourceError),
    Metadata(MetadataError),
}

impl fmt::Display for RuntimeLoopError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidBatchSize => write!(f, "runtime loop batch_size must be greater than 0"),
            Self::InvalidMaxEvents => write!(f, "runtime loop max_events must be greater than 0"),
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
    pub tool_call_lifecycles: Vec<ToolCallLifecycleSummary>,
    pub signal_observations: BTreeMap<String, SignalObservationSummary>,
    pub feature_window_maxima: BTreeMap<String, u64>,
    pub source_diagnostics: Vec<String>,
}

impl RuntimeRunSummary {
    pub fn total_rollbacks(&self) -> u64 {
        self.inline_rollbacks + self.tick_rollbacks
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct SignalObservationSummary {
    pub event_count: u64,
    pub value_total: u64,
    pub value_max: u64,
}

impl SignalObservationSummary {
    fn record(&mut self, value: u64) {
        self.event_count = self.event_count.saturating_add(1);
        self.value_total = self.value_total.saturating_add(value);
        self.value_max = self.value_max.max(value);
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ToolCallLifecycleSummary {
    pub lifecycle_id: String,
    pub started_at_ms: u64,
    pub ended_at_ms: u64,
    pub stages: BTreeMap<String, u64>,
    pub boosted_actions: u64,
    pub rollback_actions: u64,
    pub background_events: u64,
    pub isolation_events: u64,
    pub target_pids: BTreeSet<u32>,
}

impl ToolCallLifecycleSummary {
    pub fn duration_ms(&self) -> u64 {
        self.ended_at_ms.saturating_sub(self.started_at_ms)
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
        if config.max_events == Some(0) {
            return Err(RuntimeLoopError::InvalidMaxEvents);
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
        let mut lifecycle_tracker = ToolCallLifecycleTracker::default();
        let mut next_tick_at_ms = None;

        'outer: loop {
            let batch_size = self
                .config
                .max_events
                .map(|max_events| {
                    max_events
                        .saturating_sub(summary.processed_events)
                        .min(self.config.batch_size as u64) as usize
                })
                .unwrap_or(self.config.batch_size);
            if batch_size == 0 {
                break;
            }

            let batch = source.poll_batch(batch_size)?;
            if batch.is_empty() {
                break;
            }

            for raw_event in batch {
                if self.config.tick_interval_ms > 0 {
                    if let Some(mut next_tick) = next_tick_at_ms {
                        while next_tick <= raw_event.timestamp_ms {
                            let rollbacks = orchestrator.tick(next_tick);
                            summary.tick_rollbacks += rollbacks.len() as u64;
                            lifecycle_tracker.observe_actions(&rollbacks);
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
                lifecycle_tracker.observe_event(&runtime_event);
                record_signal_observation(&mut summary.signal_observations, &runtime_event);
                let outcome = orchestrator.process_event(runtime_event);
                record_feature_window_maxima(
                    &mut summary.feature_window_maxima,
                    outcome.feature_windows.values(),
                );
                lifecycle_tracker.observe_actions(&outcome.applied_actions);
                lifecycle_tracker.observe_actions(&outcome.rollbacks);

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

                if self
                    .config
                    .max_events
                    .is_some_and(|max_events| summary.processed_events >= max_events)
                {
                    break 'outer;
                }
            }
        }

        if let Some(last_timestamp_ms) = summary.last_timestamp_ms {
            let rollbacks = orchestrator
                .tick(last_timestamp_ms.saturating_add(self.config.drain_after_source_ms));
            summary.tick_rollbacks += rollbacks.len() as u64;
            lifecycle_tracker.observe_actions(&rollbacks);
            collect_audit_highlights(&mut audit_highlights, &rollbacks);
        }

        summary.metric_records = orchestrator.metrics().len();
        summary.trace_records = orchestrator.traces().len();
        summary.audit_highlights = audit_highlights.into_iter().collect();
        summary.tool_call_lifecycles = lifecycle_tracker.finish();
        summary.source_diagnostics = source.diagnostics();

        Ok(summary)
    }
}

fn record_signal_observation(
    observations: &mut BTreeMap<String, SignalObservationSummary>,
    event: &runtime_orchestrator::Event,
) {
    observations
        .entry(event.signal.as_str().to_string())
        .or_default()
        .record(event.value);
}

fn record_feature_window_maxima<'a, I>(maxima: &mut BTreeMap<String, u64>, windows: I)
where
    I: IntoIterator<Item = &'a runtime_orchestrator::FeatureWindow>,
{
    for window in windows {
        update_maximum(
            maxima,
            "run_queue_delay_us_max",
            window.run_queue_delay_us_max,
        );
        update_maximum(maxima, "offcpu_time_us_max", window.offcpu_time_us_max);
        update_maximum(
            maxima,
            "cpu_migrations_per_sec",
            window.cpu_migrations_per_sec,
        );
        update_maximum(
            maxima,
            "major_page_faults_per_sec",
            window.major_page_faults_per_sec,
        );
        update_maximum(
            maxima,
            "subprocess_start_delay_us_max",
            window.subprocess_start_delay_us_max,
        );
        update_maximum(maxima, "queue_wait_us_max", window.queue_wait_us_max);
        update_maximum(
            maxima,
            "optional_io_latency_us_max",
            window.optional_io_latency_us_max,
        );
    }
}

fn update_maximum(maxima: &mut BTreeMap<String, u64>, key: &str, value: u64) {
    let current = maxima.entry(key.to_string()).or_default();
    *current = (*current).max(value);
}

#[derive(Default)]
struct ToolCallLifecycleTracker {
    lifecycles: BTreeMap<String, ToolCallLifecycleSummary>,
}

impl ToolCallLifecycleTracker {
    fn observe_event(&mut self, event: &runtime_orchestrator::Event) {
        let Some(lifecycle_id) = tool_call_lifecycle_id(event) else {
            return;
        };

        let entry = self
            .lifecycles
            .entry(lifecycle_id.clone())
            .or_insert_with(|| ToolCallLifecycleSummary {
                lifecycle_id,
                started_at_ms: event.timestamp_ms,
                ended_at_ms: event.timestamp_ms,
                ..ToolCallLifecycleSummary::default()
            });
        entry.started_at_ms = entry.started_at_ms.min(event.timestamp_ms);
        entry.ended_at_ms = entry.ended_at_ms.max(event.timestamp_ms);
        entry.target_pids.insert(event.pid);

        let stage = event_stage_label(event);
        *entry.stages.entry(stage).or_default() += 1;
        if event_stage_label(event) == "background" {
            entry.background_events += 1;
        }
    }

    fn observe_actions(&mut self, actions: &[runtime_orchestrator::AppliedAction]) {
        for action in actions {
            let Some(lifecycle_id) = action.audit_fields.get("tool_call_id").cloned() else {
                continue;
            };

            let entry = self
                .lifecycles
                .entry(lifecycle_id.clone())
                .or_insert_with(|| ToolCallLifecycleSummary {
                    lifecycle_id,
                    started_at_ms: action.applied_at_ms,
                    ended_at_ms: action.expires_at_ms,
                    ..ToolCallLifecycleSummary::default()
                });
            entry.started_at_ms = entry.started_at_ms.min(action.applied_at_ms);
            entry.ended_at_ms = entry.ended_at_ms.max(action.expires_at_ms);
            entry.target_pids.insert(action.target_pid);

            if action.state == runtime_orchestrator::AppliedActionState::Applied {
                entry.boosted_actions += action.actions.len() as u64;
            }
            if action.state == runtime_orchestrator::AppliedActionState::RolledBack {
                entry.rollback_actions += action.actions.len() as u64;
            }
            if action
                .audit_fields
                .get("isolation_mode")
                .is_some_and(|mode| mode != "none")
            {
                entry.isolation_events += 1;
            }
        }
    }

    fn finish(self) -> Vec<ToolCallLifecycleSummary> {
        self.lifecycles.into_values().collect()
    }
}

fn tool_call_lifecycle_id(event: &runtime_orchestrator::Event) -> Option<String> {
    event
        .tag_markers
        .iter()
        .find_map(|tag| tag.strip_prefix("tool_call_id=").map(str::to_string))
        .or_else(|| extract_tool_call_id(&event.cmdline))
        .or_else(|| extract_tool_call_id(event.cgroup.as_deref().unwrap_or_default()))
        .or_else(|| extract_tool_call_id(event.parent_cmdline.as_deref().unwrap_or_default()))
}

fn extract_tool_call_id(value: &str) -> Option<String> {
    for marker in ["tool_call_id=", "--tool-call-id "] {
        if let Some(found) = extract_after_marker(value, marker) {
            return Some(found);
        }
    }
    if let Some(rest) = value.split("/tool-call/").nth(1) {
        return rest
            .split('/')
            .next()
            .filter(|item| !item.is_empty())
            .map(str::to_string);
    }
    None
}

fn extract_after_marker(value: &str, marker: &str) -> Option<String> {
    let rest = value.split(marker).nth(1)?;
    rest.split(|ch: char| ch.is_ascii_whitespace() || ch == ',' || ch == ';' || ch == '/')
        .next()
        .filter(|item| !item.is_empty())
        .map(str::to_string)
}

fn event_stage_label(event: &runtime_orchestrator::Event) -> String {
    let cmdline = event.cmdline.to_ascii_lowercase();
    let cgroup = event
        .cgroup
        .as_deref()
        .unwrap_or_default()
        .to_ascii_lowercase();
    let process_name = event.process_name.to_ascii_lowercase();

    if cmdline.contains("rerank") || cgroup.contains("rerank") {
        "rerank".to_string()
    } else if cmdline.contains("retrieval") || cgroup.contains("retrieval") {
        "retrieval".to_string()
    } else if process_name == "stress-ng"
        || cmdline.contains("background")
        || cgroup.contains("background")
    {
        "background".to_string()
    } else {
        "executor".to_string()
    }
}

fn collect_audit_highlights(
    highlights: &mut BTreeSet<String>,
    actions: &[runtime_orchestrator::AppliedAction],
) {
    const AUDIT_PREFIXES: [&str; 7] = [
        "backend.apply.live_guard.",
        "backend.apply.capture.",
        "backend.apply.apply.",
        "backend.apply.lease.",
        "backend.rollback.live_guard.",
        "backend.rollback.lease.",
        "backend.rollback.rollback.",
    ];
    const ACTION_AUDIT_FIELDS: [&str; 5] = [
        "tool_call_id",
        "tool_call_stage",
        "action_plan",
        "isolation_mode",
        "isolation_scope",
    ];

    for action in actions {
        for (key, value) in &action.audit_fields {
            if AUDIT_PREFIXES.iter().any(|prefix| key.starts_with(prefix))
                || ACTION_AUDIT_FIELDS.contains(&key.as_str())
            {
                let value = audit_highlight_value(action, key, value);
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

fn audit_highlight_value(
    action: &runtime_orchestrator::AppliedAction,
    key: &str,
    value: &str,
) -> String {
    if action.scenario.as_str() != "tool_call_booster" {
        return value.to_string();
    }
    let Some(action_index) = apply_detail_index(key) else {
        return value.to_string();
    };

    let tool_call_stage = action
        .audit_fields
        .get("tool_call_stage")
        .map(String::as_str)
        .unwrap_or("unknown");
    let tool_call_id = action
        .audit_fields
        .get("tool_call_id")
        .map(String::as_str)
        .unwrap_or("unknown");
    let action_kind = action_kind(action.actions.get(action_index));
    let effective = action_effective(action, action_index, value);
    let detail = value.replace('\r', "\\r").replace('\n', "\\n");

    format!(
        "tool_call_stage={tool_call_stage};tool_call_id={tool_call_id};action_kind={action_kind};effective={effective};{detail}"
    )
}

fn apply_detail_index(key: &str) -> Option<usize> {
    let rest = key.strip_prefix("backend.apply.apply.")?;
    let index = rest.strip_suffix(".detail")?;
    index.parse::<usize>().ok()
}

fn action_kind(action: Option<&runtime_orchestrator::Action>) -> &'static str {
    match action {
        Some(runtime_orchestrator::Action::RaiseNice { .. }) => "raise_nice",
        Some(runtime_orchestrator::Action::SetAffinity { .. }) => "set_affinity",
        Some(runtime_orchestrator::Action::UseCpuset { .. }) => "use_cpuset",
        Some(runtime_orchestrator::Action::WarmupExecutor) => "warmup_executor",
        None => "unknown",
    }
}

fn action_effective(
    action: &runtime_orchestrator::AppliedAction,
    action_index: usize,
    detail: &str,
) -> bool {
    if action
        .audit_fields
        .get("backend.apply.backend")
        .map(String::as_str)
        != Some("linux-command")
    {
        return false;
    }

    match action.actions.get(action_index) {
        Some(runtime_orchestrator::Action::RaiseNice { .. }) => {
            renice_priority_changed(detail).unwrap_or(false)
        }
        Some(runtime_orchestrator::Action::SetAffinity { .. }) => {
            taskset_affinity_changed(detail).unwrap_or(false)
        }
        Some(runtime_orchestrator::Action::UseCpuset { .. })
        | Some(runtime_orchestrator::Action::WarmupExecutor)
        | None => false,
    }
}

fn renice_priority_changed(detail: &str) -> Option<bool> {
    let old_priority = parse_i32_after(detail, "old priority")?;
    let new_priority = parse_i32_after(detail, "new priority")?;
    Some(old_priority != new_priority)
}

fn parse_i32_after(value: &str, marker: &str) -> Option<i32> {
    let rest = value.split(marker).nth(1)?;
    rest.split(|ch: char| !(ch == '-' || ch.is_ascii_digit()))
        .find(|item| !item.is_empty())
        .and_then(|item| item.parse::<i32>().ok())
}

fn taskset_affinity_changed(detail: &str) -> Option<bool> {
    let current = affinity_list_after(detail, "current affinity list:")?;
    let new = affinity_list_after(detail, "new affinity list:")?;
    let current = parse_cpu_set(current)?;
    let new = parse_cpu_set(new)?;
    Some(current != new)
}

fn affinity_list_after<'a>(value: &'a str, marker: &str) -> Option<&'a str> {
    value
        .split(marker)
        .nth(1)?
        .lines()
        .next()?
        .split(';')
        .next()
        .map(str::trim)
        .filter(|item| !item.is_empty())
}

fn parse_cpu_set(raw: &str) -> Option<BTreeSet<u32>> {
    let mut cpus = BTreeSet::new();
    for segment in raw
        .split(',')
        .map(str::trim)
        .filter(|item| !item.is_empty())
    {
        if let Some((start, end)) = segment.split_once('-') {
            let start = start.trim().parse::<u32>().ok()?;
            let end = end.trim().parse::<u32>().ok()?;
            if start > end {
                return None;
            }
            cpus.extend(start..=end);
        } else {
            cpus.insert(segment.parse::<u32>().ok()?);
        }
    }
    (!cpus.is_empty()).then_some(cpus)
}

#[cfg(test)]
mod tests {
    use std::collections::{BTreeMap, BTreeSet};
    use std::path::{Path, PathBuf};

    use crate::{
        EventSource, MockEventSource, NoopMetadataProvider, RuntimeLoop, RuntimeLoopConfig,
        SourceError, SourceEvent, StaticMetadataProvider,
    };
    use aegisai_actuator::{
        Actuator, CommandLinuxSyscallApplier, LinuxActuatorBackend,
        PlannedOnlyLinuxSyscallExecutor, UnavailableLinuxProcessStateProvider,
    };
    use runtime_orchestrator::{
        Action, AppliedAction, AppliedActionState, PinStrategy, RuntimeOrchestrator,
        RuntimeOrchestratorConfig, ScenarioKind, SignalKind,
    };

    use super::collect_audit_highlights;

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
    fn runtime_loop_can_stop_after_max_events() {
        let mut orchestrator =
            RuntimeOrchestrator::from_repo_root(repo_root()).expect("config should load");
        let mut source = MockEventSource::demo_sequence();
        let mut metadata = StaticMetadataProvider::demo();
        let runtime_loop = RuntimeLoop::new(RuntimeLoopConfig {
            max_events: Some(1),
            ..RuntimeLoopConfig::default()
        })
        .expect("valid config");

        let summary = runtime_loop
            .run(&mut orchestrator, &mut source, &mut metadata)
            .expect("runtime loop should succeed");

        assert_eq!(summary.processed_events, 1);
    }

    struct DiagnosticSource {
        diagnostics: Vec<String>,
    }

    impl EventSource for DiagnosticSource {
        fn source_name(&self) -> &str {
            "diagnostic-source"
        }

        fn next_event(&mut self) -> Result<Option<SourceEvent>, SourceError> {
            Ok(None)
        }

        fn diagnostics(&self) -> Vec<String> {
            self.diagnostics.clone()
        }
    }

    #[test]
    fn runtime_loop_records_source_diagnostics_even_without_events() {
        let mut orchestrator =
            RuntimeOrchestrator::from_repo_root(repo_root()).expect("config should load");
        let mut source = DiagnosticSource {
            diagnostics: vec!["helper compatibility: status=compatible".to_string()],
        };
        let mut metadata = NoopMetadataProvider;
        let runtime_loop = RuntimeLoop::new(RuntimeLoopConfig::default()).expect("valid config");

        let summary = runtime_loop
            .run(&mut orchestrator, &mut source, &mut metadata)
            .expect("runtime loop should succeed");

        assert_eq!(summary.processed_events, 0);
        assert_eq!(
            summary.source_diagnostics,
            vec!["helper compatibility: status=compatible".to_string()]
        );
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
    fn tool_call_lifecycle_mock_tracks_subchains_and_isolation() {
        let mut orchestrator =
            RuntimeOrchestrator::from_repo_root(repo_root()).expect("config should load");
        let mut source = MockEventSource::tool_call_lifecycle_sequence();
        let mut metadata = NoopMetadataProvider;
        let runtime_loop = RuntimeLoop::new(RuntimeLoopConfig::default()).expect("valid config");

        let summary = runtime_loop
            .run(&mut orchestrator, &mut source, &mut metadata)
            .expect("runtime loop should succeed");

        assert_eq!(summary.source_name, "mock-tool-call-lifecycle");
        assert!(summary
            .triggered_scenarios
            .contains_key("tool_call_booster"));
        assert_eq!(summary.tool_call_lifecycles.len(), 1);
        let lifecycle = &summary.tool_call_lifecycles[0];
        assert_eq!(lifecycle.lifecycle_id, "tc-001");
        assert_eq!(lifecycle.stages.get("executor"), Some(&1));
        assert_eq!(lifecycle.stages.get("retrieval"), Some(&2));
        assert_eq!(lifecycle.stages.get("rerank"), Some(&1));
        assert_eq!(lifecycle.background_events, 1);
        assert!(lifecycle.boosted_actions >= 3);
        assert!(lifecycle.rollback_actions >= 3);
        assert!(lifecycle.isolation_events >= 3);
    }

    #[test]
    fn runtime_loop_summarizes_procfs_explainability_signals() {
        let mut orchestrator =
            RuntimeOrchestrator::from_repo_root(repo_root()).expect("config should load");
        let mut source = MockEventSource::new(
            "procfs-signal-summary",
            vec![
                SourceEvent::new(1_000, 42, SignalKind::CpuMigration, 3)
                    .with_process_name("ollama"),
                SourceEvent::new(1_100, 42, SignalKind::MajorPageFault, 2)
                    .with_process_name("ollama"),
            ],
        );
        let mut metadata = NoopMetadataProvider;
        let runtime_loop = RuntimeLoop::new(RuntimeLoopConfig::default()).expect("valid config");

        let summary = runtime_loop
            .run(&mut orchestrator, &mut source, &mut metadata)
            .expect("runtime loop should succeed");

        let migrations = summary
            .signal_observations
            .get("cpu_migration")
            .expect("cpu migration summary");
        assert_eq!(migrations.event_count, 1);
        assert_eq!(migrations.value_total, 3);
        assert_eq!(migrations.value_max, 3);

        let faults = summary
            .signal_observations
            .get("major_page_fault")
            .expect("major fault summary");
        assert_eq!(faults.event_count, 1);
        assert_eq!(faults.value_total, 2);
        assert_eq!(faults.value_max, 2);
        assert!(summary
            .feature_window_maxima
            .contains_key("cpu_migrations_per_sec"));
        assert!(summary
            .feature_window_maxima
            .contains_key("major_page_faults_per_sec"));
    }

    #[test]
    fn runtime_loop_collects_audit_highlights_from_backend_execution() {
        let config = RuntimeOrchestratorConfig::load_from_repo_root(repo_root())
            .expect("config should load");
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
        assert!(summary
            .audit_highlights
            .iter()
            .any(|highlight| highlight.contains("backend.apply.lease.")));
        assert!(summary
            .audit_highlights
            .iter()
            .any(|highlight| highlight.contains("tool_call_stage=")));
        assert!(summary.audit_highlights.iter().any(|highlight| {
            highlight.contains("scenario=tool_call_booster;backend.apply.apply.")
                && highlight.contains(".detail=tool_call_stage=")
                && highlight.contains(";tool_call_id=")
                && highlight.contains(";action_kind=")
                && highlight.contains(";effective=false;")
        }));
    }

    #[test]
    fn tool_call_apply_details_carry_inline_attribution_and_effectiveness() {
        let mut audit_fields = BTreeMap::new();
        audit_fields.insert("tool_call_stage".to_string(), "retrieval".to_string());
        audit_fields.insert("tool_call_id".to_string(), "tc-001".to_string());
        audit_fields.insert(
            "backend.apply.backend".to_string(),
            "linux-command".to_string(),
        );
        audit_fields.insert(
            "backend.apply.apply.0.detail".to_string(),
            "runner=system-command-runner;command=taskset -pc 0,1 42;output=pid 42's current affinity list: 0-7\npid 42's new affinity list: 0,1".to_string(),
        );

        let action = AppliedAction {
            scenario: ScenarioKind::ToolCallBooster,
            target_pid: 42,
            target_process_name: "python".to_string(),
            actions: vec![Action::SetAffinity {
                strategy: PinStrategy::PreferReservedCores,
                max_cpu_ratio: 0.5,
            }],
            applied_at_ms: 1_000,
            expires_at_ms: 1_500,
            state: AppliedActionState::Applied,
            audit_fields,
        };
        let mut highlights = BTreeSet::new();

        collect_audit_highlights(&mut highlights, &[action]);

        let detail = highlights
            .iter()
            .find(|highlight| highlight.contains("backend.apply.apply.0.detail="))
            .expect("apply detail highlight");
        assert!(!detail.contains('\n'));
        assert!(detail.contains(
            "backend.apply.apply.0.detail=tool_call_stage=retrieval;tool_call_id=tc-001;action_kind=set_affinity;effective=true;"
        ));
    }
}
