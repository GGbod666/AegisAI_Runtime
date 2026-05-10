use std::collections::{BTreeMap, BTreeSet};
use std::fmt;
use std::path::Path;

use aegisai_actuator as aa;
use aegisai_classifier as ac;
use aegisai_collector as agc;
use aegisai_metrics::{
    MetricKind, MetricTrace, MetricsConfig, MetricsConfigError, MetricsRecorder, RecordInput,
    TraceKind,
};
use aegisai_policy_engine as pe;
use aegisai_runtime_contracts::{EventContext, WorkloadProfile as SharedWorkloadProfile};

use crate::config::{AwarenessConfig, ClassifierConfig, RuntimeConfig, RuntimeOrchestratorConfig};
use crate::model::{
    Action, AppliedAction, Event, FeatureWindow, MetricRecord, OrchestrationOutcome, SignalKind,
    WorkloadProfile, WorkloadTag,
};

pub struct RuntimeOrchestrator {
    config: RuntimeOrchestratorConfig,
    collector: agc::Collector,
    classifier: AiWorkloadAwareness,
    policy_engine: pe::PolicyEngine,
    actuator: aa::Actuator,
    metrics: MetricsRecorder,
}

#[derive(Debug)]
pub enum RuntimeOrchestratorInitError {
    Collector(agc::CollectorError),
    Metrics(MetricsConfigError),
}

impl fmt::Display for RuntimeOrchestratorInitError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Collector(error) => write!(f, "{error}"),
            Self::Metrics(error) => write!(f, "{error}"),
        }
    }
}

impl std::error::Error for RuntimeOrchestratorInitError {}

impl From<agc::CollectorError> for RuntimeOrchestratorInitError {
    fn from(value: agc::CollectorError) -> Self {
        Self::Collector(value)
    }
}

impl From<MetricsConfigError> for RuntimeOrchestratorInitError {
    fn from(value: MetricsConfigError) -> Self {
        Self::Metrics(value)
    }
}

#[derive(Debug)]
pub enum RuntimeOrchestratorLoadError {
    Config(crate::ConfigError),
    Init(RuntimeOrchestratorInitError),
}

impl fmt::Display for RuntimeOrchestratorLoadError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Config(error) => write!(f, "{error}"),
            Self::Init(error) => write!(f, "{error}"),
        }
    }
}

impl std::error::Error for RuntimeOrchestratorLoadError {}

impl From<crate::ConfigError> for RuntimeOrchestratorLoadError {
    fn from(value: crate::ConfigError) -> Self {
        Self::Config(value)
    }
}

impl From<RuntimeOrchestratorInitError> for RuntimeOrchestratorLoadError {
    fn from(value: RuntimeOrchestratorInitError) -> Self {
        Self::Init(value)
    }
}

impl RuntimeOrchestrator {
    pub fn new(config: RuntimeOrchestratorConfig) -> Result<Self, RuntimeOrchestratorInitError> {
        Self::with_actuator(config, aa::Actuator::default())
    }

    pub fn with_actuator(
        config: RuntimeOrchestratorConfig,
        actuator: aa::Actuator,
    ) -> Result<Self, RuntimeOrchestratorInitError> {
        Ok(Self {
            collector: agc::Collector::new(collector_config_from_runtime(&config))?,
            classifier: AiWorkloadAwareness::new(config.runtime.clone(), config.classifier.clone()),
            policy_engine: pe::PolicyEngine::new(config.scenarios.clone(), config.safety.clone()),
            actuator,
            metrics: MetricsRecorder::new(metrics_config_from_runtime(&config.runtime))?,
            config,
        })
    }

    pub fn from_repo_root(root: impl AsRef<Path>) -> Result<Self, RuntimeOrchestratorLoadError> {
        let config = RuntimeOrchestratorConfig::load_from_repo_root(root)?;
        Ok(Self::new(config)?)
    }

