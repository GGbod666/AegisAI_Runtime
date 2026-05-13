use std::collections::{BTreeMap, BTreeSet};
use std::fmt;
use std::fs;
use std::path::{Path, PathBuf};

use aegisai_classifier as ac;
pub use aegisai_runtime_contracts::{
    PinStrategy, SafetyConfig, ScenarioActions, ScenarioKind, ScenarioPolicy, SignalKind,
    TriggerThresholds, WorkloadTag,
};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RuntimeConfig {
    pub deployment_target: String,
    pub kernel_min: String,
    pub cgroup_version: String,
    pub primary_runtime: String,
    pub fallback_runtime: String,
    pub selection_mode: String,
    pub process_names: Vec<String>,
    pub pid_allowlist: BTreeSet<u32>,
    pub focus_signals: BTreeSet<SignalKind>,
    pub tracked_metrics: Vec<String>,
}

pub type ProcessRule = ac::ProcessRule;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AwarenessConfig {
    pub enable_cmdline_rules: bool,
    pub enable_cgroup_rules: bool,
    pub enable_parent_child_inference: bool,
    pub enable_pid_allowlist: bool,
    pub interactive_default: BTreeSet<WorkloadTag>,
    pub tool_executor_default: BTreeSet<WorkloadTag>,
    pub background_default: BTreeSet<WorkloadTag>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ClassifierConfig {
    pub awareness: AwarenessConfig,
    pub process_rules: Vec<ProcessRule>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct RuntimeOrchestratorConfig {
    pub runtime: RuntimeConfig,
    pub classifier: ClassifierConfig,
    pub safety: SafetyConfig,
    pub scenarios: BTreeMap<ScenarioKind, ScenarioPolicy>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RuntimeConfigProfile {
    LocalDemo,
    Named(String),
}

impl RuntimeConfigProfile {
    pub fn local_demo() -> Self {
        Self::LocalDemo
    }

    pub fn named(name: impl AsRef<str>) -> Result<Self, ConfigError> {
        Ok(Self::Named(validate_profile_name(name.as_ref())?))
    }

    pub fn name(&self) -> &str {
        match self {
            Self::LocalDemo => "local_demo",
            Self::Named(name) => name,
        }
    }
}

impl RuntimeOrchestratorConfig {
    pub fn load_from_repo_root(root: impl AsRef<Path>) -> Result<Self, ConfigError> {
        Self::load_from_repo_root_with_profile(root, &RuntimeConfigProfile::LocalDemo)
    }

    pub fn load_profile_from_repo_root(
        root: impl AsRef<Path>,
        profile_name: impl AsRef<str>,
    ) -> Result<Self, ConfigError> {
        let profile = RuntimeConfigProfile::named(profile_name)?;
        Self::load_from_repo_root_with_profile(root, &profile)
    }

    pub fn load_from_repo_root_with_profile(
        root: impl AsRef<Path>,
        profile: &RuntimeConfigProfile,
    ) -> Result<Self, ConfigError> {
        let paths = config_paths_for_profile(root.as_ref(), profile)?;
        Self::load_from_paths(paths)
    }

    fn load_from_paths(paths: ConfigPaths) -> Result<Self, ConfigError> {
        let strict = paths.strict_schema;
        let runtime_document = read_config_file(paths.runtime)?;
        let classifier_document = read_config_file(paths.classifier)?;
        let awareness_document = read_config_file(paths.awareness)?;
        let safety_document = read_config_file(paths.safety)?;
        let inference_tail_guard_document = read_config_file(paths.inference_tail_guard)?;
        let tool_call_booster_document = read_config_file(paths.tool_call_booster)?;

        let runtime = parse_runtime_config(&runtime_document, strict)?;
        let mut classifier = parse_classifier_config(&classifier_document, strict)?;
        merge_awareness_defaults(&mut classifier.awareness, &awareness_document, strict)?;
        let safety = parse_safety_config(&safety_document, strict)?;

        let mut scenarios = BTreeMap::new();
        let inference_policy = parse_scenario_policy(
            &inference_tail_guard_document,
            ScenarioKind::InferenceTailGuard,
            strict,
        )?;
        let tool_policy = parse_scenario_policy(
            &tool_call_booster_document,
            ScenarioKind::ToolCallBooster,
            strict,
        )?;

        if strict {
            validate_cross_file_safety(
                &runtime_document,
                &safety_document,
                &[
                    (&inference_tail_guard_document, &inference_policy),
                    (&tool_call_booster_document, &tool_policy),
                ],
                &runtime,
                &safety,
            )?;
        }

        scenarios.insert(ScenarioKind::InferenceTailGuard, inference_policy);
        scenarios.insert(ScenarioKind::ToolCallBooster, tool_policy);

        Ok(Self {
            runtime,
            classifier,
            safety,
            scenarios,
        })
    }
}

fn validate_cross_file_safety(
    runtime_document: &ConfigDocument,
    safety_document: &ConfigDocument,
    scenario_documents: &[(&ConfigDocument, &ScenarioPolicy)],
    runtime: &RuntimeConfig,
    safety: &SafetyConfig,
) -> Result<(), ConfigError> {
    for (scenario_document, policy) in scenario_documents {
        if !policy.enabled {
            continue;
        }

        validate_scenario_limits_against_safety(
            safety_document,
            scenario_document,
            safety,
            policy,
        )?;
        validate_scenario_triggers_against_focus_signals(
            runtime_document,
            scenario_document,
            runtime,
            policy,
        )?;
        validate_live_action_scope(runtime_document, scenario_document, runtime, policy)?;
    }
    Ok(())
}

fn validate_scenario_limits_against_safety(
    safety_document: &ConfigDocument,
    scenario_document: &ConfigDocument,
    safety: &SafetyConfig,
    policy: &ScenarioPolicy,
) -> Result<(), ConfigError> {
    if policy.max_boost_duration_ms > safety.max_boost_duration_ms {
        return Err(cross_file_error(
            scenario_document,
            safety_document,
            "policy",
            "max_boost_duration_ms",
            format!(
                "must be <= global_safety.max_boost_duration_ms ({})",
                safety.max_boost_duration_ms
            ),
        ));
    }

    if let Some(delta) = policy.actions.raise_nice {
        let max_delta = safety.normalized_max_priority_delta();
        if delta.abs() > max_delta {
            return Err(cross_file_error(
                scenario_document,
                safety_document,
                format!("actions.{}", policy.scenario.as_str()),
                "raise_nice",
                format!("absolute value must be <= global_safety.max_priority_delta ({max_delta})"),
            ));
        }
    }

    Ok(())
}

fn validate_scenario_triggers_against_focus_signals(
    runtime_document: &ConfigDocument,
    scenario_document: &ConfigDocument,
    runtime: &RuntimeConfig,
    policy: &ScenarioPolicy,
) -> Result<(), ConfigError> {
    for signal in required_focus_signals(policy) {
        if !runtime.focus_signals.contains(&signal) {
            return Err(cross_file_error(
                scenario_document,
                runtime_document,
                format!("triggers.{}", policy.scenario.as_str()),
                trigger_key_for_signal(&signal),
                format!(
                    "requires collection.focus_signals to include `{}`",
                    signal.as_str()
                ),
            ));
        }
    }

    Ok(())
}

fn validate_live_action_scope(
    runtime_document: &ConfigDocument,
    scenario_document: &ConfigDocument,
    runtime: &RuntimeConfig,
    policy: &ScenarioPolicy,
) -> Result<(), ConfigError> {
    let action_section = format!("actions.{}", policy.scenario.as_str());

    if policy.actions.pin_strategy.is_some() && !has_live_pid_allowlist_scope(runtime) {
        return Err(cross_file_error(
            scenario_document,
            runtime_document,
            &action_section,
            "pin_strategy",
            "live affinity requires selection.mode = \"pid_allowlist\" and a non-empty pid_allowlist",
        ));
    }

    if policy.actions.use_cpuset == Some(true) {
        return Err(cross_file_error(
            scenario_document,
            runtime_document,
            &action_section,
            "use_cpuset",
            "live cpuset writes are disabled",
        ));
    }

    Ok(())
}

fn has_live_pid_allowlist_scope(runtime: &RuntimeConfig) -> bool {
    runtime.selection_mode == "pid_allowlist" && !runtime.pid_allowlist.is_empty()
}

fn required_focus_signals(policy: &ScenarioPolicy) -> Vec<SignalKind> {
    let triggers = &policy.triggers;
    let mut signals = Vec::new();
    if triggers.run_queue_delay_us.is_some() {
        signals.push(SignalKind::RunQueueDelay);
    }
    if triggers.offcpu_spike_us.is_some() {
        signals.push(SignalKind::OffCpuTime);
    }
    if triggers.cpu_migrations_per_sec.is_some() {
        signals.push(SignalKind::CpuMigration);
    }
    if triggers.major_page_faults_per_sec.is_some() {
        signals.push(SignalKind::MajorPageFault);
    }
    if triggers.subprocess_start_delay_us.is_some() {
        signals.push(SignalKind::SubprocessStartDelay);
    }
    if triggers.queue_wait_us.is_some() {
        signals.push(SignalKind::QueueWait);
    }
    if triggers.optional_io_latency_us.is_some() {
        signals.push(SignalKind::IoLatency);
    }
    signals
}

fn trigger_key_for_signal(signal: &SignalKind) -> &'static str {
    match signal {
        SignalKind::RunQueueDelay => "run_queue_delay_us",
        SignalKind::OffCpuTime => "offcpu_spike_us",
        SignalKind::CpuMigration => "cpu_migrations_per_sec",
        SignalKind::MajorPageFault => "major_page_faults_per_sec",
        SignalKind::SubprocessStartDelay => "subprocess_start_delay_us",
        SignalKind::QueueWait => "queue_wait_us",
        SignalKind::IoLatency => "optional_io_latency_us",
        SignalKind::Unknown(_) => "*",
    }
}

#[derive(Debug)]
struct ConfigPaths {
    strict_schema: bool,
    runtime: ConfigFile,
    classifier: ConfigFile,
    awareness: ConfigFile,
    safety: ConfigFile,
    inference_tail_guard: ConfigFile,
    tool_call_booster: ConfigFile,
}

#[derive(Debug)]
struct ConfigFile {
    profile: String,
    path: PathBuf,
}

impl ConfigFile {
    fn new(profile: &RuntimeConfigProfile, path: PathBuf) -> Self {
        Self {
            profile: profile.name().to_string(),
            path,
        }
    }
}

#[derive(Debug)]
struct ConfigDocument {
    profile: String,
    path: PathBuf,
    contents: String,
}

impl ConfigDocument {
    fn context(&self) -> ConfigContext<'_> {
        ConfigContext {
            profile: &self.profile,
            path: &self.path,
        }
    }
}

#[derive(Clone, Copy)]
struct ConfigContext<'a> {
    profile: &'a str,
    path: &'a Path,
}

