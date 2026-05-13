use std::collections::{BTreeMap, BTreeSet};
use std::env;
use std::fs::OpenOptions;
use std::io::Write;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

use aegisai_actuator::{
    Actuator, LinuxActuatorBackend, LiveLinuxCommandGuard, NoopActuatorBackend,
};
use aegisai_runtime_daemon::{
    LinuxProbeSource, MockEventSource, NoopMetadataProvider, ProbeReaderConfig,
    ProcfsMetadataProvider, RuntimeLoop, RuntimeLoopConfig, StaticMetadataProvider,
};
use runtime_orchestrator::{RuntimeConfigProfile, RuntimeOrchestrator, RuntimeOrchestratorConfig};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let cli =
        CliConfig::parse_with_env(env::args().skip(1), env::var("AEGISAI_CONFIG_PROFILE").ok())?;

    let config = RuntimeOrchestratorConfig::load_from_repo_root_with_profile(
        &cli.repo_root,
        &cli.config_profile,
    )?;
    let runtime_config = runtime_config_for_source(&cli, &config);
    let config = config_for_actuator_scope(&cli, config);
    let mut orchestrator =
        RuntimeOrchestrator::with_actuator(config.clone(), build_actuator(&cli, &config)?)?;
    let runtime_loop = RuntimeLoop::new(RuntimeLoopConfig {
        batch_size: cli.batch_size,
        tick_interval_ms: cli.tick_interval_ms,
        drain_after_source_ms: cli.drain_after_source_ms,
        max_events: cli.max_events,
    })?;

    let summary = match (cli.source.as_str(), cli.metadata.as_str()) {
        ("mock", "demo") => {
            let mut source = build_mock_source(&cli.mock_profile)?;
            let mut metadata = StaticMetadataProvider::demo();
            runtime_loop.run(&mut orchestrator, &mut source, &mut metadata)?
        }
        ("mock", "noop") => {
            let mut source = build_mock_source(&cli.mock_profile)?;
            let mut metadata = NoopMetadataProvider;
            runtime_loop.run(&mut orchestrator, &mut source, &mut metadata)?
        }
        ("mock", "procfs") => {
            let mut source = build_mock_source(&cli.mock_profile)?;
            let mut metadata = procfs_metadata_provider_for_mock()?;
            runtime_loop.run(&mut orchestrator, &mut source, &mut metadata)?
        }
        ("linux", "procfs") => {
            let mut source = LinuxProbeSource::from_runtime_with_config(
                &runtime_config,
                cli.probe_reader_config(),
            )?;
            let mut metadata = procfs_metadata_provider();
            runtime_loop.run(&mut orchestrator, &mut source, &mut metadata)?
        }
        ("linux", "demo") | ("linux", "noop") => {
            return Err(format!(
                "metadata mode `{}` is not supported with source mode `linux`",
                cli.metadata
            )
            .into())
        }
        (source, _) => return Err(format!("unsupported source mode `{source}`").into()),
    };

    print_summary(&summary);
    if let Some(log_path) = cli.verification_log.as_ref() {
        append_summary_to_log(log_path, &summary)?;
    }
    Ok(())
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct CliConfig {
    repo_root: PathBuf,
    config_profile: RuntimeConfigProfile,
    source: String,
    mock_profile: String,
    metadata: String,
    actuator_backend: String,
    confirm_live_actuator: bool,
    enable_live_affinity: bool,
    live_pid_allowlist: BTreeSet<u32>,
    warmup_executor_program: Option<String>,
    warmup_executor_args: Vec<String>,
    warmup_executor_timeout_ms: u64,
    require_all_probes: bool,
    probe_buffer_events: usize,
    probe_poll_timeout_ms: u64,
    batch_size: usize,
    max_events: Option<u64>,
    tick_interval_ms: u64,
    drain_after_source_ms: u64,
    verification_log: Option<PathBuf>,
}

impl Default for CliConfig {
    fn default() -> Self {
        Self {
            repo_root: env::current_dir().unwrap_or_else(|_| PathBuf::from(".")),
            config_profile: RuntimeConfigProfile::local_demo(),
            source: "mock".to_string(),
            mock_profile: "demo".to_string(),
            metadata: "demo".to_string(),
            actuator_backend: "noop".to_string(),
            confirm_live_actuator: false,
            enable_live_affinity: false,
            live_pid_allowlist: BTreeSet::new(),
            warmup_executor_program: None,
            warmup_executor_args: Vec::new(),
            warmup_executor_timeout_ms: 250,
            require_all_probes: true,
            probe_buffer_events: 4_096,
            probe_poll_timeout_ms: 100,
            batch_size: 32,
            max_events: None,
            tick_interval_ms: 200,
            drain_after_source_ms: 5_000,
            verification_log: None,
        }
    }
}

impl CliConfig {
    #[cfg(test)]
    fn parse<I>(args: I) -> Result<Self, String>
    where
        I: IntoIterator<Item = String>,
    {
        Self::parse_with_env(args, None)
    }

