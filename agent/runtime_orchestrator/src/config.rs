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
        let runtime = parse_runtime_config(&read(paths.runtime)?)?;
        let mut classifier = parse_classifier_config(&read(paths.classifier)?)?;
        merge_awareness_defaults(&mut classifier.awareness, &read(paths.awareness)?)?;
        let safety = parse_safety_config(&read(paths.safety)?)?;

        let mut scenarios = BTreeMap::new();
        let inference_policy = parse_scenario_policy(
            &read(paths.inference_tail_guard)?,
            ScenarioKind::InferenceTailGuard,
        )?;
        scenarios.insert(ScenarioKind::InferenceTailGuard, inference_policy);

        let tool_policy = parse_scenario_policy(
            &read(paths.tool_call_booster)?,
            ScenarioKind::ToolCallBooster,
        )?;
        scenarios.insert(ScenarioKind::ToolCallBooster, tool_policy);

        Ok(Self {
            runtime,
            classifier,
            safety,
            scenarios,
        })
    }
}

#[derive(Debug)]
struct ConfigPaths {
    runtime: PathBuf,
    classifier: PathBuf,
    awareness: PathBuf,
    safety: PathBuf,
    inference_tail_guard: PathBuf,
    tool_call_booster: PathBuf,
}

fn config_paths_for_profile(
    root: &Path,
    profile: &RuntimeConfigProfile,
) -> Result<ConfigPaths, ConfigError> {
    match profile {
        RuntimeConfigProfile::LocalDemo => Ok(ConfigPaths {
            runtime: root.join("configs/runtime/runtime.example.toml"),
            classifier: root.join("configs/classifier/process_rules.example.toml"),
            awareness: root.join("configs/scenarios/ai_workload_awareness.example.toml"),
            safety: root.join("configs/safety/default.toml"),
            inference_tail_guard: root.join("configs/scenarios/inference_tail_guard.example.toml"),
            tool_call_booster: root.join("configs/scenarios/tool_call_booster.example.toml"),
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
                runtime: profile_root.join("runtime.toml"),
                classifier: profile_root.join("classifier/process_rules.toml"),
                awareness: profile_root.join("scenarios/ai_workload_awareness.toml"),
                safety: profile_root.join("safety/default.toml"),
                inference_tail_guard: profile_root.join("scenarios/inference_tail_guard.toml"),
                tool_call_booster: profile_root.join("scenarios/tool_call_booster.toml"),
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

    fn missing(path: &Path) -> Self {
        Self::new(format!("missing config file: {}", path.display()))
    }
}

impl fmt::Display for ConfigError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.message)
    }
}

impl std::error::Error for ConfigError {}

fn read(path: PathBuf) -> Result<String, ConfigError> {
    fs::read_to_string(&path).map_err(|_| ConfigError::missing(&path))
}