impl ConfigContext<'_> {
    fn error(
        self,
        section: impl AsRef<str>,
        key: impl AsRef<str>,
        constraint: impl AsRef<str>,
    ) -> ConfigError {
        ConfigError::new(format!(
            "config schema error: profile `{}` file {} section `{}` key `{}` violates constraint: {}",
            self.profile,
            self.path.display(),
            section.as_ref(),
            key.as_ref(),
            constraint.as_ref()
        ))
    }
}

fn cross_file_error(
    primary: &ConfigDocument,
    related: &ConfigDocument,
    section: impl AsRef<str>,
    key: impl AsRef<str>,
    constraint: impl AsRef<str>,
) -> ConfigError {
    ConfigError::new(format!(
        "config cross-file error: profile `{}` files {} and {} section `{}` key `{}` violates constraint: {}",
        primary.profile,
        primary.path.display(),
        related.path.display(),
        section.as_ref(),
        key.as_ref(),
        constraint.as_ref()
    ))
}

fn config_paths_for_profile(
    root: &Path,
    profile: &RuntimeConfigProfile,
) -> Result<ConfigPaths, ConfigError> {
    match profile {
        RuntimeConfigProfile::LocalDemo => Ok(ConfigPaths {
            strict_schema: false,
            runtime: ConfigFile::new(profile, root.join("configs/runtime/runtime.example.toml")),
            classifier: ConfigFile::new(
                profile,
                root.join("configs/classifier/process_rules.example.toml"),
            ),
            awareness: ConfigFile::new(
                profile,
                root.join("configs/scenarios/ai_workload_awareness.example.toml"),
            ),
            safety: ConfigFile::new(profile, root.join("configs/safety/default.toml")),
            inference_tail_guard: ConfigFile::new(
                profile,
                root.join("configs/scenarios/inference_tail_guard.example.toml"),
            ),
            tool_call_booster: ConfigFile::new(
                profile,
                root.join("configs/scenarios/tool_call_booster.example.toml"),
            ),
        }),
        RuntimeConfigProfile::Named(name) => {
            let profile_root = root.join("configs/profiles").join(name);
            if !profile_root.is_dir() {
                return Err(ConfigError::new(format!(
                    "missing config profile `{name}` root: {}",
                    profile_root.display()
                )));
            }
            Ok(ConfigPaths {
                strict_schema: true,
                runtime: ConfigFile::new(profile, profile_root.join("runtime.toml")),
                classifier: ConfigFile::new(
                    profile,
                    profile_root.join("classifier/process_rules.toml"),
                ),
                awareness: ConfigFile::new(
                    profile,
                    profile_root.join("scenarios/ai_workload_awareness.toml"),
                ),
                safety: ConfigFile::new(profile, profile_root.join("safety/default.toml")),
                inference_tail_guard: ConfigFile::new(
                    profile,
                    profile_root.join("scenarios/inference_tail_guard.toml"),
                ),
                tool_call_booster: ConfigFile::new(
                    profile,
                    profile_root.join("scenarios/tool_call_booster.toml"),
                ),
            })
        }
    }
}

fn validate_profile_name(raw: &str) -> Result<String, ConfigError> {
    let name = raw.trim();
    if name.is_empty() {
        return Err(ConfigError::new("config profile name cannot be empty"));
    }
    if name.contains('/') || name.contains('\\') {
        return Err(ConfigError::new(format!(
            "config profile `{name}` must not contain path separators"
        )));
    }
    if name.contains('.') {
        return Err(ConfigError::new(format!(
            "config profile `{name}` must not contain dot segments"
        )));
    }
    if !name
        .chars()
        .all(|ch| ch.is_ascii_alphanumeric() || ch == '_')
    {
        return Err(ConfigError::new(format!(
            "config profile `{name}` must be an ASCII identifier"
        )));
    }
    Ok(name.to_string())
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConfigError {
    message: String,
}

impl ConfigError {
    fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
        }
    }

    fn missing(file: &ConfigFile) -> Self {
        Self::new(format!(
            "missing config file for profile `{}`: {}",
            file.profile,
            file.path.display()
        ))
    }
}

impl fmt::Display for ConfigError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.message)
    }
}

impl std::error::Error for ConfigError {}