    pub fn process_event(&mut self, event: Event) -> OrchestrationOutcome {
        let rollbacks = self.actuator.expire(event.timestamp_ms);
        if should_collect_signal(&self.config.runtime, &event.signal) {
            if let Some(collector_event) = to_collector_event(&event) {
                let _ = self.collector.ingest(collector_event);
            }
        }

        let profile = self.classifier.classify(&event);

        let enabled_policies = self
            .config
            .scenarios
            .iter()
            .filter(|(_, policy)| policy.enabled)
            .map(|(scenario, policy)| (scenario.clone(), policy.clone()))
            .collect::<Vec<_>>();

        let evaluated_scenarios = enabled_policies
            .iter()
            .map(|(scenario, _)| scenario.as_str().to_string())
            .collect::<Vec<_>>();

        let mut feature_windows = BTreeMap::new();
        let mut policy_contexts = Vec::new();

        for (scenario, policy) in &enabled_policies {
            let feature_window = self.project_feature_window(
                event.pid,
                event.timestamp_ms,
                policy.evaluation_window_ms,
            );
            feature_windows.insert(scenario.clone(), feature_window.clone());
            policy_contexts.push(pe::PolicyContext {
                scenario: scenario.clone(),
                event: EventContext::new(event.timestamp_ms, event.pid, event.process_name.clone()),
                feature_window,
                profile: profile.clone(),
                audit_fields: event_audit_fields(&event),
            });
        }

        let applied_actions = self.policy_engine.evaluate_all(policy_contexts.iter());
        let applied_actions = applied_actions
            .into_iter()
            .map(|plan| {
                self.actuator
                    .apply(plan, event.timestamp_ms, self.config.safety.require_revert)
            })
            .collect::<Vec<_>>();

        let metric_record = self.metrics.record(
            RecordInput::new(
                event.timestamp_ms,
                profile.pid,
                profile.process_name.clone(),
            )
            .with_workload_tags(profile.tags.iter().map(WorkloadTag::as_str))
            .with_evaluated_scenarios(evaluated_scenarios.iter().map(|item| item.as_str()))
            .with_triggered_scenarios(
                applied_actions
                    .iter()
                    .map(|action| action.scenario.as_str()),
            )
            .with_action_count(applied_actions.len())
            .with_rollback_count(rollbacks.len())
            .with_traces(action_traces(&applied_actions, &rollbacks))
            .with_notes(metric_notes(&applied_actions, &rollbacks)),
        );

        OrchestrationOutcome {
            profile,
            feature_windows,
            applied_actions,
            rollbacks,
            metric_record,
        }
    }

    pub fn tick(&mut self, now_ms: u64) -> Vec<AppliedAction> {
        let rollbacks = self.actuator.expire(now_ms);
        for rollback in &rollbacks {
            let rollback_trace = with_action_audit_fields(
                MetricTrace::new(
                    rollback.expires_at_ms,
                    rollback.target_pid,
                    rollback.target_process_name.clone(),
                    TraceKind::ActionRolledBack,
                    format!("rolled back {} action(s)", rollback.actions.len()),
                )
                .with_scenario(rollback.scenario.as_str())
                .with_field("actions", action_names(&rollback.actions).join(","))
                .with_field("rolled_back", "true"),
                rollback,
            );
            self.metrics.record(
                RecordInput::new(
                    now_ms,
                    rollback.target_pid,
                    rollback.target_process_name.clone(),
                )
                .with_rollback_count(1)
                .with_traces([rollback_trace])
                .with_notes([format!("rollback:{}", rollback.scenario.as_str())]),
            );
        }
        rollbacks
    }

    pub fn metrics(&self) -> &[MetricRecord] {
        self.metrics.records()
    }

    pub fn traces(&self) -> &[MetricTrace] {
        self.metrics.traces()
    }

    pub fn active_action_count(&self) -> usize {
        self.actuator.active_count()
    }

    pub fn actuator_backend_name(&self) -> &str {
        self.actuator.backend_name()
    }

    fn project_feature_window(&mut self, pid: u32, now_ms: u64, window_ms: u64) -> FeatureWindow {
        let window = self.collector.process_window(
            pid,
            agc::TimestampMicros::new(now_ms.saturating_mul(1_000)),
            window_ms.saturating_mul(1_000),
        );

        collector_window_to_runtime(window)
    }
}