    fn parse_with_env<I>(args: I, env_profile: Option<String>) -> Result<Self, String>
    where
        I: IntoIterator<Item = String>,
    {
        let mut config = Self::default();
        if let Some(profile) = env_profile {
            config.config_profile = parse_config_profile(&profile)?;
        }
        let mut args = args.into_iter();

        while let Some(arg) = args.next() {
            match arg.as_str() {
                "--repo-root" => {
                    let value = args
                        .next()
                        .ok_or_else(|| "--repo-root expects a path".to_string())?;
                    config.repo_root = PathBuf::from(value);
                }
                "--config-profile" => {
                    let value = args
                        .next()
                        .ok_or_else(|| "--config-profile expects a profile name".to_string())?;
                    config.config_profile = parse_config_profile(&value)?;
                }
                "--source" => {
                    let value = args
                        .next()
                        .ok_or_else(|| "--source expects `mock` or `linux`".to_string())?;
                    match value.as_str() {
                        "mock" | "linux" => config.source = value,
                        other => {
                            return Err(format!(
                                "unsupported source mode `{other}`; expected `mock` or `linux`"
                            ));
                        }
                    }
                }
                "--mock-profile" => {
                    config.mock_profile = args.next().ok_or_else(|| {
                        "--mock-profile expects `demo` or `tool-call-lifecycle`".to_string()
                    })?;
                }
                "--metadata" => {
                    config.metadata = args.next().ok_or_else(|| {
                        "--metadata expects `demo`, `noop`, or `procfs`".to_string()
                    })?;
                }
                "--actuator-backend" => {
                    let value = args.next().ok_or_else(|| {
                        "--actuator-backend expects `noop`, `linux-skeleton`, `linux-command`, or `linux-command-dry-run`"
                            .to_string()
                    })?;
                    match value.as_str() {
                        "noop" | "linux-skeleton" | "linux-command" | "linux-command-dry-run" => {
                            config.actuator_backend = value;
                        }
                        other => {
                            return Err(format!(
                                "unsupported actuator backend `{other}`; expected `noop`, `linux-skeleton`, `linux-command`, or `linux-command-dry-run`"
                            ));
                        }
                    }
                }
                "--allow-partial-probes" => {
                    config.require_all_probes = false;
                }
                "--confirm-live-actuator" => {
                    config.confirm_live_actuator = true;
                }
                "--enable-live-affinity" => {
                    config.enable_live_affinity = true;
                }
                "--live-pid-allowlist" => {
                    let value = args.next().ok_or_else(|| {
                        "--live-pid-allowlist expects a comma-separated PID list".to_string()
                    })?;
                    config.live_pid_allowlist = parse_pid_allowlist(&value)?;
                }
                "--warmup-executor-command" => {
                    let value = args.next().ok_or_else(|| {
                        "--warmup-executor-command expects a program path".to_string()
                    })?;
                    if value.trim().is_empty() {
                        return Err("--warmup-executor-command expects a non-empty program path"
                            .to_string());
                    }
                    config.warmup_executor_program = Some(value);
                }
                "--warmup-executor-arg" => {
                    let value = args
                        .next()
                        .ok_or_else(|| "--warmup-executor-arg expects a value".to_string())?;
                    config.warmup_executor_args.push(value);
                }
                "--warmup-executor-timeout-ms" => {
                    let value = args
                        .next()
                        .ok_or_else(|| {
                            "--warmup-executor-timeout-ms expects a positive integer".to_string()
                        })?
                        .parse::<u64>()
                        .map_err(|_| {
                            "--warmup-executor-timeout-ms expects a positive integer".to_string()
                        })?;
                    if value == 0 {
                        return Err(
                            "--warmup-executor-timeout-ms expects a positive integer".to_string()
                        );
                    }
                    config.warmup_executor_timeout_ms = value;
                }
                "--probe-buffer-events" => {
                    config.probe_buffer_events = args
                        .next()
                        .ok_or_else(|| "--probe-buffer-events expects an integer".to_string())?
                        .parse()
                        .map_err(|_| "--probe-buffer-events expects an integer".to_string())?;
                }
                "--probe-poll-timeout-ms" => {
                    config.probe_poll_timeout_ms = args
                        .next()
                        .ok_or_else(|| "--probe-poll-timeout-ms expects an integer".to_string())?
                        .parse()
                        .map_err(|_| "--probe-poll-timeout-ms expects an integer".to_string())?;
                }
                "--batch-size" => {
                    config.batch_size = args
                        .next()
                        .ok_or_else(|| "--batch-size expects an integer".to_string())?
                        .parse()
                        .map_err(|_| "--batch-size expects an integer".to_string())?;
                }
                "--max-events" => {
                    let value = args
                        .next()
                        .ok_or_else(|| "--max-events expects a positive integer".to_string())?
                        .parse::<u64>()
                        .map_err(|_| "--max-events expects a positive integer".to_string())?;
                    if value == 0 {
                        return Err("--max-events expects a positive integer".to_string());
                    }
                    config.max_events = Some(value);
                }
                "--tick-ms" => {
                    config.tick_interval_ms = args
                        .next()
                        .ok_or_else(|| "--tick-ms expects an integer".to_string())?
                        .parse()
                        .map_err(|_| "--tick-ms expects an integer".to_string())?;
                }
                "--drain-ms" => {
                    config.drain_after_source_ms = args
                        .next()
                        .ok_or_else(|| "--drain-ms expects an integer".to_string())?
                        .parse()
                        .map_err(|_| "--drain-ms expects an integer".to_string())?;
                }
                "--verification-log" => {
                    let value = args
                        .next()
                        .ok_or_else(|| "--verification-log expects a path".to_string())?;
                    config.verification_log = Some(PathBuf::from(value));
                }
                "--help" | "-h" => return Err(Self::usage()),
                other => return Err(format!("unknown argument `{other}`\n\n{}", Self::usage())),
            }
        }

        if config.warmup_executor_program.is_none() && !config.warmup_executor_args.is_empty() {
            return Err("--warmup-executor-arg requires --warmup-executor-command".to_string());
        }

        Ok(config)
    }

    fn usage() -> String {
        [
            "Usage: aegisai-runtime-daemon [options]",
            "",
            "Options:",
            "  --repo-root <path>   Repository root containing configs/ (default: current dir)",
            "  --config-profile <name>  Production config profile under configs/profiles/<name>/; overrides AEGISAI_CONFIG_PROFILE",
            "  --source <mode>      Source mode: mock | linux (default: mock)",
            "  --mock-profile <name>  Mock source profile: demo | tool-call-lifecycle (default: demo)",
            "  --metadata <mode>    Metadata mode: demo | noop | procfs (default: demo)",
            "  --actuator-backend <mode>  Backend mode: noop | linux-skeleton | linux-command | linux-command-dry-run (default: noop)",
            "  --confirm-live-actuator  Required before linux-command may execute host renice/taskset",
            "  --enable-live-affinity  Allow linux-command to apply taskset after nice-only validation",
            "  --live-pid-allowlist <pids>  Live actuator PID allowlist override, e.g. 1234,5678",
            "  --warmup-executor-command <program>  Explicit command for WarmupExecutor side effects",
            "  --warmup-executor-arg <arg>  Argument for --warmup-executor-command; repeat as needed",
            "  --warmup-executor-timeout-ms <n>  WarmupExecutor command timeout (default: 250)",
            "  --allow-partial-probes     Continue when some Linux probes cannot attach",
            "  --probe-buffer-events <n>  Linux reader buffered-event hint (default: 4096)",
            "  --probe-poll-timeout-ms <n>  Linux reader poll timeout hint (default: 100)",
            "  --batch-size <n>     Max events per poll batch (default: 32)",
            "  --max-events <n>     Stop after processing n events and print a summary",
            "  --tick-ms <n>        Periodic rollback tick interval in ms (default: 200)",
            "  --drain-ms <n>       Final drain window after source exhaustion in ms (default: 5000)",
            "  --verification-log <path>  Append daemon summary to a verification log",
        ]
        .join("\n")
    }