fn read_config_file(file: ConfigFile) -> Result<ConfigDocument, ConfigError> {
    let contents = fs::read_to_string(&file.path).map_err(|_| ConfigError::missing(&file))?;
    Ok(ConfigDocument {
        profile: file.profile,
        path: file.path,
        contents,
    })
}

fn parse_runtime_config(
    document: &ConfigDocument,
    strict: bool,
) -> Result<RuntimeConfig, ConfigError> {
    let ctx = document.context();
    let mut section = String::new();
    let mut deployment_target = None;
    let mut kernel_min = None;
    let mut cgroup_version = None;
    let mut primary_runtime = None;
    let mut fallback_runtime = None;
    let mut selection_mode = None;
    let mut process_names = None;
    let mut pid_allowlist = None;
    let mut focus_signals = None;
    let mut tracked_metrics = None;

    for line in normalize_lines(&document.contents) {
        if let Some(table) = parse_table_name(&line) {
            if strict
                && !matches!(
                    table.as_str(),
                    "target" | "runtime" | "selection" | "collection" | "metrics"
                )
            {
                return Err(ctx.error(table, "*", "known runtime config section"));
            }
            section = table;
            continue;
        }

        let (key, value) = parse_assignment(&line)?;
        match (section.as_str(), key) {
            ("target", "deployment_target") => deployment_target = Some(parse_string(value)?),
            ("target", "kernel_min") => kernel_min = Some(parse_string(value)?),
            ("target", "cgroup_version") => cgroup_version = Some(parse_string(value)?),
            ("runtime", "primary_runtime") => primary_runtime = Some(parse_string(value)?),
            ("runtime", "fallback_runtime") => fallback_runtime = Some(parse_string(value)?),
            ("selection", "mode") => {
                selection_mode = Some(parse_runtime_selection_mode(value, ctx, strict)?)
            }
            ("selection", "process_names") => process_names = Some(parse_string_array(value)?),
            ("selection", "pid_allowlist") => {
                pid_allowlist = Some(parse_u32_array(value)?.into_iter().collect());
            }
            ("collection", "focus_signals") => {
                focus_signals = Some(parse_signal_set(
                    value,
                    ctx,
                    "collection",
                    "focus_signals",
                    strict,
                )?);
            }
            ("metrics", "track") => tracked_metrics = Some(parse_string_array(value)?),
            _ if strict => return Err(ctx.error(&section, key, "known runtime config key")),
            _ => {}
        }
    }

    let deployment_target = required_string_if_strict(
        deployment_target,
        ctx,
        "target",
        "deployment_target",
        strict,
    )?;
    let kernel_min = required_string_if_strict(kernel_min, ctx, "target", "kernel_min", strict)?;
    let cgroup_version =
        required_string_if_strict(cgroup_version, ctx, "target", "cgroup_version", strict)?;
    let primary_runtime =
        required_string_if_strict(primary_runtime, ctx, "runtime", "primary_runtime", strict)?;
    let fallback_runtime =
        required_string_if_strict(fallback_runtime, ctx, "runtime", "fallback_runtime", strict)?;
    let selection_mode =
        required_string_if_strict(selection_mode, ctx, "selection", "mode", strict)?;
    let process_names = required_value_if_strict(
        process_names,
        ctx,
        "selection",
        "process_names",
        "required field",
        strict,
    )?
    .unwrap_or_default();
    let pid_allowlist = required_value_if_strict(
        pid_allowlist,
        ctx,
        "selection",
        "pid_allowlist",
        "required field",
        strict,
    )?
    .unwrap_or_default();
    let focus_signals = required_value_if_strict(
        focus_signals,
        ctx,
        "collection",
        "focus_signals",
        "required field",
        strict,
    )?
    .unwrap_or_default();
    let tracked_metrics = required_value_if_strict(
        tracked_metrics,
        ctx,
        "metrics",
        "track",
        "required field",
        strict,
    )?
    .unwrap_or_default();

    Ok(RuntimeConfig {
        deployment_target,
        kernel_min,
        cgroup_version,
        primary_runtime,
        fallback_runtime,
        selection_mode,
        process_names,
        pid_allowlist,
        focus_signals,
        tracked_metrics,
    })
}

fn parse_classifier_config(
    document: &ConfigDocument,
    strict: bool,
) -> Result<ClassifierConfig, ConfigError> {
    if strict {
        validate_classifier_schema(document)?;
    }
    let process_rules = ac::ClassifierConfig::from_toml_str(&document.contents)
        .map_err(|error| ConfigError::new(error.to_string()))?
        .process_rules;

    Ok(ClassifierConfig {
        awareness: default_awareness_config(),
        process_rules,
    })
}

fn validate_classifier_schema(document: &ConfigDocument) -> Result<(), ConfigError> {
    let ctx = document.context();
    let mut saw_rule = false;
    let mut has_matcher = false;
    let mut has_tags = false;

    for line in normalize_lines(&document.contents) {
        if line == "[[process_rules]]" {
            if saw_rule {
                validate_classifier_rule(ctx, has_matcher, has_tags)?;
            }
            saw_rule = true;
            has_matcher = false;
            has_tags = false;
            continue;
        }

        if let Some(table) = parse_table_name(&line) {
            return Err(ctx.error(
                table,
                "*",
                "classifier config supports [[process_rules]] only",
            ));
        }

        let (key, _) = parse_assignment(&line)?;
        if !saw_rule {
            return Err(ctx.error("process_rules", key, "key must be inside [[process_rules]]"));
        }

        match key {
            "id" => {}
            "name"
            | "process_name"
            | "cmdline_contains"
            | "cgroup_contains"
            | "pids"
            | "pid_allowlist"
            | "tag_markers"
            | "parent_name"
            | "parent_process_name"
            | "parent_cmdline_contains"
            | "parent_has_any_tags" => has_matcher = true,
            "tags" => has_tags = true,
            other => {
                return Err(ctx.error("process_rules", other, "known classifier process rule key"))
            }
        }
    }

    if !saw_rule {
        return Err(ctx.error("process_rules", "*", "at least one [[process_rules]] entry"));
    }
    validate_classifier_rule(ctx, has_matcher, has_tags)
}

fn validate_classifier_rule(
    ctx: ConfigContext<'_>,
    has_matcher: bool,
    has_tags: bool,
) -> Result<(), ConfigError> {
    if !has_matcher {
        return Err(ctx.error("process_rules", "*", "at least one matcher"));
    }
    if !has_tags {
        return Err(ctx.error("process_rules", "tags", "required field"));
    }
    Ok(())
}