fn parse_runtime_config(input: &str) -> Result<RuntimeConfig, ConfigError> {
    let mut section = String::new();
    let mut deployment_target = String::new();
    let mut kernel_min = String::new();
    let mut cgroup_version = String::new();
    let mut primary_runtime = String::new();
    let mut fallback_runtime = String::new();
    let mut selection_mode = String::new();
    let mut process_names = Vec::new();
    let mut pid_allowlist = BTreeSet::new();
    let mut focus_signals = BTreeSet::new();
    let mut tracked_metrics = Vec::new();

    for line in normalize_lines(input) {
        if let Some(table) = parse_table_name(&line) {
            section = table;
            continue;
        }

        let (key, value) = parse_assignment(&line)?;
        match (section.as_str(), key) {
            ("target", "deployment_target") => deployment_target = parse_string(value)?,
            ("target", "kernel_min") => kernel_min = parse_string(value)?,
            ("target", "cgroup_version") => cgroup_version = parse_string(value)?,
            ("runtime", "primary_runtime") => primary_runtime = parse_string(value)?,
            ("runtime", "fallback_runtime") => fallback_runtime = parse_string(value)?,
            ("selection", "mode") => selection_mode = parse_string(value)?,
            ("selection", "process_names") => process_names = parse_string_array(value)?,
            ("selection", "pid_allowlist") => {
                pid_allowlist = parse_u32_array(value)?.into_iter().collect();
            }
            ("collection", "focus_signals") => {
                focus_signals = parse_string_array(value)?
                    .into_iter()
                    .map(|signal| SignalKind::parse(&signal))
                    .collect();
            }
            ("metrics", "track") => tracked_metrics = parse_string_array(value)?,
            _ => {}
        }
    }

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

fn parse_classifier_config(input: &str) -> Result<ClassifierConfig, ConfigError> {
    let process_rules = ac::ClassifierConfig::from_toml_str(input)
        .map_err(|error| ConfigError::new(error.to_string()))?
        .process_rules;

    Ok(ClassifierConfig {
        awareness: default_awareness_config(),
        process_rules,
    })
}

fn merge_awareness_defaults(
    awareness: &mut AwarenessConfig,
    input: &str,
) -> Result<(), ConfigError> {
    let mut section = String::new();

    for line in normalize_lines(input) {
        if let Some(table) = parse_table_name(&line) {
            section = table;
            continue;
        }

        let (key, value) = parse_assignment(&line)?;
        match (section.as_str(), key) {
            ("classifier", "enable_cmdline_rules") => {
                awareness.enable_cmdline_rules = parse_bool(value)?
            }
            ("classifier", "enable_cgroup_rules") => {
                awareness.enable_cgroup_rules = parse_bool(value)?
            }
            ("classifier", "enable_parent_child_inference") => {
                awareness.enable_parent_child_inference = parse_bool(value)?
            }
            ("classifier", "enable_pid_allowlist") => {
                awareness.enable_pid_allowlist = parse_bool(value)?
            }
            ("labels", "interactive_default") => {
                awareness.interactive_default = parse_tag_set(value)?
            }
            ("labels", "tool_executor_default") => {
                awareness.tool_executor_default = parse_tag_set(value)?
            }
            ("labels", "background_default") => {
                awareness.background_default = parse_tag_set(value)?
            }
            _ => {}
        }
    }

    Ok(())
}

fn parse_safety_config(input: &str) -> Result<SafetyConfig, ConfigError> {
    let mut section = String::new();
    let mut require_revert = true;
    let mut allow_background_throttle = false;
    let mut max_priority_delta = 0;
    let mut max_boost_duration_ms = 0;
    let mut max_affinity_change_ratio = 0.0;

    for line in normalize_lines(input) {
        if let Some(table) = parse_table_name(&line) {
            section = table;
            continue;
        }

        let (key, value) = parse_assignment(&line)?;
        match (section.as_str(), key) {
            ("global_safety", "require_revert") => require_revert = parse_bool(value)?,
            ("global_safety", "allow_background_throttle") => {
                allow_background_throttle = parse_bool(value)?
            }
            ("global_safety", "max_priority_delta") => max_priority_delta = parse_i32(value)?,
            ("global_safety", "max_boost_duration_ms") => max_boost_duration_ms = parse_u64(value)?,
            ("global_safety", "max_affinity_change_ratio") => {
                max_affinity_change_ratio = parse_f32(value)?
            }
            _ => {}
        }
    }

    Ok(SafetyConfig {
        require_revert,
        allow_background_throttle,
        max_priority_delta,
        max_boost_duration_ms,
        max_affinity_change_ratio,
    })
}

fn parse_scenario_policy(
    input: &str,
    scenario: ScenarioKind,
) -> Result<ScenarioPolicy, ConfigError> {
    let mut section = String::new();
    let mut enabled = false;
    let mut evaluation_window_ms = 0;
    let mut cooldown_ms = 0;
    let mut max_boost_duration_ms = 0;
    let mut triggers = TriggerThresholds::default();
    let mut actions = ScenarioActions::default();

    for line in normalize_lines(input) {
        if let Some(table) = parse_table_name(&line) {
            section = table;
            continue;
        }

        let (key, value) = parse_assignment(&line)?;
        match (section.as_str(), key) {
            ("policy", "active_scenarios") => {
                let active = parse_string_array(value)?
                    .into_iter()
                    .map(|item| ScenarioKind::parse(&item))
                    .collect::<Vec<_>>();
                enabled = active.iter().any(|item| item == &scenario);
            }
            ("policy", "evaluation_window_ms") => evaluation_window_ms = parse_u64(value)?,
            ("policy", "cooldown_ms") => cooldown_ms = parse_u64(value)?,
            ("policy", "max_boost_duration_ms") => max_boost_duration_ms = parse_u64(value)?,
            _ if section == format!("triggers.{}", scenario.as_str()) => match key {
                "run_queue_delay_us" => triggers.run_queue_delay_us = Some(parse_u64(value)?),
                "offcpu_spike_us" => triggers.offcpu_spike_us = Some(parse_u64(value)?),
                "cpu_migrations_per_sec" => {
                    triggers.cpu_migrations_per_sec = Some(parse_u64(value)?)
                }
                "major_page_faults_per_sec" => {
                    triggers.major_page_faults_per_sec = Some(parse_u64(value)?)
                }
                "subprocess_start_delay_us" => {
                    triggers.subprocess_start_delay_us = Some(parse_u64(value)?)
                }
                "queue_wait_us" => triggers.queue_wait_us = Some(parse_u64(value)?),
                "optional_io_latency_us" => {
                    triggers.optional_io_latency_us = Some(parse_u64(value)?)
                }
                _ => {}
            },
            _ if section == format!("actions.{}", scenario.as_str()) => match key {
                "raise_nice" => actions.raise_nice = Some(parse_i32(value)?),
                "pin_strategy" => {
                    actions.pin_strategy = Some(PinStrategy::parse(&parse_string(value)?))
                }
                "use_cpuset" => actions.use_cpuset = Some(parse_bool(value)?),
                "warmup_executor" => actions.warmup_executor = Some(parse_bool(value)?),
                _ => {}
            },
            _ => {}
        }
    }

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