    fn probe_reader_config(&self) -> ProbeReaderConfig {
        ProbeReaderConfig {
            require_all_probes: self.require_all_probes,
            max_buffered_events: self.probe_buffer_events,
            poll_timeout_ms: self.probe_poll_timeout_ms,
        }
    }
}

fn parse_config_profile(raw: &str) -> Result<RuntimeConfigProfile, String> {
    RuntimeConfigProfile::named(raw).map_err(|error| error.to_string())
}

fn build_mock_source(profile: &str) -> Result<MockEventSource, String> {
    match profile {
        "demo" => Ok(MockEventSource::demo_sequence()),
        "tool-call-lifecycle" => Ok(MockEventSource::tool_call_lifecycle_sequence()),
        other => Err(format!(
            "unsupported mock profile `{other}`; expected `demo` or `tool-call-lifecycle`"
        )),
    }
}

fn runtime_config_for_source(
    cli: &CliConfig,
    config: &RuntimeOrchestratorConfig,
) -> runtime_orchestrator::RuntimeConfig {
    let mut runtime = config.runtime.clone();
    if cli.actuator_backend == "linux-command" && !cli.live_pid_allowlist.is_empty() {
        runtime.selection_mode = "pid_allowlist".to_string();
        runtime.process_names.clear();
        runtime.pid_allowlist = cli.live_pid_allowlist.clone();
    }
    runtime
}

fn config_for_actuator_scope(
    cli: &CliConfig,
    mut config: RuntimeOrchestratorConfig,
) -> RuntimeOrchestratorConfig {
    if cli.actuator_backend == "linux-command" && !cli.enable_live_affinity {
        for policy in config.scenarios.values_mut() {
            policy.actions.pin_strategy = None;
        }
    }
    config
}

fn parse_pid_allowlist(raw: &str) -> Result<BTreeSet<u32>, String> {
    let mut pids = BTreeSet::new();
    for value in raw
        .split(',')
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        let pid = value
            .parse::<u32>()
            .map_err(|_| format!("invalid PID `{value}` in --live-pid-allowlist"))?;
        if pid == 0 {
            return Err("--live-pid-allowlist cannot include pid 0".to_string());
        }
        pids.insert(pid);
    }
    if pids.is_empty() {
        return Err("--live-pid-allowlist expects at least one PID".to_string());
    }
    Ok(pids)
}

fn append_summary_to_log(
    path: &PathBuf,
    summary: &aegisai_runtime_daemon::RuntimeRunSummary,
) -> std::io::Result<()> {
    if let Some(parent) = path.parent() {
        if !parent.as_os_str().is_empty() {
            std::fs::create_dir_all(parent)?;
        }
    }

    let mut file = OpenOptions::new().create(true).append(true).open(path)?;
    writeln!(
        file,
        "\n### {} - Runtime daemon summary",
        unix_timestamp_label()
    )?;
    writeln!(file)?;
    writeln!(file, "- Source: `{}`", summary.source_name)?;
    writeln!(
        file,
        "- Metadata provider: `{}`",
        summary.metadata_provider_name
    )?;
    writeln!(
        file,
        "- Actuator backend: `{}`",
        summary.actuator_backend_name
    )?;
    writeln!(file, "- Processed events: `{}`", summary.processed_events)?;
    writeln!(file, "- Applied actions: `{}`", summary.applied_actions)?;
    writeln!(file, "- Inline rollbacks: `{}`", summary.inline_rollbacks)?;
    writeln!(file, "- Tick rollbacks: `{}`", summary.tick_rollbacks)?;
    writeln!(file, "- Metric records: `{}`", summary.metric_records)?;
    writeln!(file, "- Trace records: `{}`", summary.trace_records)?;
    if !summary.signal_observations.is_empty() {
        writeln!(file, "- Signal observations:")?;
        for (signal, observation) in &summary.signal_observations {
            writeln!(
                file,
                "  - `{}`: events={}, total={}, max={}",
                signal, observation.event_count, observation.value_total, observation.value_max
            )?;
        }
    }
    if !summary.feature_window_maxima.is_empty() {
        writeln!(file, "- Feature window maxima:")?;
        for (metric, value) in &summary.feature_window_maxima {
            writeln!(file, "  - `{metric}`: {value}")?;
        }
    }
    if !summary.audit_highlights.is_empty() {
        writeln!(file, "- Audit highlights:")?;
        for highlight in &summary.audit_highlights {
            writeln!(file, "  - `{highlight}`")?;
        }
    }
    if !summary.source_diagnostics.is_empty() {
        writeln!(file, "- Source diagnostics:")?;
        for diagnostic in &summary.source_diagnostics {
            writeln!(file, "  - `{diagnostic}`")?;
        }
    }
    if !summary.tool_call_lifecycles.is_empty() {
        writeln!(file, "- Tool call lifecycles:")?;
        for lifecycle in &summary.tool_call_lifecycles {
            writeln!(
                file,
                "  - `{}`: duration_ms={}, stages={}, boosted_actions={}, rollback_actions={}, background_events={}, isolation_events={}, pids={}",
                lifecycle.lifecycle_id,
                lifecycle.duration_ms(),
                format_stage_counts(&lifecycle.stages),
                lifecycle.boosted_actions,
                lifecycle.rollback_actions,
                lifecycle.background_events,
                lifecycle.isolation_events,
                format_pids(&lifecycle.target_pids)
            )?;
        }
    }
    if summary.triggered_scenarios.is_empty() {
        writeln!(file, "- Triggered scenarios: `none`")?;
    } else {
        let triggered = summary
            .triggered_scenarios
            .iter()
            .map(|(scenario, count)| format!("{scenario}:{count}"))
            .collect::<Vec<_>>()
            .join(", ");
        writeln!(file, "- Triggered scenarios: `{triggered}`")?;
    }
    Ok(())
}