fn merge_awareness_defaults(
    awareness: &mut AwarenessConfig,
    document: &ConfigDocument,
    strict: bool,
) -> Result<(), ConfigError> {
    let ctx = document.context();
    let mut section = String::new();
    let mut enable_cmdline_rules = None;
    let mut enable_cgroup_rules = None;
    let mut enable_parent_child_inference = None;
    let mut enable_pid_allowlist = None;
    let mut interactive_default = None;
    let mut tool_executor_default = None;
    let mut background_default = None;

    for line in normalize_lines(&document.contents) {
        if let Some(table) = parse_table_name(&line) {
            if strict && !matches!(table.as_str(), "classifier" | "labels") {
                return Err(ctx.error(table, "*", "known awareness config section"));
            }
            section = table;
            continue;
        }

        let (key, value) = parse_assignment(&line)?;
        match (section.as_str(), key) {
            ("classifier", "enable_cmdline_rules") => {
                enable_cmdline_rules = Some(parse_bool(value)?)
            }
            ("classifier", "enable_cgroup_rules") => enable_cgroup_rules = Some(parse_bool(value)?),
            ("classifier", "enable_parent_child_inference") => {
                enable_parent_child_inference = Some(parse_bool(value)?)
            }
            ("classifier", "enable_pid_allowlist") => {
                enable_pid_allowlist = Some(parse_bool(value)?)
            }
            ("labels", "interactive_default") => interactive_default = Some(parse_tag_set(value)?),
            ("labels", "tool_executor_default") => {
                tool_executor_default = Some(parse_tag_set(value)?)
            }
            ("labels", "background_default") => background_default = Some(parse_tag_set(value)?),
            _ if strict => return Err(ctx.error(&section, key, "known awareness config key")),
            _ => {}
        }
    }

    if strict {
        awareness.enable_cmdline_rules = required_value(
            enable_cmdline_rules,
            ctx,
            "classifier",
            "enable_cmdline_rules",
            "required field",
        )?;
        awareness.enable_cgroup_rules = required_value(
            enable_cgroup_rules,
            ctx,
            "classifier",
            "enable_cgroup_rules",
            "required field",
        )?;
        awareness.enable_parent_child_inference = required_value(
            enable_parent_child_inference,
            ctx,
            "classifier",
            "enable_parent_child_inference",
            "required field",
        )?;
        awareness.enable_pid_allowlist = required_value(
            enable_pid_allowlist,
            ctx,
            "classifier",
            "enable_pid_allowlist",
            "required field",
        )?;
        awareness.interactive_default = required_value(
            interactive_default,
            ctx,
            "labels",
            "interactive_default",
            "required field",
        )?;
        awareness.tool_executor_default = required_value(
            tool_executor_default,
            ctx,
            "labels",
            "tool_executor_default",
            "required field",
        )?;
        awareness.background_default = required_value(
            background_default,
            ctx,
            "labels",
            "background_default",
            "required field",
        )?;
    } else {
        if let Some(value) = enable_cmdline_rules {
            awareness.enable_cmdline_rules = value;
        }
        if let Some(value) = enable_cgroup_rules {
            awareness.enable_cgroup_rules = value;
        }
        if let Some(value) = enable_parent_child_inference {
            awareness.enable_parent_child_inference = value;
        }
        if let Some(value) = enable_pid_allowlist {
            awareness.enable_pid_allowlist = value;
        }
        if let Some(value) = interactive_default {
            awareness.interactive_default = value;
        }
        if let Some(value) = tool_executor_default {
            awareness.tool_executor_default = value;
        }
        if let Some(value) = background_default {
            awareness.background_default = value;
        }
    }

    Ok(())
}

fn parse_safety_config(
    document: &ConfigDocument,
    strict: bool,
) -> Result<SafetyConfig, ConfigError> {
    let ctx = document.context();
    let mut section = String::new();
    let mut require_revert = None;
    let mut allow_background_throttle = None;
    let mut max_priority_delta = None;
    let mut max_boost_duration_ms = None;
    let mut max_affinity_change_ratio = None;

    for line in normalize_lines(&document.contents) {
        if let Some(table) = parse_table_name(&line) {
            if strict && table != "global_safety" {
                return Err(ctx.error(table, "*", "known safety config section"));
            }
            section = table;
            continue;
        }

        let (key, value) = parse_assignment(&line)?;
        match (section.as_str(), key) {
            ("global_safety", "require_revert") => require_revert = Some(parse_bool(value)?),
            ("global_safety", "allow_background_throttle") => {
                allow_background_throttle = Some(parse_bool(value)?)
            }
            ("global_safety", "max_priority_delta") => {
                let parsed = parse_i32(value)?;
                if strict && !(0..=20).contains(&parsed) {
                    return Err(ctx.error(
                        "global_safety",
                        "max_priority_delta",
                        "integer in range 0..=20",
                    ));
                }
                max_priority_delta = Some(parsed);
            }
            ("global_safety", "max_boost_duration_ms") => {
                max_boost_duration_ms = Some(parse_positive_duration_ms(
                    value,
                    ctx,
                    "global_safety",
                    key,
                    strict,
                )?)
            }
            ("global_safety", "max_affinity_change_ratio") => {
                let parsed = parse_f32(value)?;
                if strict && !(0.0..=1.0).contains(&parsed) {
                    return Err(ctx.error(
                        "global_safety",
                        "max_affinity_change_ratio",
                        "finite float in range 0.0..=1.0",
                    ));
                }
                max_affinity_change_ratio = Some(parsed);
            }
            _ if strict => return Err(ctx.error(&section, key, "known safety config key")),
            _ => {}
        }
    }

    let require_revert = required_value_if_strict(
        require_revert,
        ctx,
        "global_safety",
        "require_revert",
        "required field",
        strict,
    )?
    .unwrap_or(true);
    let allow_background_throttle = required_value_if_strict(
        allow_background_throttle,
        ctx,
        "global_safety",
        "allow_background_throttle",
        "required field",
        strict,
    )?
    .unwrap_or(false);
    let max_priority_delta = required_value_if_strict(
        max_priority_delta,
        ctx,
        "global_safety",
        "max_priority_delta",
        "required field",
        strict,
    )?
    .unwrap_or(0);
    let max_boost_duration_ms = required_value_if_strict(
        max_boost_duration_ms,
        ctx,
        "global_safety",
        "max_boost_duration_ms",
        "required field",
        strict,
    )?
    .unwrap_or(0);
    let max_affinity_change_ratio = required_value_if_strict(
        max_affinity_change_ratio,
        ctx,
        "global_safety",
        "max_affinity_change_ratio",
        "required field",
        strict,
    )?
    .unwrap_or(0.0);

    Ok(SafetyConfig {
        require_revert,
        allow_background_throttle,
        max_priority_delta,
        max_boost_duration_ms,
        max_affinity_change_ratio,
    })
}