struct AiWorkloadAwareness {
    runtime: RuntimeConfig,
    awareness: AwarenessConfig,
    classifier: ac::Classifier,
}

impl AiWorkloadAwareness {
    fn new(runtime: RuntimeConfig, config: ClassifierConfig) -> Self {
        let awareness = config.awareness;
        let classifier = ac::Classifier::with_options(
            config.process_rules,
            ac::ClassifierOptions {
                enable_cmdline_rules: awareness.enable_cmdline_rules,
                enable_cgroup_rules: awareness.enable_cgroup_rules,
                enable_parent_child_inference: awareness.enable_parent_child_inference,
                enable_pid_allowlist: awareness.enable_pid_allowlist,
            },
        );

        Self {
            runtime,
            awareness,
            classifier,
        }
    }

    fn classify(&self, event: &Event) -> WorkloadProfile {
        let snapshot = self.snapshot_from_event(event);
        let process_name = snapshot.process_name.clone();
        let profile = self.classify_snapshot(&snapshot);
        SharedWorkloadProfile::from_classifier(process_name, profile)
    }

    fn classify_snapshot(&self, snapshot: &ac::ProcessSnapshot) -> ac::WorkloadProfile {
        let mut tags = BTreeSet::new();
        let mut matched_rules = Vec::new();

        if self.is_runtime_process(&snapshot.process_name) {
            tags.extend(self.awareness.interactive_default.iter().cloned());
            matched_rules.push(format!("runtime.process_names:{}", snapshot.process_name));
        }

        if self.awareness.enable_pid_allowlist && self.runtime.pid_allowlist.contains(&snapshot.pid)
        {
            tags.extend(self.awareness.interactive_default.iter().cloned());
            matched_rules.push(format!("runtime.pid_allowlist:{}", snapshot.pid));
        }

        if self.awareness.enable_parent_child_inference {
            if let Some(parent) = snapshot.parent.as_ref() {
                if self.is_runtime_process(&parent.process_name) {
                    tags.extend(self.awareness.interactive_default.iter().cloned());
                    matched_rules.push(format!("parent_runtime:{}", parent.process_name));
                }
            }
        }

        let rule_profile = self.classifier.classify_process(snapshot);
        tags.extend(rule_profile.tags);
        matched_rules.extend(rule_profile.matched_rules);

        if tags.contains(&WorkloadTag::ToolCall) {
            tags.extend(self.awareness.tool_executor_default.iter().cloned());
        }

        if tags.contains(&WorkloadTag::BackgroundJob) {
            tags.extend(self.awareness.background_default.iter().cloned());
        }

        ac::WorkloadProfile::from_process(snapshot, tags, matched_rules)
    }

    fn snapshot_from_event(&self, event: &Event) -> ac::ProcessSnapshot {
        let mut snapshot =
            ac::ProcessSnapshot::new(event.pid, event.process_name.clone(), event.cmdline.clone());
        snapshot.tid = event.tid;
        snapshot.cgroup_path = event.cgroup.clone();
        snapshot.tags = event.tag_markers.clone();
        snapshot.parent = self.parent_ref_from_event(event);
        snapshot
    }

    fn parent_ref_from_event(&self, event: &Event) -> Option<ac::ProcessRef> {
        let has_parent = event.parent_pid.is_some()
            || event.parent_process_name.is_some()
            || event.parent_cmdline.is_some();
        if !has_parent {
            return None;
        }

        let parent_pid = event.parent_pid.unwrap_or(0);
        let parent_process_name = event
            .parent_process_name
            .clone()
            .unwrap_or_else(|| "unknown".to_string());
        let parent_cmdline = event.parent_cmdline.clone().unwrap_or_default();

        let parent_snapshot = ac::ProcessSnapshot::new(
            parent_pid,
            parent_process_name.clone(),
            parent_cmdline.clone(),
        );
        let parent_profile = self.classify_snapshot(&parent_snapshot);

        let mut parent = ac::ProcessRef::new(parent_pid, parent_process_name, parent_cmdline);
        parent.tags = parent_profile.tags;
        Some(parent)
    }