fn unix_timestamp_label() -> String {
    let seconds = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs())
        .unwrap_or(0);
    format!("unix:{seconds}")
}

fn print_summary(summary: &aegisai_runtime_daemon::RuntimeRunSummary) {
    println!("AegisAI Runtime Daemon Summary");
    println!("source: {}", summary.source_name);
    println!("metadata: {}", summary.metadata_provider_name);
    println!("actuator_backend: {}", summary.actuator_backend_name);
    println!("processed_events: {}", summary.processed_events);
    println!("applied_actions: {}", summary.applied_actions);
    println!("inline_rollbacks: {}", summary.inline_rollbacks);
    println!("tick_rollbacks: {}", summary.tick_rollbacks);
    println!("metric_records: {}", summary.metric_records);
    println!("trace_records: {}", summary.trace_records);
    if !summary.signal_observations.is_empty() {
        println!("signal_observations:");
        for (signal, observation) in &summary.signal_observations {
            println!(
                "  {signal}: events={} total={} max={}",
                observation.event_count, observation.value_total, observation.value_max
            );
        }
    }
    if !summary.feature_window_maxima.is_empty() {
        println!("feature_window_maxima:");
        for (metric, value) in &summary.feature_window_maxima {
            println!("  {metric}: {value}");
        }
    }
    if !summary.audit_highlights.is_empty() {
        println!("audit_highlights:");
        for highlight in &summary.audit_highlights {
            println!("  {highlight}");
        }
    }
    if !summary.source_diagnostics.is_empty() {
        println!("source_diagnostics:");
        for diagnostic in &summary.source_diagnostics {
            println!("  {diagnostic}");
        }
    }
    if !summary.tool_call_lifecycles.is_empty() {
        println!("tool_call_lifecycles:");
        for lifecycle in &summary.tool_call_lifecycles {
            println!(
                "  {}: duration_ms={} stages={} boosted_actions={} rollback_actions={} background_events={} isolation_events={} pids={}",
                lifecycle.lifecycle_id,
                lifecycle.duration_ms(),
                format_stage_counts(&lifecycle.stages),
                lifecycle.boosted_actions,
                lifecycle.rollback_actions,
                lifecycle.background_events,
                lifecycle.isolation_events,
                format_pids(&lifecycle.target_pids)
            );
        }
    }

    if summary.triggered_scenarios.is_empty() {
        println!("triggered_scenarios: none");
    } else {
        println!("triggered_scenarios:");
        for (scenario, count) in &summary.triggered_scenarios {
            println!("  {scenario}: {count}");
        }
    }
}

fn format_stage_counts(stages: &BTreeMap<String, u64>) -> String {
    if stages.is_empty() {
        return "none".to_string();
    }

    stages
        .iter()
        .map(|(stage, count)| format!("{stage}:{count}"))
        .collect::<Vec<_>>()
        .join(",")
}

fn format_pids(pids: &BTreeSet<u32>) -> String {
    if pids.is_empty() {
        return "none".to_string();
    }

    pids.iter()
        .map(u32::to_string)
        .collect::<Vec<_>>()
        .join(",")
}

#[cfg(target_os = "linux")]
fn procfs_metadata_provider() -> ProcfsMetadataProvider {
    ProcfsMetadataProvider::default()
}

#[cfg(not(target_os = "linux"))]
fn procfs_metadata_provider() -> ProcfsMetadataProvider {
    ProcfsMetadataProvider
}

#[cfg(target_os = "linux")]
fn procfs_metadata_provider_for_mock() -> Result<ProcfsMetadataProvider, String> {
    Ok(ProcfsMetadataProvider::default())
}

#[cfg(not(target_os = "linux"))]
fn procfs_metadata_provider_for_mock() -> Result<ProcfsMetadataProvider, String> {
    Err("procfs metadata provider is only available on Linux; use `demo` or `noop` metadata on Windows".to_string())
}

fn build_actuator(cli: &CliConfig, config: &RuntimeOrchestratorConfig) -> Result<Actuator, String> {
    if cli.warmup_executor_program.is_some()
        && !matches!(
            cli.actuator_backend.as_str(),
            "linux-command" | "linux-command-dry-run"
        )
    {
        return Err(
            "--warmup-executor-command requires `linux-command` or `linux-command-dry-run`"
                .to_string(),
        );
    }

    match cli.actuator_backend.as_str() {
        "noop" => Ok(Actuator::with_backend(NoopActuatorBackend)),
        "linux-skeleton" => Ok(Actuator::with_backend(LinuxActuatorBackend::default())),
        "linux-command" => build_linux_command_actuator(cli, config),
        "linux-command-dry-run" => build_linux_command_dry_run_actuator(cli),
        other => Err(format!(
            "unsupported actuator backend `{other}`; expected `noop`, `linux-skeleton`, `linux-command`, or `linux-command-dry-run`"
        )),
    }
}

#[cfg(target_os = "linux")]
fn build_linux_command_actuator(
    cli: &CliConfig,
    config: &RuntimeOrchestratorConfig,
) -> Result<Actuator, String> {
    if !cli.confirm_live_actuator {
        return Err(
            "`linux-command` requires --confirm-live-actuator before host commands may run"
                .to_string(),
        );
    }
    let allowed_pids = if cli.live_pid_allowlist.is_empty() {
        config.runtime.pid_allowlist.clone()
    } else {
        cli.live_pid_allowlist.clone()
    };
    if allowed_pids.is_empty() {
        return Err(
            "`linux-command` requires a non-empty PID allowlist from --live-pid-allowlist or [selection].pid_allowlist in runtime config".to_string(),
        );
    }

    let guard = if cli.enable_live_affinity {
        LiveLinuxCommandGuard::nice_and_affinity(allowed_pids.iter().copied(), true)
    } else {
        LiveLinuxCommandGuard::nice_only(allowed_pids.iter().copied(), true)
    };
    let guard = if can_raise_nice_priority() {
        guard
    } else {
        guard.without_priority_raise()
    };
    let mut applier = aegisai_actuator::CommandLinuxSyscallApplier::live(guard.clone());
    if let Some(command) = warmup_executor_command(cli) {
        applier = applier.with_system_warmup_executor(command, cli.warmup_executor_timeout_ms);
    }
    let executor =
        aegisai_actuator::PlannedOnlyLinuxSyscallExecutor::with_state_provider_and_applier(
            aegisai_actuator::ProcfsLinuxProcessStateProvider,
            applier,
        )
        .with_live_guard(guard);
    Ok(Actuator::with_backend(
        LinuxActuatorBackend::with_named_executor("linux-command", executor),
    ))
}

