use std::env;
use std::fs::OpenOptions;
use std::io::Write;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

use aegisai_actuator::{Actuator, LinuxActuatorBackend, NoopActuatorBackend};
use aegisai_runtime_daemon::{
    LinuxProbeSource, MockEventSource, NoopMetadataProvider, ProbeReaderConfig,
    ProcfsMetadataProvider, RuntimeLoop, RuntimeLoopConfig, StaticMetadataProvider,
};
use runtime_orchestrator::{RuntimeOrchestrator, RuntimeOrchestratorConfig};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let cli = CliConfig::parse(env::args().skip(1))?;

    let config = RuntimeOrchestratorConfig::load_from_repo_root(&cli.repo_root)?;
    let runtime_config = config.runtime.clone();
    let mut orchestrator = RuntimeOrchestrator::with_actuator(config, build_actuator(&cli)?)?;
    let runtime_loop = RuntimeLoop::new(RuntimeLoopConfig {
        batch_size: cli.batch_size,
        tick_interval_ms: cli.tick_interval_ms,
        drain_after_source_ms: cli.drain_after_source_ms,
    })?;

    let summary = match (cli.source.as_str(), cli.metadata.as_str()) {
        ("mock", "demo") => {
            let mut source = MockEventSource::demo_sequence();
            let mut metadata = StaticMetadataProvider::demo();
            runtime_loop.run(&mut orchestrator, &mut source, &mut metadata)?
        }
        ("mock", "noop") => {
            let mut source = MockEventSource::demo_sequence();
            let mut metadata = NoopMetadataProvider;
            runtime_loop.run(&mut orchestrator, &mut source, &mut metadata)?
        }
        ("mock", "procfs") => {
            let mut source = MockEventSource::demo_sequence();
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
    metadata: String,
    actuator_backend: String,
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
            metadata: "demo".to_string(),
            actuator_backend: "noop".to_string(),
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
                "--metadata" => {
                    config.metadata = args.next().ok_or_else(|| {
                        "--metadata expects `demo`, `noop`, or `procfs`".to_string()
                    })?;
                }
                "--actuator-backend" => {
                    config.actuator_backend = args.next().ok_or_else(|| {
                        "--actuator-backend expects `noop`, `linux-skeleton`, or `linux-command`"
                            .to_string()
                    })?;
                }
                "--allow-partial-probes" => {
                    config.require_all_probes = false;
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
            "  --metadata <mode>    Metadata mode: demo | noop | procfs (default: demo)",
            "  --actuator-backend <mode>  Backend mode: noop | linux-skeleton | linux-command (default: noop)",
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

    if summary.triggered_scenarios.is_empty() {
        println!("triggered_scenarios: none");
    } else {
        println!("triggered_scenarios:");
        for (scenario, count) in &summary.triggered_scenarios {
            println!("  {scenario}: {count}");
        }
    }
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

fn build_actuator(cli: &CliConfig) -> Result<Actuator, String> {
    match cli.actuator_backend.as_str() {
        "noop" => Ok(Actuator::with_backend(NoopActuatorBackend)),
        "linux-skeleton" => Ok(Actuator::with_backend(LinuxActuatorBackend::default())),
        "linux-command" => build_linux_command_actuator(),
        other => Err(format!(
            "unsupported actuator backend `{other}`; expected `noop`, `linux-skeleton`, or `linux-command`"
        )),
    }
}

#[cfg(target_os = "linux")]
fn build_linux_command_actuator() -> Result<Actuator, String> {
    let executor =
        aegisai_actuator::PlannedOnlyLinuxSyscallExecutor::with_state_provider_and_applier(
            aegisai_actuator::ProcfsLinuxProcessStateProvider,
            aegisai_actuator::CommandLinuxSyscallApplier::new(),
        );
    Ok(Actuator::with_backend(
        LinuxActuatorBackend::with_named_executor("linux-command", executor),
    ))
}

#[cfg(not(target_os = "linux"))]
fn build_linux_command_actuator() -> Result<Actuator, String> {
    Err("`linux-command` actuator backend is only available on Linux".to_string())
}

#[cfg(test)]
mod tests {
    use super::CliConfig;

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
    fn cli_accepts_linux_command_backend_name() {
        let cli = CliConfig::parse(
            ["--actuator-backend", "linux-command"]
                .into_iter()
                .map(str::to_string),
        )
        .expect("cli should parse");

        assert_eq!(cli.actuator_backend, "linux-command");
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
}