fn parse_scenario_policy(
    document: &ConfigDocument,
    scenario: ScenarioKind,
    strict: bool,
) -> Result<ScenarioPolicy, ConfigError> {
    let ctx = document.context();
    let mut section = String::new();
    let mut active_scenarios = None;
    let mut evaluation_window_ms = None;
    let mut cooldown_ms = None;
    let mut max_boost_duration_ms = None;
    let mut triggers = TriggerThresholds::default();
    let mut actions = ScenarioActions::default();
    let trigger_section = format!("triggers.{}", scenario.as_str());
    let action_section = format!("actions.{}", scenario.as_str());

    for line in normalize_lines(&document.contents) {
        if let Some(table) = parse_table_name(&line) {
            if strict && table != "policy" && table != trigger_section && table != action_section {
                return Err(ctx.error(table, "*", "known scenario policy section"));
            }
            section = table;
            continue;
        }

        let (key, value) = parse_assignment(&line)?;
        match (section.as_str(), key) {
            ("policy", "active_scenarios") => {
                active_scenarios = Some(parse_scenario_array(value, ctx, "policy", key, strict)?);
            }
            ("policy", "evaluation_window_ms") => {
                evaluation_window_ms = Some(parse_positive_duration_ms(
                    value, ctx, "policy", key, strict,
                )?)
            }
            ("policy", "cooldown_ms") => {
                cooldown_ms = Some(parse_positive_duration_ms(
                    value, ctx, "policy", key, strict,
                )?)
            }
            ("policy", "max_boost_duration_ms") => {
                max_boost_duration_ms = Some(parse_positive_duration_ms(
                    value, ctx, "policy", key, strict,
                )?)
            }
            _ if section == trigger_section => match key {
                "run_queue_delay_us" => {
                    triggers.run_queue_delay_us = Some(parse_positive_duration_ms(
                        value, ctx, &section, key, strict,
                    )?)
                }
                "offcpu_spike_us" => {
                    triggers.offcpu_spike_us = Some(parse_positive_duration_ms(
                        value, ctx, &section, key, strict,
                    )?)
                }
                "cpu_migrations_per_sec" => {
                    triggers.cpu_migrations_per_sec = Some(parse_positive_duration_ms(
                        value, ctx, &section, key, strict,
                    )?)
                }
                "major_page_faults_per_sec" => {
                    triggers.major_page_faults_per_sec = Some(parse_positive_duration_ms(
                        value, ctx, &section, key, strict,
                    )?)
                }
                "subprocess_start_delay_us" => {
                    triggers.subprocess_start_delay_us = Some(parse_positive_duration_ms(
                        value, ctx, &section, key, strict,
                    )?)
                }
                "queue_wait_us" => {
                    triggers.queue_wait_us = Some(parse_positive_duration_ms(
                        value, ctx, &section, key, strict,
                    )?)
                }
                "optional_io_latency_us" => {
                    triggers.optional_io_latency_us = Some(parse_positive_duration_ms(
                        value, ctx, &section, key, strict,
                    )?)
                }
                _ if strict => return Err(ctx.error(&section, key, "known trigger key")),
                _ => {}
            },
            _ if section == action_section => match key {
                "raise_nice" => {
                    let parsed = parse_i32(value)?;
                    if strict && !(-20..=19).contains(&parsed) {
                        return Err(ctx.error(&section, key, "integer in range -20..=19"));
                    }
                    actions.raise_nice = Some(parsed);
                }
                "pin_strategy" => {
                    actions.pin_strategy =
                        Some(parse_pin_strategy(value, ctx, &section, key, strict)?);
                }
                "use_cpuset" => actions.use_cpuset = Some(parse_bool(value)?),
                "warmup_executor" => actions.warmup_executor = Some(parse_bool(value)?),
                _ if strict => return Err(ctx.error(&section, key, "known action key")),
                _ => {}
            },
            _ if strict => return Err(ctx.error(&section, key, "known scenario policy key")),
            _ => {}
        }
    }

    let active_scenarios = required_value_if_strict(
        active_scenarios,
        ctx,
        "policy",
        "active_scenarios",
        "required field",
        strict,
    )?
    .unwrap_or_default();
    let enabled = active_scenarios.iter().any(|item| item == &scenario);
    let evaluation_window_ms = required_value_if_strict(
        evaluation_window_ms,
        ctx,
        "policy",
        "evaluation_window_ms",
        "required field",
        strict,
    )?
    .unwrap_or(0);
    let cooldown_ms = required_value_if_strict(
        cooldown_ms,
        ctx,
        "policy",
        "cooldown_ms",
        "required field",
        strict,
    )?
    .unwrap_or(0);
    let max_boost_duration_ms = required_value_if_strict(
        max_boost_duration_ms,
        ctx,
        "policy",
        "max_boost_duration_ms",
        "required field",
        strict,
    )?
    .unwrap_or(0);

    Ok(ScenarioPolicy {
        scenario,
        enabled,
        evaluation_window_ms,
        cooldown_ms,
        max_boost_duration_ms,
        triggers,
        actions,
    })
}

fn default_awareness_config() -> AwarenessConfig {
    AwarenessConfig {
        enable_cmdline_rules: true,
        enable_cgroup_rules: true,
        enable_parent_child_inference: true,
        enable_pid_allowlist: true,
        interactive_default: default_interactive_tags(),
        tool_executor_default: default_tool_executor_tags(),
        background_default: default_background_tags(),
    }
}

fn default_interactive_tags() -> BTreeSet<WorkloadTag> {
    [
        WorkloadTag::AiInference,
        WorkloadTag::InteractiveLatencySensitive,
    ]
    .into_iter()
    .collect()
}

fn default_tool_executor_tags() -> BTreeSet<WorkloadTag> {
    [WorkloadTag::ToolCall].into_iter().collect()
}

fn default_background_tags() -> BTreeSet<WorkloadTag> {
    [WorkloadTag::BackgroundJob].into_iter().collect()
}

fn normalize_lines(input: &str) -> Vec<String> {
    let mut lines = Vec::new();
    let mut multiline = String::new();
    let mut collecting_array = false;

    for raw in input.lines() {
        let line = raw.split('#').next().unwrap_or("").trim();
        if line.is_empty() {
            continue;
        }

        if collecting_array {
            multiline.push(' ');
            multiline.push_str(line);
            if line.contains(']') {
                lines.push(multiline.trim().to_string());
                multiline.clear();
                collecting_array = false;
            }
            continue;
        }

        let starts_array = line.contains('=') && line.contains('[') && !line.contains(']');
        if starts_array {
            multiline.push_str(line);
            collecting_array = true;
            continue;
        }

        lines.push(line.to_string());
    }

    if !multiline.is_empty() {
        lines.push(multiline.trim().to_string());
    }

    lines
}

fn parse_table_name(line: &str) -> Option<String> {
    if line.starts_with("[[") || !line.starts_with('[') || !line.ends_with(']') {
        return None;
    }
    Some(line[1..line.len() - 1].trim().to_string())
}

fn parse_assignment(line: &str) -> Result<(&str, &str), ConfigError> {
    line.split_once('=')
        .map(|(key, value)| (key.trim(), value.trim()))
        .ok_or_else(|| ConfigError::new(format!("invalid config line: {line}")))
}

fn parse_string(raw: &str) -> Result<String, ConfigError> {
    let trimmed = raw.trim();
    if trimmed.starts_with('"') && trimmed.ends_with('"') && trimmed.len() >= 2 {
        Ok(trimmed[1..trimmed.len() - 1].to_string())
    } else {
        Err(ConfigError::new(format!("expected string, got {raw}")))
    }
}

fn parse_string_array(raw: &str) -> Result<Vec<String>, ConfigError> {
    let trimmed = raw.trim();
    if !trimmed.starts_with('[') || !trimmed.ends_with(']') {
        return Err(ConfigError::new(format!(
            "expected string array, got {raw}"
        )));
    }

    let inner = trimmed[1..trimmed.len() - 1].trim();
    if inner.is_empty() {
        return Ok(Vec::new());
    }

    inner
        .split(',')
        .map(|item| parse_string(item.trim()))
        .collect()
}

fn parse_tag_set(raw: &str) -> Result<BTreeSet<WorkloadTag>, ConfigError> {
    Ok(parse_string_array(raw)?
        .into_iter()
        .map(|value| WorkloadTag::parse(&value))
        .collect())
}

fn parse_runtime_selection_mode(
    raw: &str,
    ctx: ConfigContext<'_>,
    strict: bool,
) -> Result<String, ConfigError> {
    let mode = parse_string(raw)?;
    if strict && mode != "process_name" && mode != "pid_allowlist" {
        return Err(ctx.error("selection", "mode", "one of process_name, pid_allowlist"));
    }
    Ok(mode)
}

fn parse_signal_set(
    raw: &str,
    ctx: ConfigContext<'_>,
    section: &str,
    key: &str,
    strict: bool,
) -> Result<BTreeSet<SignalKind>, ConfigError> {
    parse_string_array(raw)?
        .into_iter()
        .map(|signal| {
            let parsed = SignalKind::parse(&signal);
            if strict && matches!(parsed, SignalKind::Unknown(_)) {
                return Err(ctx.error(section, key, format!("known signal `{signal}`")));
            }
            Ok(parsed)
        })
        .collect()
}