    fn is_runtime_process(&self, process_name: &str) -> bool {
        self.runtime
            .process_names
            .iter()
            .any(|name| name.eq_ignore_ascii_case(process_name))
    }
}

fn collector_config_from_runtime(config: &RuntimeOrchestratorConfig) -> agc::CollectorConfig {
    let max_window_us = config
        .scenarios
        .values()
        .map(|policy| policy.evaluation_window_ms.saturating_mul(1_000))
        .max()
        .unwrap_or(500_000)
        .max(1);

    let mut collector_config = agc::CollectorConfig::default()
        .with_scopes(vec![agc::AggregationScope::Process])
        .with_recent_event_retention_us(max_window_us);
    collector_config.window_size_us = max_window_us;
    collector_config
}

fn collector_window_to_runtime(window: agc::FeatureWindow) -> FeatureWindow {
    let pid = match &window.target {
        agc::AggregationTarget::Process { pid } => *pid,
        agc::AggregationTarget::Thread { pid, .. } => *pid,
        agc::AggregationTarget::Cgroup { cgroup_id } => *cgroup_id as u32,
    };

    FeatureWindow {
        pid,
        started_at_ms: window.window_start.as_u64() / 1_000,
        ended_at_ms: window.window_end.as_u64() / 1_000,
        sample_count: window.observed_events as usize,
        run_queue_delay_us_max: window.run_queue_delay.max.unwrap_or(0),
        offcpu_time_us_max: window.off_cpu.max.unwrap_or(0),
        cpu_migrations_per_sec: window.cpu_migrations.per_second.floor() as u64,
        major_page_faults_per_sec: window.major_page_faults.per_second.floor() as u64,
        subprocess_start_delay_us_max: window.subprocess_start_delay.max.unwrap_or(0),
        queue_wait_us_max: window.queue_wait.max.unwrap_or(0),
        optional_io_latency_us_max: window.io_latency.max.unwrap_or(0),
    }
}

fn to_collector_event(event: &Event) -> Option<agc::Event> {
    let kind = match &event.signal {
        SignalKind::RunQueueDelay => agc::EventKind::RunQueueDelay {
            delay_us: event.value,
        },
        SignalKind::OffCpuTime => agc::EventKind::OffCpu {
            duration_us: event.value,
        },
        SignalKind::CpuMigration => agc::EventKind::CpuMigration {
            count: event.value.max(1),
        },
        SignalKind::MajorPageFault => agc::EventKind::MajorPageFault {
            count: event.value.max(1),
        },
        SignalKind::SubprocessStartDelay => agc::EventKind::SubprocessStartDelay {
            delay_us: event.value,
        },
        SignalKind::QueueWait => agc::EventKind::QueueWait {
            wait_us: event.value,
        },
        SignalKind::IoLatency => agc::EventKind::IoLatency {
            latency_us: event.value,
        },
        SignalKind::Unknown(_) => return None,
    };

    Some(agc::Event::new(
        agc::TimestampMicros::new(event.timestamp_ms.saturating_mul(1_000)),
        event.pid,
        event.tid.unwrap_or(event.pid),
        None,
        collector_probe_source(&event.signal),
        kind,
    ))
}

fn collector_probe_source(signal: &SignalKind) -> agc::ProbeSource {
    match signal {
        SignalKind::RunQueueDelay | SignalKind::CpuMigration => agc::ProbeSource::Sched,
        SignalKind::OffCpuTime => agc::ProbeSource::OffCpu,
        SignalKind::MajorPageFault => agc::ProbeSource::Fault,
        SignalKind::IoLatency => agc::ProbeSource::Io,
        SignalKind::SubprocessStartDelay | SignalKind::QueueWait | SignalKind::Unknown(_) => {
            agc::ProbeSource::Runtime
        }
    }
}

fn metrics_config_from_runtime(runtime: &RuntimeConfig) -> MetricsConfig {
    MetricsConfig::default().with_tracked_metrics(
        runtime
            .tracked_metrics
            .iter()
            .map(|metric| MetricKind::parse(metric))
            .collect(),
    )
}

