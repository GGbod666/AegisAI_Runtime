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
use runtime_orchestrator::{RuntimeOrchestrator, RuntimeOrchestratorConfig};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let cli = CliConfig::parse(env::args().skip(1))?;

    let config = RuntimeOrchestratorConfig::load_from_repo_root(&cli.repo_root)?;
    let runtime_config = config.runtime.clone();
    let mut orchestrator =
        RuntimeOrchestrator::with_actuator(config.clone(), build_actuator(&cli, &config)?)?;
    let runtime_loop = RuntimeLoop::new(RuntimeLoopConfig {
        batch_size: cli.batch_size,
        tick_interval_ms: cli.tick_interval_ms,
        drain_after_source_ms: cli.drain_after_source_ms,
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
    source: String,
    mock_profile: String,
    metadata: String,
    actuator_backend: String,
    confirm_live_actuator: bool,
    enable_live_affinity: bool,
    live_pid_allowlist: BTreeSet<u32>,
    require_all_probes: bool,
    probe_buffer_events: usize,
    probe_poll_timeout_ms: u64,
    batch_size: usize,
    tick_interval_ms: u64,
    drain_after_source_ms: u64,
    verification_log: Option<PathBuf>,
}

impl Default for CliConfig {
    fn default() -> Self {
        Self {
            repo_root: env::current_dir().unwrap_or_else(|_| PathBuf::from(".")),
            source: "mock".to_string(),
            mock_profile: "demo".to_string(),
            metadata: "demo".to_string(),
            actuator_backend: "noop".to_string(),
            confirm_live_actuator: false,
            enable_live_affinity: false,
            live_pid_allowlist: BTreeSet::new(),
            require_all_probes: true,
            probe_buffer_events: 4_096,
            probe_poll_timeout_ms: 100,
            batch_size: 32,
            tick_interval_ms: 200,
            drain_after_source_ms: 5_000,
            verification_log: None,
        }
    }
}

impl CliConfig {
    fn parse<I>(args: I) -> Result<Self, String>
    where
        I: IntoIterator<Item = String>,
    {
        let mut config = Self::default();
        let mut args = args.into_iter();

        while let Some(arg) = args.next() {
            match arg.as_str() {
                "--repo-root" => {
                    let value = args
                        .next()
                        .ok_or_else(|| "--repo-root expects a path".to_string())?;
                    config.repo_root = PathBuf::from(value);
                }
                "--source" => {
                    config.source = args
                        .next()
                        .ok_or_else(|| "--source expects `mock` or `linux`".to_string())?;
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
                    config.actuator_backend = args.next().ok_or_else(|| {
                        "--actuator-backend expects `noop`, `linux-skeleton`, `linux-command`, or `linux-command-dry-run`"
                            .to_string()
                    })?;
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

        Ok(config)
    }

    fn usage() -> String {
        [
            "Usage: aegisai-runtime-daemon [options]",
            "",
            "Options:",
            "  --repo-root <path>   Repository root containing configs/ (default: current dir)",
            "  --source <mode>      Source mode: mock | linux (default: mock)",
            "  --mock-profile <name>  Mock source profile: demo | tool-call-lifecycle (default: demo)",
            "  --metadata <mode>    Metadata mode: demo | noop | procfs (default: demo)",
            "  --actuator-backend <mode>  Backend mode: noop | linux-skeleton | linux-command | linux-command-dry-run (default: noop)",
            "  --confirm-live-actuator  Required before linux-command may execute host renice/taskset",
            "  --enable-live-affinity  Allow linux-command to apply taskset after nice-only validation",
            "  --live-pid-allowlist <pids>  Live actuator PID allowlist override, e.g. 1234,5678",
            "  --allow-partial-probes     Continue when some Linux probes cannot attach",
            "  --probe-buffer-events <n>  Linux reader buffered-event hint (default: 4096)",
            "  --probe-poll-timeout-ms <n>  Linux reader poll timeout hint (default: 100)",
            "  --batch-size <n>     Max events per poll batch (default: 32)",
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

fn build_mock_source(profile: &str) -> Result<MockEventSource, String> {
    match profile {
        "demo" => Ok(MockEventSource::demo_sequence()),
        "tool-call-lifecycle" => Ok(MockEventSource::tool_call_lifecycle_sequence()),
        other => Err(format!(
            "unsupported mock profile `{other}`; expected `demo` or `tool-call-lifecycle`"
        )),
    }
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
    if !summary.audit_highlights.is_empty() {
        writeln!(file, "- Audit highlights:")?;
        for highlight in &summary.audit_highlights {
            writeln!(file, "  - `{highlight}`")?;
        }
    }
    if !summary.tool_call_lifecycles.is_empty() {
        writeln!(file, "- Tool call lifecycles:")?;
        for lifecycle in &summary.tool_call_lifecycles {
            writeln!(
                file,
                "  - `{}`: duration_ms={}, stages={}, boosted_actions={}, background_events={}, isolation_events={}, pids={}",
                lifecycle.lifecycle_id,
                lifecycle.duration_ms(),
                format_stage_counts(&lifecycle.stages),
                lifecycle.boosted_actions,
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
    if !summary.audit_highlights.is_empty() {
        println!("audit_highlights:");
        for highlight in &summary.audit_highlights {
            println!("  {highlight}");
        }
    }
    if !summary.tool_call_lifecycles.is_empty() {
        println!("tool_call_lifecycles:");
        for lifecycle in &summary.tool_call_lifecycles {
            println!(
                "  {}: duration_ms={} stages={} boosted_actions={} background_events={} isolation_events={} pids={}",
                lifecycle.lifecycle_id,
                lifecycle.duration_ms(),
                format_stage_counts(&lifecycle.stages),
                lifecycle.boosted_actions,
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
    match cli.actuator_backend.as_str() {
        "noop" => Ok(Actuator::with_backend(NoopActuatorBackend)),
        "linux-skeleton" => Ok(Actuator::with_backend(LinuxActuatorBackend::default())),
        "linux-command" => build_linux_command_actuator(cli, config),
        "linux-command-dry-run" => build_linux_command_dry_run_actuator(),
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
    let executor =
        aegisai_actuator::PlannedOnlyLinuxSyscallExecutor::with_state_provider_and_applier(
            aegisai_actuator::ProcfsLinuxProcessStateProvider,
            aegisai_actuator::CommandLinuxSyscallApplier::live(guard.clone()),
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
fn build_linux_command_dry_run_actuator() -> Result<Actuator, String> {
    let executor =
        aegisai_actuator::PlannedOnlyLinuxSyscallExecutor::with_state_provider_and_applier(
            aegisai_actuator::ProcfsLinuxProcessStateProvider,
            aegisai_actuator::CommandLinuxSyscallApplier::dry_run(),
        );
    Ok(Actuator::with_backend(
        LinuxActuatorBackend::with_named_executor("linux-command-dry-run", executor),
    ))
}

#[cfg(not(target_os = "linux"))]
fn build_linux_command_dry_run_actuator() -> Result<Actuator, String> {
    Err("`linux-command-dry-run` actuator backend is only available on Linux".to_string())
}

#[cfg(test)]
mod tests {
    use std::collections::{BTreeMap, BTreeSet};
    use std::fs;
    use std::path::{Path, PathBuf};

    use aegisai_runtime_daemon::{EventSource, RuntimeRunSummary, ToolCallLifecycleSummary};
    use runtime_orchestrator::RuntimeOrchestratorConfig;

    use super::{
        append_summary_to_log, build_actuator, build_linux_command_dry_run_actuator,
        build_mock_source, CliConfig,
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
    fn linux_command_dry_run_backend_uses_named_backend() {
        let actuator = build_linux_command_dry_run_actuator().expect("backend should build");
        assert_eq!(actuator.backend_name(), "linux-command-dry-run");
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
        assert!(contents.contains("isolation_events=3"));
    }
}