fn parse_scenario_array(
    raw: &str,
    ctx: ConfigContext<'_>,
    section: &str,
    key: &str,
    strict: bool,
) -> Result<Vec<ScenarioKind>, ConfigError> {
    parse_string_array(raw)?
        .into_iter()
        .map(|scenario| {
            let parsed = ScenarioKind::parse(&scenario);
            if strict && matches!(parsed, ScenarioKind::Unknown(_)) {
                return Err(ctx.error(section, key, format!("known scenario `{scenario}`")));
            }
            Ok(parsed)
        })
        .collect()
}

fn parse_pin_strategy(
    raw: &str,
    ctx: ConfigContext<'_>,
    section: &str,
    key: &str,
    strict: bool,
) -> Result<PinStrategy, ConfigError> {
    let value = parse_string(raw)?;
    let strategy = PinStrategy::parse(&value);
    if strict && matches!(strategy, PinStrategy::Unknown(_)) {
        return Err(ctx.error(section, key, format!("known pin strategy `{value}`")));
    }
    Ok(strategy)
}

fn parse_positive_duration_ms(
    raw: &str,
    ctx: ConfigContext<'_>,
    section: &str,
    key: &str,
    strict: bool,
) -> Result<u64, ConfigError> {
    let value = parse_u64(raw)?;
    if strict && value == 0 {
        return Err(ctx.error(section, key, "positive duration"));
    }
    Ok(value)
}

fn required_string_if_strict(
    value: Option<String>,
    ctx: ConfigContext<'_>,
    section: &str,
    key: &str,
    strict: bool,
) -> Result<String, ConfigError> {
    Ok(
        required_value_if_strict(value, ctx, section, key, "required field", strict)?
            .unwrap_or_default(),
    )
}

fn required_value<T>(
    value: Option<T>,
    ctx: ConfigContext<'_>,
    section: &str,
    key: &str,
    constraint: &str,
) -> Result<T, ConfigError> {
    value.ok_or_else(|| ctx.error(section, key, constraint))
}

fn required_value_if_strict<T>(
    value: Option<T>,
    ctx: ConfigContext<'_>,
    section: &str,
    key: &str,
    constraint: &str,
    strict: bool,
) -> Result<Option<T>, ConfigError> {
    if strict {
        required_value(value, ctx, section, key, constraint).map(Some)
    } else {
        Ok(value)
    }
}

fn parse_u32_array(raw: &str) -> Result<Vec<u32>, ConfigError> {
    let trimmed = raw.trim();
    if !trimmed.starts_with('[') || !trimmed.ends_with(']') {
        return Err(ConfigError::new(format!(
            "expected integer array, got {raw}"
        )));
    }

    let inner = trimmed[1..trimmed.len() - 1].trim();
    if inner.is_empty() {
        return Ok(Vec::new());
    }

    inner
        .split(',')
        .map(|item| {
            item.trim()
                .parse::<u32>()
                .map_err(|_| ConfigError::new(format!("expected u32, got {}", item.trim())))
        })
        .collect()
}

fn parse_bool(raw: &str) -> Result<bool, ConfigError> {
    raw.trim()
        .parse::<bool>()
        .map_err(|_| ConfigError::new(format!("expected bool, got {raw}")))
}

fn parse_i32(raw: &str) -> Result<i32, ConfigError> {
    raw.trim()
        .parse::<i32>()
        .map_err(|_| ConfigError::new(format!("expected i32, got {raw}")))
}

fn parse_u64(raw: &str) -> Result<u64, ConfigError> {
    raw.trim()
        .parse::<u64>()
        .map_err(|_| ConfigError::new(format!("expected u64, got {raw}")))
}