fn should_collect_signal(runtime: &RuntimeConfig, signal: &SignalKind) -> bool {
    runtime.focus_signals.is_empty() || runtime.focus_signals.contains(signal)
}

fn event_audit_fields(event: &Event) -> BTreeMap<String, String> {
    let mut audit_fields = BTreeMap::new();
    if let Some(tool_call_id) = tool_call_lifecycle_id(event) {
        audit_fields.insert("tool_call_id".to_string(), tool_call_id);
    }
    audit_fields
}

fn tool_call_lifecycle_id(event: &Event) -> Option<String> {
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

fn action_traces(
    applied_actions: &[AppliedAction],
    rollbacks: &[AppliedAction],
) -> Vec<MetricTrace> {
    let mut traces = Vec::new();

    for action in applied_actions {
        let trace = MetricTrace::new(
            action.applied_at_ms,
            action.target_pid,
            action.target_process_name.clone(),
            TraceKind::ActionApplied,
            format!("applied {} action(s)", action.actions.len()),
        )
        .with_scenario(action.scenario.as_str())
        .with_field("actions", action_names(&action.actions).join(","))
        .with_field("expires_at_ms", action.expires_at_ms.to_string());
        traces.push(with_action_audit_fields(trace, action));
    }

    for rollback in rollbacks {
        let trace = MetricTrace::new(
            rollback.expires_at_ms,
            rollback.target_pid,
            rollback.target_process_name.clone(),
            TraceKind::ActionRolledBack,
            format!("rolled back {} action(s)", rollback.actions.len()),
        )
        .with_scenario(rollback.scenario.as_str())
        .with_field("actions", action_names(&rollback.actions).join(","))
        .with_field("rolled_back", "true");
        traces.push(with_action_audit_fields(trace, rollback));
    }

    traces
}

fn with_action_audit_fields(mut trace: MetricTrace, action: &AppliedAction) -> MetricTrace {
    const TRACE_AUDIT_FIELDS: [&str; 11] = [
        "tool_call_id",
        "tool_call_stage",
        "tool_call_focus",
        "tool_call_subchain",
        "duration_ratio",
        "duration_ms",
        "action_plan",
        "isolation_mode",
        "isolation_scope",
        "background_isolation",
        "warmup_executor_skipped",
    ];

    for field in TRACE_AUDIT_FIELDS {
        if let Some(value) = action.audit_fields.get(field) {
            trace = trace.with_field(field, value.as_str());
        }
    }
    trace
}

fn action_names(actions: &[Action]) -> Vec<String> {
    actions
        .iter()
        .map(|action| match action {
            Action::RaiseNice { delta } => format!("raise_nice:{delta}"),
            Action::SetAffinity {
                strategy,
                max_cpu_ratio,
            } => format!("set_affinity:{}:{max_cpu_ratio}", strategy.as_str()),
            Action::UseCpuset { enabled } => format!("use_cpuset:{enabled}"),
            Action::WarmupExecutor => "warmup_executor".to_string(),
        })
        .collect()
}

fn metric_notes(applied_actions: &[AppliedAction], rollbacks: &[AppliedAction]) -> Vec<String> {
    let mut notes = Vec::new();

    for action in applied_actions {
        if let Some(breaches) = action.audit_fields.get("breaches") {
            notes.push(format!("{}:{breaches}", action.scenario.as_str()));
        }
    }

    for rollback in rollbacks {
        notes.push(format!("rollback:{}", rollback.scenario.as_str()));
    }

    notes
}

#[cfg(test)]
mod tests {
    use std::path::{Path, PathBuf};

    use super::*;
    use crate::config::{SafetyConfig, TriggerThresholds};
    use crate::model::{
        AppliedActionState, LatencySensitivity, ScenarioKind, StageLabel, WorkloadClass,
    };

    fn repo_root() -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .ancestors()
            .nth(2)
            .expect("crate lives under agent/runtime_orchestrator")
            .to_path_buf()
    }

    #[test]
    fn loads_sample_configs_from_repo() {
        let config = RuntimeOrchestratorConfig::load_from_repo_root(repo_root())
            .expect("sample configs should parse");

        assert_eq!(config.runtime.primary_runtime, "ollama");
        assert_eq!(config.runtime.process_names, vec!["ollama", "llama-server"]);
        assert_eq!(config.classifier.process_rules.len(), 7);
        assert!(config.classifier.awareness.enable_parent_child_inference);
        assert!(config
            .scenarios
            .contains_key(&ScenarioKind::InferenceTailGuard));
        assert!(config
            .scenarios
            .contains_key(&ScenarioKind::ToolCallBooster));
        assert_eq!(config.safety.max_boost_duration_ms, 800);
    }

    #[test]
    fn inference_tail_guard_triggers_for_latency_sensitive_runtime() {
        let mut orchestrator =
            RuntimeOrchestrator::from_repo_root(repo_root()).expect("config should load");

        let outcome = orchestrator.process_event(
            Event::new(1_000, 101, "ollama", SignalKind::RunQueueDelay, 2_500)
                .with_cmdline("ollama serve"),
        );

        assert!(outcome.profile.has_tag(&WorkloadTag::AiInference));
        assert!(outcome
            .profile
            .has_tag(&WorkloadTag::InteractiveLatencySensitive));
        assert_eq!(outcome.profile.workload_class, WorkloadClass::AiInference);
        assert_eq!(outcome.profile.stage, StageLabel::Inference);
        assert_eq!(
            outcome.profile.latency_sensitivity,
            LatencySensitivity::Interactive
        );
        assert_eq!(outcome.applied_actions.len(), 1);
        assert_eq!(
            outcome.applied_actions[0].scenario,
            ScenarioKind::InferenceTailGuard
        );
        assert!(outcome.applied_actions[0]
            .actions
            .iter()
            .any(|action| matches!(action, Action::RaiseNice { delta } if *delta == -5)));
        assert_eq!(orchestrator.active_action_count(), 1);
    }

    #[test]
    fn tool_call_booster_triggers_for_retrieval_worker() {
        let mut orchestrator =
            RuntimeOrchestrator::from_repo_root(repo_root()).expect("config should load");

        let outcome = orchestrator.process_event(
            Event::new(2_000, 202, "python", SignalKind::QueueWait, 2_500)
                .with_cmdline("python tool-executor retrieval-worker --tool-call-id tc-001"),
        );

        assert!(outcome.profile.has_tag(&WorkloadTag::ToolCall));
        assert!(outcome.profile.has_tag(&WorkloadTag::RetrievalStage));
        assert_eq!(outcome.profile.workload_class, WorkloadClass::ToolCall);
        assert_eq!(outcome.profile.stage, StageLabel::Retrieval);
        assert_eq!(outcome.applied_actions.len(), 1);
        assert_eq!(
            outcome.applied_actions[0].scenario,
            ScenarioKind::ToolCallBooster
        );
        assert!(outcome.applied_actions[0]
            .actions
            .iter()
            .any(|action| matches!(action, Action::WarmupExecutor)));
        assert_eq!(
            outcome.applied_actions[0].audit_fields.get("tool_call_id"),
            Some(&"tc-001".to_string())
        );
        assert_eq!(
            outcome.applied_actions[0]
                .audit_fields
                .get("tool_call_subchain"),
            Some(&"retrieval_io".to_string())
        );
        assert_eq!(
            outcome.applied_actions[0]
                .audit_fields
                .get("isolation_mode"),
            Some(&"retrieval_affinity_only".to_string())
        );
    }

    #[test]
    fn cooldown_prevents_retrigger_and_tick_rolls_back_expired_actions() {
        let mut orchestrator =
            RuntimeOrchestrator::from_repo_root(repo_root()).expect("config should load");

        let first = orchestrator.process_event(
            Event::new(5_000, 303, "python", SignalKind::QueueWait, 2_500)
                .with_cmdline("python tool-executor retrieval-worker"),
        );
        assert_eq!(first.applied_actions.len(), 1);

        let second = orchestrator.process_event(
            Event::new(5_200, 303, "python", SignalKind::QueueWait, 3_000)
                .with_cmdline("python tool-executor retrieval-worker"),
        );
        assert_eq!(second.applied_actions.len(), 0);

        let rollbacks = orchestrator.tick(6_300);
        assert_eq!(rollbacks.len(), 1);
        assert_eq!(rollbacks[0].state, AppliedActionState::RolledBack);
        assert_eq!(orchestrator.active_action_count(), 0);
        assert!(orchestrator
            .traces()
            .iter()
            .any(|trace| trace.kind == TraceKind::ActionRolledBack));
    }

    #[test]
    fn process_event_expires_due_action_before_applying_new_action() {
        let config = RuntimeOrchestratorConfig::load_from_repo_root(repo_root())
            .expect("config should load");
        let actuator = aa::Actuator::with_backend(aa::RecordingActuatorBackend::default());
        let mut orchestrator =
            RuntimeOrchestrator::with_actuator(config, actuator).expect("orchestrator should init");

        let first = orchestrator.process_event(
            Event::new(1_000, 606, "ollama", SignalKind::RunQueueDelay, 2_500)
                .with_cmdline("ollama serve"),
        );
        assert_eq!(first.applied_actions.len(), 1);
        assert!(first.rollbacks.is_empty());

        let second = orchestrator.process_event(
            Event::new(2_500, 606, "ollama", SignalKind::RunQueueDelay, 2_500)
                .with_cmdline("ollama serve"),
        );

        assert_eq!(second.rollbacks.len(), 1);
        assert_eq!(second.applied_actions.len(), 1);
        assert_eq!(
            second.rollbacks[0]
                .audit_fields
                .get("backend.rollback.operation_index"),
            Some(&"2".to_string())
        );
        assert_eq!(
            second.applied_actions[0]
                .audit_fields
                .get("backend.apply.operation_index"),
            Some(&"3".to_string())
        );
        assert_eq!(second.metric_record.rollback_count, 1);
        assert_eq!(second.metric_record.action_count, 1);
        assert_eq!(orchestrator.active_action_count(), 1);
    }

    #[test]
    fn records_action_traces_for_metrics_module() {
        let mut orchestrator =
            RuntimeOrchestrator::from_repo_root(repo_root()).expect("config should load");

        orchestrator.process_event(
            Event::new(9_000, 404, "ollama", SignalKind::RunQueueDelay, 2_500)
                .with_cmdline("ollama serve"),
        );

        assert!(orchestrator
            .traces()
            .iter()
            .any(|trace| trace.kind == TraceKind::ActionApplied));
    }

    #[test]
    fn action_traces_include_tool_call_lifecycle_audit_fields() {
        let mut orchestrator =
            RuntimeOrchestrator::from_repo_root(repo_root()).expect("config should load");

        orchestrator.process_event(
            Event::new(9_000, 5151, "python", SignalKind::QueueWait, 2_500)
                .with_cmdline("python tool-executor retrieval-worker --tool-call-id tc-009"),
        );

        let trace = orchestrator
            .traces()
            .iter()
            .find(|trace| {
                trace.kind == TraceKind::ActionApplied
                    && trace.scenario.as_deref() == Some("tool_call_booster")
            })
            .expect("tool call action trace should be recorded");

        assert_eq!(
            trace.fields.get("tool_call_id"),
            Some(&"tc-009".to_string())
        );
        assert_eq!(
            trace.fields.get("tool_call_subchain"),
            Some(&"retrieval_io".to_string())
        );
        assert_eq!(trace.fields.get("duration_ratio"), Some(&"3/4".to_string()));
        assert_eq!(trace.fields.get("duration_ms"), Some(&"600".to_string()));
        assert!(trace
            .fields
            .get("action_plan")
            .is_some_and(|plan| plan.contains("warmup_executor")));
        assert_eq!(
            trace.fields.get("background_isolation"),
            Some(&"blocked_by_safety".to_string())
        );
    }

    #[test]
    fn tick_rollback_traces_preserve_tool_call_audit_fields() {
        let mut orchestrator =
            RuntimeOrchestrator::from_repo_root(repo_root()).expect("config should load");

        orchestrator.process_event(
            Event::new(9_000, 5152, "python", SignalKind::QueueWait, 2_500)
                .with_cmdline("python tool-executor rerank-worker --tool-call-id tc-010"),
        );
        let rollbacks = orchestrator.tick(9_500);
        assert_eq!(rollbacks.len(), 1);

        let trace = orchestrator
            .traces()
            .iter()
            .rev()
            .find(|trace| {
                trace.kind == TraceKind::ActionRolledBack
                    && trace.scenario.as_deref() == Some("tool_call_booster")
            })
            .expect("tool call rollback trace should be recorded");

        assert_eq!(
            trace.fields.get("tool_call_id"),
            Some(&"tc-010".to_string())
        );
        assert_eq!(
            trace.fields.get("tool_call_stage"),
            Some(&"rerank".to_string())
        );
        assert_eq!(trace.fields.get("duration_ratio"), Some(&"1/2".to_string()));
        assert!(trace
            .fields
            .get("action_plan")
            .is_some_and(|plan| plan.contains("raise_nice:-3")));
        assert_eq!(trace.fields.get("rolled_back"), Some(&"true".to_string()));
    }

    #[test]
    fn runtime_pid_allowlist_produces_interactive_inference_profile() {
        let config = RuntimeOrchestratorConfig {
            runtime: RuntimeConfig {
                deployment_target: "linux".to_string(),
                kernel_min: "5.15".to_string(),
                cgroup_version: "v2".to_string(),
                primary_runtime: "ollama".to_string(),
                fallback_runtime: "llama.cpp".to_string(),
                selection_mode: "pid_allowlist".to_string(),
                process_names: Vec::new(),
                pid_allowlist: [9001].into_iter().collect(),
                focus_signals: [SignalKind::RunQueueDelay].into_iter().collect(),
                tracked_metrics: vec!["boost_hit_rate".to_string()],
            },
            classifier: ClassifierConfig {
                awareness: crate::config::AwarenessConfig {
                    enable_cmdline_rules: true,
                    enable_cgroup_rules: true,
                    enable_parent_child_inference: true,
                    enable_pid_allowlist: true,
                    interactive_default: [
                        WorkloadTag::AiInference,
                        WorkloadTag::InteractiveLatencySensitive,
                    ]
                    .into_iter()
                    .collect(),
                    tool_executor_default: [WorkloadTag::ToolCall].into_iter().collect(),
                    background_default: [WorkloadTag::BackgroundJob].into_iter().collect(),
                },
                process_rules: Vec::new(),
            },
            safety: SafetyConfig {
                require_revert: true,
                allow_background_throttle: false,
                max_priority_delta: 5,
                max_boost_duration_ms: 500,
                max_affinity_change_ratio: 0.5,
            },
            scenarios: std::collections::BTreeMap::from([(
                ScenarioKind::InferenceTailGuard,
                crate::config::ScenarioPolicy {
                    scenario: ScenarioKind::InferenceTailGuard,
                    enabled: true,
                    evaluation_window_ms: 500,
                    cooldown_ms: 0,
                    max_boost_duration_ms: 500,
                    triggers: TriggerThresholds {
                        run_queue_delay_us: Some(50),
                        ..TriggerThresholds::default()
                    },
                    actions: crate::config::ScenarioActions {
                        raise_nice: Some(-1),
                        ..crate::config::ScenarioActions::default()
                    },
                },
            )]),
        };
        let mut orchestrator = RuntimeOrchestrator::new(config).expect("config should initialize");

        let outcome = orchestrator.process_event(Event::new(
            10_000,
            9001,
            "python",
            SignalKind::RunQueueDelay,
            100,
        ));

        assert_eq!(outcome.profile.workload_class, WorkloadClass::AiInference);
        assert_eq!(outcome.profile.stage, StageLabel::Inference);
        assert_eq!(
            outcome.profile.latency_sensitivity,
            LatencySensitivity::Interactive
        );
        assert!(outcome
            .profile
            .matched_rules
            .contains(&"runtime.pid_allowlist:9001".to_string()));
    }
}