#[cfg(not(target_os = "linux"))]
fn build_linux_command_actuator(
    _cli: &CliConfig,
    _config: &RuntimeOrchestratorConfig,
) -> Result<Actuator, String> {
    Err("`linux-command` actuator backend is only available on Linux".to_string())
}

#[cfg(target_os = "linux")]
fn build_linux_command_dry_run_actuator(cli: &CliConfig) -> Result<Actuator, String> {
    let mut applier = aegisai_actuator::CommandLinuxSyscallApplier::dry_run();
    if let Some(command) = warmup_executor_command(cli) {
        applier = applier.with_dry_run_warmup_executor(command, cli.warmup_executor_timeout_ms);
    }
    let executor =
        aegisai_actuator::PlannedOnlyLinuxSyscallExecutor::with_state_provider_and_applier(
            aegisai_actuator::ProcfsLinuxProcessStateProvider,
            applier,
        );
    Ok(Actuator::with_backend(
        LinuxActuatorBackend::with_named_executor("linux-command-dry-run", executor),
    ))
}

#[cfg(not(target_os = "linux"))]
fn build_linux_command_dry_run_actuator(_cli: &CliConfig) -> Result<Actuator, String> {
    Err("`linux-command-dry-run` actuator backend is only available on Linux".to_string())
}

fn warmup_executor_command(cli: &CliConfig) -> Option<aegisai_actuator::WarmupExecutorCommand> {
    cli.warmup_executor_program.as_ref().map(|program| {
        aegisai_actuator::WarmupExecutorCommand::new(
            program.clone(),
            cli.warmup_executor_args.clone(),
        )
    })
}

#[cfg(target_os = "linux")]
fn can_raise_nice_priority() -> bool {
    let Ok(status) = std::fs::read_to_string("/proc/self/status") else {
        return false;
    };
    let Some(cap_eff) = status
        .lines()
        .find_map(|line| line.strip_prefix("CapEff:").map(str::trim))
    else {
        return false;
    };
    u64::from_str_radix(cap_eff, 16)
        .map(|capabilities| capabilities & (1 << 23) != 0)
        .unwrap_or(false)
}

#[cfg(test)]
mod tests {
    use std::collections::{BTreeMap, BTreeSet};
    use std::fs;
    use std::path::{Path, PathBuf};

    use aegisai_runtime_daemon::{
        EventSource, RuntimeRunSummary, SignalObservationSummary, ToolCallLifecycleSummary,
    };
    use runtime_orchestrator::RuntimeOrchestratorConfig;

    use super::{
        append_summary_to_log, build_actuator, build_linux_command_dry_run_actuator,
        build_mock_source, config_for_actuator_scope, runtime_config_for_source, CliConfig,
    };