fn parse_f32(raw: &str) -> Result<f32, ConfigError> {
    raw.trim()
        .parse::<f32>()
        .map_err(|_| ConfigError::new(format!("expected f32, got {raw}")))
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::path::{Path, PathBuf};

    use super::*;

    fn repo_root() -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .ancestors()
            .nth(2)
            .expect("crate lives under agent/runtime_orchestrator")
            .to_path_buf()
    }

    fn temp_repo_root(name: &str) -> PathBuf {
        let root = std::env::temp_dir().join(format!(
            "aegisai-runtime-profile-{name}-{}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&root);
        root
    }

    fn copy_profile_fixture(root: &Path, profile: &str) {
        let source = repo_root().join("configs");
        let profile_root = root.join("configs/profiles").join(profile);
        fs::create_dir_all(profile_root.join("classifier")).expect("classifier dir");
        fs::create_dir_all(profile_root.join("scenarios")).expect("scenarios dir");
        fs::create_dir_all(profile_root.join("safety")).expect("safety dir");

        fs::copy(
            source.join("runtime/runtime.example.toml"),
            profile_root.join("runtime.toml"),
        )
        .expect("runtime profile copy");
        fs::copy(
            source.join("classifier/process_rules.example.toml"),
            profile_root.join("classifier/process_rules.toml"),
        )
        .expect("classifier profile copy");
        fs::copy(
            source.join("scenarios/ai_workload_awareness.example.toml"),
            profile_root.join("scenarios/ai_workload_awareness.toml"),
        )
        .expect("awareness profile copy");
        fs::copy(
            source.join("safety/default.toml"),
            profile_root.join("safety/default.toml"),
        )
        .expect("safety profile copy");
        fs::copy(
            source.join("scenarios/inference_tail_guard.example.toml"),
            profile_root.join("scenarios/inference_tail_guard.toml"),
        )
        .expect("inference profile copy");
        fs::copy(
            source.join("scenarios/tool_call_booster.example.toml"),
            profile_root.join("scenarios/tool_call_booster.toml"),
        )
        .expect("tool profile copy");
        overwrite_profile_file(
            root,
            profile,
            "runtime.toml",
            r#"
[target]
deployment_target = "linux"
kernel_min = "5.15"
cgroup_version = "v2"

[runtime]
primary_runtime = "ollama"
fallback_runtime = "llama.cpp"

[selection]
mode = "pid_allowlist"
process_names = []
pid_allowlist = [4242]

[collection]
focus_signals = [
  "run_queue_delay",
  "offcpu_time",
  "cpu_migration",
  "major_page_fault",
  "subprocess_start_delay",
  "queue_wait",
  "io_latency"
]

[metrics]
track = [
  "ttft",
  "p95_latency",
  "p99_latency",
  "jitter",
  "boost_hit_rate",
  "rollback_count",
  "side_effect_rate"
]
"#,
        );
        overwrite_profile_file(
            root,
            profile,
            "scenarios/tool_call_booster.toml",
            r#"
[policy]
active_scenarios = ["tool_call_booster"]
evaluation_window_ms = 300
cooldown_ms = 800
max_boost_duration_ms = 800

[triggers.tool_call_booster]
subprocess_start_delay_us = 1500
queue_wait_us = 2000
optional_io_latency_us = 4000

[actions.tool_call_booster]
raise_nice = -3
pin_strategy = "prefer_low_contention_cores"
warmup_executor = true
"#,
        );
    }

    fn load_profile_error(root: &Path, profile: &str) -> String {
        RuntimeOrchestratorConfig::load_profile_from_repo_root(root, profile)
            .expect_err("invalid profile should fail")
            .to_string()
    }

    fn overwrite_profile_file(root: &Path, profile: &str, relative_path: &str, contents: &str) {
        fs::write(
            root.join("configs/profiles")
                .join(profile)
                .join(relative_path),
            contents,
        )
        .expect("profile file overwrite");
    }

    fn assert_schema_error_context(
        error: &str,
        profile: &str,
        file: &str,
        section: &str,
        key: &str,
        constraint: &str,
    ) {
        assert!(error.contains("config schema error"), "{error}");
        assert!(error.contains(&format!("profile `{profile}`")), "{error}");
        assert!(error.contains(file), "{error}");
        assert!(error.contains(&format!("section `{section}`")), "{error}");
        assert!(error.contains(&format!("key `{key}`")), "{error}");
        assert!(error.contains(constraint), "{error}");
    }

    fn assert_cross_file_error_context(
        error: &str,
        profile: &str,
        primary_file: &str,
        related_file: &str,
        section: &str,
        key: &str,
        constraint: &str,
    ) {
        assert!(error.contains("config cross-file error"), "{error}");
        assert!(error.contains(&format!("profile `{profile}`")), "{error}");
        assert!(error.contains(primary_file), "{error}");
        assert!(error.contains(related_file), "{error}");
        assert!(error.contains(&format!("section `{section}`")), "{error}");
        assert!(error.contains(&format!("key `{key}`")), "{error}");
        assert!(error.contains(constraint), "{error}");
    }

    #[test]
    fn selected_profile_loads_non_example_config_files() {
        let root = temp_repo_root("selected");
        copy_profile_fixture(&root, "production");

        let config = RuntimeOrchestratorConfig::load_profile_from_repo_root(&root, "production")
            .expect("profile should load");

        assert_eq!(config.runtime.primary_runtime, "ollama");
        assert_eq!(config.classifier.process_rules.len(), 7);
        assert!(config
            .scenarios
            .contains_key(&ScenarioKind::InferenceTailGuard));
        assert!(config
            .scenarios
            .contains_key(&ScenarioKind::ToolCallBooster));
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn local_demo_profile_preserves_example_config_compatibility() {
        let config = RuntimeOrchestratorConfig::load_from_repo_root_with_profile(
            repo_root(),
            &RuntimeConfigProfile::local_demo(),
        )
        .expect("local demo config should load");

        assert_eq!(config.runtime.primary_runtime, "ollama");
        assert_eq!(config.classifier.process_rules.len(), 7);
    }

    #[test]
    fn local_demo_profile_still_ignores_unknown_example_keys() {
        let document = ConfigDocument {
            profile: RuntimeConfigProfile::local_demo().name().to_string(),
            path: PathBuf::from("configs/runtime/runtime.example.toml"),
            contents: r#"
[target]
deployment_target = "linux"
unexpected = "kept compatible"
"#
            .to_string(),
        };

        let config =
            parse_runtime_config(&document, false).expect("local demo parser stays permissive");

        assert_eq!(config.deployment_target, "linux");
        assert!(config.primary_runtime.is_empty());
    }

    #[test]
    fn production_unknown_key_error_includes_schema_context() {
        let root = temp_repo_root("unknown-key");
        copy_profile_fixture(&root, "production");
        overwrite_profile_file(
            &root,
            "production",
            "runtime.toml",
            r#"
[target]
deployment_target = "linux"
kernel_min = "5.15"
cgroup_version = "v2"
unknown_target = "bad"

[runtime]
primary_runtime = "ollama"
fallback_runtime = "llama.cpp"

[selection]
mode = "process_name"
process_names = ["ollama"]
pid_allowlist = []

[collection]
focus_signals = ["run_queue_delay"]

[metrics]
track = ["ttft"]
"#,
        );

        let error = load_profile_error(&root, "production");

        assert_schema_error_context(
            &error,
            "production",
            "runtime.toml",
            "target",
            "unknown_target",
            "known runtime config key",
        );
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn production_missing_required_field_error_includes_schema_context() {
        let root = temp_repo_root("missing-field");
        copy_profile_fixture(&root, "production");
        overwrite_profile_file(
            &root,
            "production",
            "runtime.toml",
            r#"
[target]
deployment_target = "linux"
kernel_min = "5.15"
cgroup_version = "v2"

[runtime]
fallback_runtime = "llama.cpp"

[selection]
mode = "process_name"
process_names = ["ollama"]
pid_allowlist = []

[collection]
focus_signals = ["run_queue_delay"]

[metrics]
track = ["ttft"]
"#,
        );

        let error = load_profile_error(&root, "production");

        assert_schema_error_context(
            &error,
            "production",
            "runtime.toml",
            "runtime",
            "primary_runtime",
            "required field",
        );
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn production_classifier_unknown_key_error_includes_schema_context() {
        let root = temp_repo_root("classifier-unknown-key");
        copy_profile_fixture(&root, "production");
        overwrite_profile_file(
            &root,
            "production",
            "classifier/process_rules.toml",
            r#"
[[process_rules]]
id = "ollama"
name = "ollama"
tags = ["AI_INFERENCE"]
unexpected = "bad"
"#,
        );

        let error = load_profile_error(&root, "production");

        assert_schema_error_context(
            &error,
            "production",
            "classifier/process_rules.toml",
            "process_rules",
            "unexpected",
            "known classifier process rule key",
        );
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn production_invalid_focus_signal_is_rejected() {
        let root = temp_repo_root("invalid-focus-signal");
        copy_profile_fixture(&root, "production");
        overwrite_profile_file(
            &root,
            "production",
            "runtime.toml",
            r#"
[target]
deployment_target = "linux"
kernel_min = "5.15"
cgroup_version = "v2"

[runtime]
primary_runtime = "ollama"
fallback_runtime = "llama.cpp"

[selection]
mode = "process_name"
process_names = ["ollama"]
pid_allowlist = []

[collection]
focus_signals = ["not_a_signal"]

[metrics]
track = ["ttft"]
"#,
        );

        let error = load_profile_error(&root, "production");

        assert_schema_error_context(
            &error,
            "production",
            "runtime.toml",
            "collection",
            "focus_signals",
            "known signal `not_a_signal`",
        );
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn production_invalid_scenario_enum_is_rejected() {
        let root = temp_repo_root("invalid-scenario");
        copy_profile_fixture(&root, "production");
        overwrite_profile_file(
            &root,
            "production",
            "scenarios/inference_tail_guard.toml",
            r#"
[policy]
active_scenarios = ["unknown_scenario"]
evaluation_window_ms = 500
cooldown_ms = 1500
max_boost_duration_ms = 800

[triggers.inference_tail_guard]
run_queue_delay_us = 2000

[actions.inference_tail_guard]
raise_nice = -5
"#,
        );

        let error = load_profile_error(&root, "production");

        assert_schema_error_context(
            &error,
            "production",
            "scenarios/inference_tail_guard.toml",
            "policy",
            "active_scenarios",
            "known scenario `unknown_scenario`",
        );
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn production_invalid_action_enum_is_rejected() {
        let root = temp_repo_root("invalid-action");
        copy_profile_fixture(&root, "production");
        overwrite_profile_file(
            &root,
            "production",
            "scenarios/inference_tail_guard.toml",
            r#"
[policy]
active_scenarios = ["inference_tail_guard"]
evaluation_window_ms = 500
cooldown_ms = 1500
max_boost_duration_ms = 800

[triggers.inference_tail_guard]
run_queue_delay_us = 2000

[actions.inference_tail_guard]
pin_strategy = "spread_everywhere"
"#,
        );

        let error = load_profile_error(&root, "production");

        assert_schema_error_context(
            &error,
            "production",
            "scenarios/inference_tail_guard.toml",
            "actions.inference_tail_guard",
            "pin_strategy",
            "known pin strategy `spread_everywhere`",
        );
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn production_invalid_raise_nice_is_rejected() {
        let root = temp_repo_root("invalid-raise-nice");
        copy_profile_fixture(&root, "production");
        overwrite_profile_file(
            &root,
            "production",
            "scenarios/tool_call_booster.toml",
            r#"
[policy]
active_scenarios = ["tool_call_booster"]
evaluation_window_ms = 300
cooldown_ms = 800
max_boost_duration_ms = 1200

[triggers.tool_call_booster]
queue_wait_us = 2000

[actions.tool_call_booster]
raise_nice = -21
"#,
        );

        let error = load_profile_error(&root, "production");

        assert_schema_error_context(
            &error,
            "production",
            "scenarios/tool_call_booster.toml",
            "actions.tool_call_booster",
            "raise_nice",
            "integer in range -20..=19",
        );
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn production_invalid_duration_is_rejected() {
        let root = temp_repo_root("invalid-duration");
        copy_profile_fixture(&root, "production");
        overwrite_profile_file(
            &root,
            "production",
            "scenarios/inference_tail_guard.toml",
            r#"
[policy]
active_scenarios = ["inference_tail_guard"]
evaluation_window_ms = 0
cooldown_ms = 1500
max_boost_duration_ms = 800

[triggers.inference_tail_guard]
run_queue_delay_us = 2000

[actions.inference_tail_guard]
raise_nice = -5
"#,
        );

        let error = load_profile_error(&root, "production");

        assert_schema_error_context(
            &error,
            "production",
            "scenarios/inference_tail_guard.toml",
            "policy",
            "evaluation_window_ms",
            "positive duration",
        );
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn production_scenario_duration_above_global_max_is_rejected() {
        let root = temp_repo_root("cross-file-duration");
        copy_profile_fixture(&root, "production");
        overwrite_profile_file(
            &root,
            "production",
            "scenarios/inference_tail_guard.toml",
            r#"
[policy]
active_scenarios = ["inference_tail_guard"]
evaluation_window_ms = 500
cooldown_ms = 1500
max_boost_duration_ms = 900

[triggers.inference_tail_guard]
run_queue_delay_us = 2000
offcpu_spike_us = 3000
cpu_migrations_per_sec = 10
major_page_faults_per_sec = 3

[actions.inference_tail_guard]
raise_nice = -5
pin_strategy = "prefer_reserved_cores"
use_cpuset = false
"#,
        );

        let error = load_profile_error(&root, "production");

        assert_cross_file_error_context(
            &error,
            "production",
            "scenarios/inference_tail_guard.toml",
            "safety/default.toml",
            "policy",
            "max_boost_duration_ms",
            "global_safety.max_boost_duration_ms (800)",
        );
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn production_priority_delta_outside_global_max_is_rejected() {
        let root = temp_repo_root("cross-file-priority");
        copy_profile_fixture(&root, "production");
        overwrite_profile_file(
            &root,
            "production",
            "scenarios/inference_tail_guard.toml",
            r#"
[policy]
active_scenarios = ["inference_tail_guard"]
evaluation_window_ms = 500
cooldown_ms = 1500
max_boost_duration_ms = 800

[triggers.inference_tail_guard]
run_queue_delay_us = 2000

[actions.inference_tail_guard]
raise_nice = -6
pin_strategy = "prefer_reserved_cores"
use_cpuset = false
"#,
        );

        let error = load_profile_error(&root, "production");

        assert_cross_file_error_context(
            &error,
            "production",
            "scenarios/inference_tail_guard.toml",
            "safety/default.toml",
            "actions.inference_tail_guard",
            "raise_nice",
            "global_safety.max_priority_delta (5)",
        );
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn production_trigger_requiring_absent_focus_signal_is_rejected() {
        let root = temp_repo_root("cross-file-focus");
        copy_profile_fixture(&root, "production");
        overwrite_profile_file(
            &root,
            "production",
            "runtime.toml",
            r#"
[target]
deployment_target = "linux"
kernel_min = "5.15"
cgroup_version = "v2"

[runtime]
primary_runtime = "ollama"
fallback_runtime = "llama.cpp"

[selection]
mode = "pid_allowlist"
process_names = []
pid_allowlist = [4242]

[collection]
focus_signals = ["run_queue_delay"]

[metrics]
track = ["ttft"]
"#,
        );

        let error = load_profile_error(&root, "production");

        assert_cross_file_error_context(
            &error,
            "production",
            "scenarios/inference_tail_guard.toml",
            "runtime.toml",
            "triggers.inference_tail_guard",
            "offcpu_spike_us",
            "collection.focus_signals to include `offcpu_time`",
        );
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn production_live_affinity_requires_pid_allowlist_mode() {
        let root = temp_repo_root("cross-file-affinity-mode");
        copy_profile_fixture(&root, "production");
        overwrite_profile_file(
            &root,
            "production",
            "runtime.toml",
            r#"
[target]
deployment_target = "linux"
kernel_min = "5.15"
cgroup_version = "v2"

[runtime]
primary_runtime = "ollama"
fallback_runtime = "llama.cpp"

[selection]
mode = "process_name"
process_names = ["ollama"]
pid_allowlist = []

[collection]
focus_signals = [
  "run_queue_delay",
  "offcpu_time",
  "cpu_migration",
  "major_page_fault",
  "subprocess_start_delay",
  "queue_wait",
  "io_latency"
]

[metrics]
track = ["ttft"]
"#,
        );

        let error = load_profile_error(&root, "production");

        assert_cross_file_error_context(
            &error,
            "production",
            "scenarios/inference_tail_guard.toml",
            "runtime.toml",
            "actions.inference_tail_guard",
            "pin_strategy",
            "selection.mode = \"pid_allowlist\" and a non-empty pid_allowlist",
        );
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn production_live_cpuset_true_is_rejected() {
        let root = temp_repo_root("cross-file-cpuset");
        copy_profile_fixture(&root, "production");
        overwrite_profile_file(
            &root,
            "production",
            "scenarios/inference_tail_guard.toml",
            r#"
[policy]
active_scenarios = ["inference_tail_guard"]
evaluation_window_ms = 500
cooldown_ms = 1500
max_boost_duration_ms = 800

[triggers.inference_tail_guard]
run_queue_delay_us = 2000

[actions.inference_tail_guard]
raise_nice = -5
pin_strategy = "prefer_reserved_cores"
use_cpuset = true
"#,
        );

        let error = load_profile_error(&root, "production");

        assert_cross_file_error_context(
            &error,
            "production",
            "scenarios/inference_tail_guard.toml",
            "runtime.toml",
            "actions.inference_tail_guard",
            "use_cpuset",
            "live cpuset writes are disabled",
        );
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn profile_names_are_identifier_only() {
        for invalid in [
            "",
            "  ",
            "/prod",
            "prod/live",
            "prod\\live",
            ".",
            "..",
            "prod.v1",
        ] {
            let error = RuntimeConfigProfile::named(invalid)
                .expect_err("invalid profile name should fail")
                .to_string();
            assert!(error.contains("profile"));
        }

        let profile = RuntimeConfigProfile::named("prod_1").expect("identifier should parse");
        assert_eq!(profile.name(), "prod_1");
    }

    #[test]
    fn missing_selected_profile_root_fails_before_file_reads() {
        let root = temp_repo_root("missing");
        fs::create_dir_all(root.join("configs/profiles")).expect("profiles dir");

        let error = RuntimeOrchestratorConfig::load_profile_from_repo_root(&root, "production")
            .expect_err("missing profile should fail")
            .to_string();

        assert!(error.contains("missing config profile `production` root"));
        assert!(error.contains("configs/profiles/production"));
        let _ = fs::remove_dir_all(root);
    }
}