    fn repo_root() -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .ancestors()
            .nth(2)
            .expect("crate lives under agent/runtime_daemon")
            .to_path_buf()
    }

    fn sample_config() -> RuntimeOrchestratorConfig {
        RuntimeOrchestratorConfig::load_from_repo_root(repo_root()).expect("config should load")
    }

    #[test]
    fn cli_supports_probe_reader_flags() {
        let cli = CliConfig::parse(
            [
                "--source",
                "linux",
                "--metadata",
                "procfs",
                "--allow-partial-probes",
                "--probe-buffer-events",
                "8192",
                "--probe-poll-timeout-ms",
                "250",
            ]
            .into_iter()
            .map(str::to_string),
        )
        .expect("cli should parse");

        let probe_config = cli.probe_reader_config();
        assert!(!probe_config.require_all_probes);
        assert_eq!(probe_config.max_buffered_events, 8_192);
        assert_eq!(probe_config.poll_timeout_ms, 250);
    }

    #[test]
    fn cli_supports_max_events_limit() {
        let cli = CliConfig::parse(["--max-events", "512"].into_iter().map(str::to_string))
            .expect("cli should parse");

        assert_eq!(cli.max_events, Some(512));
    }

    #[test]
    fn cli_defaults_to_local_demo_config_profile() {
        let cli = CliConfig::parse(std::iter::empty::<String>()).expect("cli should parse");

        assert_eq!(
            cli.config_profile,
            runtime_orchestrator::RuntimeConfigProfile::LocalDemo
        );
    }

    #[test]
    fn cli_reads_config_profile_from_env() {
        let cli =
            CliConfig::parse_with_env(std::iter::empty::<String>(), Some("prod_1".to_string()))
                .expect("cli should parse");

        assert_eq!(
            cli.config_profile,
            runtime_orchestrator::RuntimeConfigProfile::Named("prod_1".to_string())
        );
    }

    #[test]
    fn cli_config_profile_overrides_env_profile() {
        let cli = CliConfig::parse_with_env(
            ["--config-profile", "cli_prod"]
                .into_iter()
                .map(str::to_string),
            Some("env_prod".to_string()),
        )
        .expect("cli should parse");

        assert_eq!(
            cli.config_profile,
            runtime_orchestrator::RuntimeConfigProfile::Named("cli_prod".to_string())
        );
    }

    #[test]
    fn cli_rejects_invalid_config_profile_names() {
        let error = CliConfig::parse(
            ["--config-profile", "../prod"]
                .into_iter()
                .map(str::to_string),
        )
        .expect_err("path-like profile should fail");

        assert!(error.contains("profile"));
        assert!(error.contains("path separators") || error.contains("dot segments"));
    }

    #[test]
    fn cli_rejects_zero_max_events() {
        let error = CliConfig::parse(["--max-events", "0"].into_iter().map(str::to_string))
            .expect_err("zero max events should fail");

        assert!(error.contains("positive integer"));
    }

    #[test]
    fn live_command_source_selection_uses_cli_pid_allowlist() {
        let config = sample_config();
        let cli = CliConfig::parse(
            [
                "--actuator-backend",
                "linux-command",
                "--confirm-live-actuator",
                "--live-pid-allowlist",
                "42,77",
            ]
            .into_iter()
            .map(str::to_string),
        )
        .expect("cli should parse");

        let runtime = runtime_config_for_source(&cli, &config);

        assert_eq!(runtime.selection_mode, "pid_allowlist");
        assert!(runtime.process_names.is_empty());
        assert_eq!(runtime.pid_allowlist, [42, 77].into_iter().collect());
    }

    #[test]
    fn live_command_defaults_to_nice_only_action_plan() {
        let config = sample_config();
        let cli = CliConfig::parse(
            [
                "--actuator-backend",
                "linux-command",
                "--confirm-live-actuator",
                "--live-pid-allowlist",
                "42",
            ]
            .into_iter()
            .map(str::to_string),
        )
        .expect("cli should parse");

        let scoped = config_for_actuator_scope(&cli, config);
        let inference_policy = scoped
            .scenarios
            .get(&runtime_orchestrator::ScenarioKind::InferenceTailGuard)
            .expect("inference policy");
        let tool_call_policy = scoped
            .scenarios
            .get(&runtime_orchestrator::ScenarioKind::ToolCallBooster)
            .expect("tool call policy");

        assert!(inference_policy.actions.raise_nice.is_some());
        assert!(inference_policy.actions.pin_strategy.is_none());
        assert_eq!(inference_policy.actions.use_cpuset, Some(false));
        assert!(tool_call_policy.actions.raise_nice.is_some());
        assert!(tool_call_policy.actions.pin_strategy.is_none());
    }

    #[test]
    fn live_command_can_plan_affinity_after_explicit_flag() {
        let config = sample_config();
        let cli = CliConfig::parse(
            [
                "--actuator-backend",
                "linux-command",
                "--confirm-live-actuator",
                "--enable-live-affinity",
                "--live-pid-allowlist",
                "42",
            ]
            .into_iter()
            .map(str::to_string),
        )
        .expect("cli should parse");

        let scoped = config_for_actuator_scope(&cli, config);
        let inference_policy = scoped
            .scenarios
            .get(&runtime_orchestrator::ScenarioKind::InferenceTailGuard)
            .expect("inference policy");
        let tool_call_policy = scoped
            .scenarios
            .get(&runtime_orchestrator::ScenarioKind::ToolCallBooster)
            .expect("tool call policy");

        assert!(inference_policy.actions.pin_strategy.is_some());
        assert!(tool_call_policy.actions.pin_strategy.is_some());
    }

    #[test]
    fn cli_accepts_linux_command_backend_names() {
        let cli = CliConfig::parse(
            ["--actuator-backend", "linux-command"]
                .into_iter()
                .map(str::to_string),
        )
        .expect("cli should parse");

        assert_eq!(cli.actuator_backend, "linux-command");

        let cli = CliConfig::parse(
            ["--actuator-backend", "linux-command-dry-run"]
                .into_iter()
                .map(str::to_string),
        )
        .expect("cli should parse");

        assert_eq!(cli.actuator_backend, "linux-command-dry-run");
    }

    #[test]
    fn cli_accepts_tool_call_lifecycle_mock_profile() {
        let cli = CliConfig::parse(
            ["--mock-profile", "tool-call-lifecycle"]
                .into_iter()
                .map(str::to_string),
        )
        .expect("cli should parse");

        assert_eq!(cli.mock_profile, "tool-call-lifecycle");
        assert_eq!(
            build_mock_source(&cli.mock_profile)
                .expect("profile should exist")
                .source_name(),
            "mock-tool-call-lifecycle"
        );
    }

    #[test]
    fn cli_accepts_live_actuator_confirmation_flags() {
        let cli = CliConfig::parse(
            [
                "--actuator-backend",
                "linux-command",
                "--confirm-live-actuator",
                "--enable-live-affinity",
                "--live-pid-allowlist",
                "42, 77",
            ]
            .into_iter()
            .map(str::to_string),
        )
        .expect("cli should parse");

        assert_eq!(cli.actuator_backend, "linux-command");
        assert!(cli.confirm_live_actuator);
        assert!(cli.enable_live_affinity);
        assert_eq!(cli.live_pid_allowlist, [42, 77].into_iter().collect());
    }

    #[test]
    fn cli_replaces_duplicate_live_pid_allowlist_with_last_value() {
        let cli = CliConfig::parse(
            [
                "--live-pid-allowlist",
                "42,77",
                "--live-pid-allowlist",
                "77,99,99",
            ]
            .into_iter()
            .map(str::to_string),
        )
        .expect("duplicate pid allowlist flag should parse deterministically");

        assert_eq!(cli.live_pid_allowlist, [77, 99].into_iter().collect());
    }

    #[test]
    fn cli_normalizes_whitespace_and_empty_pid_allowlist_elements() {
        let cli = CliConfig::parse(
            ["--live-pid-allowlist", " 42, , 77,  "]
                .into_iter()
                .map(str::to_string),
        )
        .expect("empty pid elements should be ignored when at least one pid remains");

        assert_eq!(cli.live_pid_allowlist, [42, 77].into_iter().collect());
    }

    #[test]
    fn cli_rejects_empty_live_pid_allowlist_after_normalization() {
        let error = CliConfig::parse(
            ["--live-pid-allowlist", " , , "]
                .into_iter()
                .map(str::to_string),
        )
        .expect_err("empty pid allowlist should fail");

        assert_eq!(error, "--live-pid-allowlist expects at least one PID");
    }

    #[test]
    fn cli_rejects_unknown_source_name() {
        let error = CliConfig::parse(["--source", "tracefs"].into_iter().map(str::to_string))
            .expect_err("unknown source should fail");

        assert_eq!(
            error,
            "unsupported source mode `tracefs`; expected `mock` or `linux`"
        );
    }

    #[test]
    fn cli_rejects_unknown_actuator_backend_name() {
        let error = CliConfig::parse(
            ["--actuator-backend", "linux-live"]
                .into_iter()
                .map(str::to_string),
        )
        .expect_err("unknown backend should fail");

        assert_eq!(
            error,
            "unsupported actuator backend `linux-live`; expected `noop`, `linux-skeleton`, `linux-command`, or `linux-command-dry-run`"
        );
    }

    #[test]
    fn cli_accepts_explicit_warmup_executor_command_boundary() {
        let cli = CliConfig::parse(
            [
                "--actuator-backend",
                "linux-command-dry-run",
                "--warmup-executor-command",
                "python3",
                "--warmup-executor-arg",
                "bench/tool_call_booster/real_tool_executor.py",
                "--warmup-executor-arg",
                "retrieval-worker",
                "--warmup-executor-timeout-ms",
                "125",
            ]
            .into_iter()
            .map(str::to_string),
        )
        .expect("cli should parse");

        assert_eq!(cli.warmup_executor_program, Some("python3".to_string()));
        assert_eq!(
            cli.warmup_executor_args,
            [
                "bench/tool_call_booster/real_tool_executor.py".to_string(),
                "retrieval-worker".to_string()
            ]
        );
        assert_eq!(cli.warmup_executor_timeout_ms, 125);
    }

    #[test]
    fn cli_rejects_warmup_executor_arg_without_command() {
        let error = CliConfig::parse(
            ["--warmup-executor-arg", "retrieval-worker"]
                .into_iter()
                .map(str::to_string),
        )
        .expect_err("warmup arg without command should fail");

        assert!(error.contains("--warmup-executor-command"));
    }

    #[test]
    fn cli_rejects_empty_warmup_executor_command() {
        let error = CliConfig::parse(
            ["--warmup-executor-command", "  "]
                .into_iter()
                .map(str::to_string),
        )
        .expect_err("empty warmup command should fail");

        assert_eq!(
            error,
            "--warmup-executor-command expects a non-empty program path"
        );
    }

    #[test]
    fn cli_accepts_warmup_executor_arg_that_looks_like_flag_after_command() {
        let cli = CliConfig::parse(
            [
                "--warmup-executor-command",
                "prime-cache",
                "--warmup-executor-arg",
                "--worker",
            ]
            .into_iter()
            .map(str::to_string),
        )
        .expect("warmup arg values may look like flags");

        assert_eq!(cli.warmup_executor_program, Some("prime-cache".to_string()));
        assert_eq!(cli.warmup_executor_args, ["--worker".to_string()]);
    }

    #[test]
    fn cli_rejects_zero_warmup_executor_timeout() {
        let error = CliConfig::parse(
            [
                "--warmup-executor-command",
                "prime-cache",
                "--warmup-executor-timeout-ms",
                "0",
            ]
            .into_iter()
            .map(str::to_string),
        )
        .expect_err("zero warmup timeout should fail");

        assert!(error.contains("positive integer"));
    }

    #[test]
    fn cli_rejects_invalid_live_pid_allowlist() {
        let error = CliConfig::parse(
            ["--live-pid-allowlist", "0,abc"]
                .into_iter()
                .map(str::to_string),
        )
        .expect_err("invalid pid allowlist should fail");

        assert!(error.contains("pid 0") || error.contains("invalid PID"));
    }

    #[test]
    fn cli_accepts_verification_log_path() {
        let cli = CliConfig::parse(
            ["--verification-log", "docs/verification_log.md"]
                .into_iter()
                .map(str::to_string),
        )
        .expect("cli should parse");

        assert_eq!(
            cli.verification_log.as_deref(),
            Some(std::path::Path::new("docs/verification_log.md"))
        );
    }

    #[test]
    fn cli_rejects_verification_log_without_path() {
        let error = CliConfig::parse(["--verification-log"].into_iter().map(str::to_string))
            .expect_err("verification log should require a path");

        assert_eq!(error, "--verification-log expects a path");
    }

    #[test]
    fn linux_command_dry_run_backend_uses_named_backend() {
        let cli = CliConfig::default();
        let actuator = build_linux_command_dry_run_actuator(&cli).expect("backend should build");
        assert_eq!(actuator.backend_name(), "linux-command-dry-run");
    }

    #[test]
    fn warmup_executor_command_requires_command_backend() {
        let config = sample_config();
        let cli = CliConfig::parse(
            ["--warmup-executor-command", "prime-cache"]
                .into_iter()
                .map(str::to_string),
        )
        .expect("cli should parse");

        let error = match build_actuator(&cli, &config) {
            Ok(_) => panic!("warmup side effect should require command backend"),
            Err(error) => error,
        };
        assert!(error.contains("linux-command"));
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn linux_command_requires_explicit_confirmation() {
        let config = sample_config();
        let cli = CliConfig::parse(
            ["--actuator-backend", "linux-command"]
                .into_iter()
                .map(str::to_string),
        )
        .expect("cli should parse");

        let error = match build_actuator(&cli, &config) {
            Ok(_) => panic!("live command should be gated"),
            Err(error) => error,
        };
        assert!(error.contains("--confirm-live-actuator"));
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn linux_command_requires_non_empty_pid_allowlist() {
        let config = sample_config();
        let cli = CliConfig::parse(
            [
                "--actuator-backend",
                "linux-command",
                "--confirm-live-actuator",
            ]
            .into_iter()
            .map(str::to_string),
        )
        .expect("cli should parse");

        let error = match build_actuator(&cli, &config) {
            Ok(_) => panic!("allowlist should be required"),
            Err(error) => error,
        };
        assert!(error.contains("pid_allowlist"));
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn linux_command_with_confirmation_and_config_allowlist_builds_live_backend() {
        let mut config = sample_config();
        config.runtime.pid_allowlist = [42].into_iter().collect();
        let cli = CliConfig::parse(
            [
                "--actuator-backend",
                "linux-command",
                "--confirm-live-actuator",
                "--enable-live-affinity",
            ]
            .into_iter()
            .map(str::to_string),
        )
        .expect("cli should parse");

        let actuator = build_actuator(&cli, &config).expect("live backend should build");
        assert_eq!(actuator.backend_name(), "linux-command");
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn linux_command_with_confirmation_and_cli_allowlist_builds_live_backend() {
        let config = sample_config();
        let cli = CliConfig::parse(
            [
                "--actuator-backend",
                "linux-command",
                "--confirm-live-actuator",
                "--live-pid-allowlist",
                "42",
            ]
            .into_iter()
            .map(str::to_string),
        )
        .expect("cli should parse");

        let actuator = build_actuator(&cli, &config).expect("live backend should build");
        assert_eq!(actuator.backend_name(), "linux-command");
    }

    #[test]
    fn verification_log_includes_audit_highlights() {
        let log_path = std::env::temp_dir().join(format!(
            "aegisai-runtime-daemon-{}-verification.md",
            std::process::id()
        ));
        let _ = fs::remove_file(&log_path);

        let summary = RuntimeRunSummary {
            source_name: "mock".to_string(),
            metadata_provider_name: "demo".to_string(),
            actuator_backend_name: "linux-command-dry-run".to_string(),
            processed_events: 3,
            applied_actions: 2,
            inline_rollbacks: 1,
            tick_rollbacks: 0,
            metric_records: 3,
            trace_records: 4,
            audit_highlights: vec![
                "pid=42;scenario=inference_tail_guard;backend.apply.apply.result=ok".to_string(),
            ],
            ..RuntimeRunSummary::default()
        };

        append_summary_to_log(&log_path, &summary).expect("summary should append");
        let contents = fs::read_to_string(&log_path).expect("log should be readable");
        let _ = fs::remove_file(&log_path);

        assert!(contents.contains("- Audit highlights:"));
        assert!(
            contents.contains("pid=42;scenario=inference_tail_guard;backend.apply.apply.result=ok")
        );
    }

    #[test]
    fn verification_log_includes_source_diagnostics() {
        let log_path = std::env::temp_dir().join(format!(
            "aegisai-runtime-daemon-source-diagnostics-{}-verification.md",
            std::process::id()
        ));
        let _ = fs::remove_file(&log_path);

        let summary = RuntimeRunSummary {
            source_name: "linux-probe".to_string(),
            metadata_provider_name: "procfs".to_string(),
            actuator_backend_name: "linux-skeleton".to_string(),
            source_diagnostics: vec![
                "helper compatibility: status=compatible; kernel=6.8.0-test".to_string()
            ],
            ..RuntimeRunSummary::default()
        };

        append_summary_to_log(&log_path, &summary).expect("summary should append");
        let contents = fs::read_to_string(&log_path).expect("log should be readable");
        let _ = fs::remove_file(&log_path);

        assert!(contents.contains("- Source diagnostics:"));
        assert!(contents.contains("helper compatibility: status=compatible"));
        assert!(contents.contains("kernel=6.8.0-test"));
    }

    #[test]
    fn verification_log_includes_tool_call_lifecycle_summary() {
        let log_path = std::env::temp_dir().join(format!(
            "aegisai-runtime-daemon-lifecycle-{}-verification.md",
            std::process::id()
        ));
        let _ = fs::remove_file(&log_path);

        let summary = RuntimeRunSummary {
            source_name: "mock-tool-call-lifecycle".to_string(),
            metadata_provider_name: "noop".to_string(),
            actuator_backend_name: "noop".to_string(),
            tool_call_lifecycles: vec![ToolCallLifecycleSummary {
                lifecycle_id: "tc-001".to_string(),
                started_at_ms: 10_000,
                ended_at_ms: 10_800,
                stages: BTreeMap::from([
                    ("executor".to_string(), 1),
                    ("retrieval".to_string(), 2),
                    ("rerank".to_string(), 1),
                ]),
                boosted_actions: 7,
                rollback_actions: 7,
                background_events: 1,
                isolation_events: 3,
                target_pids: BTreeSet::from([6_100, 6_101, 6_102]),
            }],
            ..RuntimeRunSummary::default()
        };

        append_summary_to_log(&log_path, &summary).expect("summary should append");
        let contents = fs::read_to_string(&log_path).expect("log should be readable");
        let _ = fs::remove_file(&log_path);

        assert!(contents.contains("- Tool call lifecycles:"));
        assert!(contents.contains("tc-001"));
        assert!(contents.contains("stages=executor:1,rerank:1,retrieval:2"));
        assert!(contents.contains("rollback_actions=7"));
        assert!(contents.contains("isolation_events=3"));
    }

    #[test]
    fn verification_log_includes_observation_signal_summaries() {
        let log_path = std::env::temp_dir().join(format!(
            "aegisai-runtime-daemon-observation-{}-verification.md",
            std::process::id()
        ));
        let _ = fs::remove_file(&log_path);

        let summary = RuntimeRunSummary {
            source_name: "linux-probe".to_string(),
            metadata_provider_name: "procfs".to_string(),
            actuator_backend_name: "noop".to_string(),
            signal_observations: BTreeMap::from([
                (
                    "cpu_migration".to_string(),
                    SignalObservationSummary {
                        event_count: 2,
                        value_total: 5,
                        value_max: 3,
                    },
                ),
                (
                    "major_page_fault".to_string(),
                    SignalObservationSummary {
                        event_count: 1,
                        value_total: 2,
                        value_max: 2,
                    },
                ),
            ]),
            feature_window_maxima: BTreeMap::from([
                ("cpu_migrations_per_sec".to_string(), 10),
                ("major_page_faults_per_sec".to_string(), 4),
            ]),
            ..RuntimeRunSummary::default()
        };

        append_summary_to_log(&log_path, &summary).expect("summary should append");
        let contents = fs::read_to_string(&log_path).expect("log should be readable");
        let _ = fs::remove_file(&log_path);

        assert!(contents.contains("- Signal observations:"));
        assert!(contents.contains("cpu_migration"));
        assert!(contents.contains("events=2, total=5, max=3"));
        assert!(contents.contains("- Feature window maxima:"));
        assert!(contents.contains("cpu_migrations_per_sec"));
        assert!(contents.contains("major_page_faults_per_sec"));
    }
}
