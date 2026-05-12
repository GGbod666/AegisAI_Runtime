#![forbid(unsafe_code)]

use std::collections::BTreeSet;
use std::env;

use aegisai_runtime_daemon::{
    bpftrace_required_fields, BpfTraceCompatibilityReport, BpfTracePipe, ProcfsTargetSelectors,
    SourceError, SystemBpfTracePipe,
};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let cli = Cli::parse(env::args().skip(1))?;
    match cli.command {
        HelperCommand::Check => check_helper(),
        HelperCommand::Compatibility(config) => compatibility(config),
        HelperCommand::Stream(config) => stream(config),
    }
    .map_err(|error| error.into())
}

#[derive(Debug, PartialEq, Eq)]
struct Cli {
    command: HelperCommand,
}

#[derive(Debug, PartialEq, Eq)]
enum HelperCommand {
    Check,
    Compatibility(SignalConfig),
    Stream(StreamConfig),
}

#[derive(Debug, Default, PartialEq, Eq)]
struct SignalConfig {
    include_offcpu: bool,
    include_io: bool,
}

#[derive(Debug, Default, PartialEq, Eq)]
struct StreamConfig {
    signals: SignalConfig,
    process_names: Vec<String>,
    pids: BTreeSet<u32>,
    bpftrace: Option<String>,
}

impl Cli {
    fn parse<I>(args: I) -> Result<Self, String>
    where
        I: IntoIterator<Item = String>,
    {
        let mut args = args.into_iter();
        let Some(command) = args.next() else {
            return Err(Self::usage());
        };

        match command.as_str() {
            "--check" | "check" => Ok(Self {
                command: HelperCommand::Check,
            }),
            "compatibility" => Ok(Self {
                command: HelperCommand::Compatibility(SignalConfig::parse(args, "compatibility")?),
            }),
            "stream" => Ok(Self {
                command: HelperCommand::Stream(StreamConfig::parse(args)?),
            }),
            "--help" | "-h" | "help" => Err(Self::usage()),
            other => Err(format!("unknown command `{other}`\n\n{}", Self::usage())),
        }
    }

    fn usage() -> String {
        [
            "Usage: aegisai-ebpf-helper --check",
            "       aegisai-ebpf-helper compatibility [--offcpu] [--io]",
            "       aegisai-ebpf-helper stream [--offcpu] [--io] [--pid <pid>] [--process-name <name>] [--bpftrace <path>]",
            "",
            "This helper is the narrow privileged eBPF boundary for AegisAI Runtime.",
            "Run the main runtime daemon as a normal user; install this helper with the privileges needed to attach eBPF probes.",
        ]
        .join("\n")
    }
}

impl SignalConfig {
    fn parse<I>(args: I, command: &str) -> Result<Self, String>
    where
        I: IntoIterator<Item = String>,
    {
        let mut config = Self::default();
        for arg in args {
            match arg.as_str() {
                "--offcpu" => config.include_offcpu = true,
                "--io" => config.include_io = true,
                "--help" | "-h" => return Err(Cli::usage()),
                other => return Err(format!("unknown {command} argument `{other}`")),
            }
        }

        if !(config.include_offcpu || config.include_io) {
            return Err(format!(
                "{command} requires at least one of --offcpu or --io"
            ));
        }

        Ok(config)
    }
}

impl StreamConfig {
    fn parse<I>(args: I) -> Result<Self, String>
    where
        I: IntoIterator<Item = String>,
    {
        let mut config = Self::default();
        let mut args = args.into_iter();

        while let Some(arg) = args.next() {
            match arg.as_str() {
                "--offcpu" => config.signals.include_offcpu = true,
                "--io" => config.signals.include_io = true,
                "--pid" => {
                    let raw = args
                        .next()
                        .ok_or_else(|| "--pid expects a positive integer".to_string())?;
                    let pid = raw
                        .parse::<u32>()
                        .map_err(|_| format!("invalid --pid value `{raw}`"))?;
                    if pid == 0 {
                        return Err("--pid cannot be 0".to_string());
                    }
                    config.pids.insert(pid);
                }
                "--process-name" => {
                    let name = args
                        .next()
                        .ok_or_else(|| "--process-name expects a value".to_string())?;
                    if name.trim().is_empty() {
                        return Err("--process-name cannot be empty".to_string());
                    }
                    config.process_names.push(name);
                }
                "--bpftrace" => {
                    config.bpftrace = Some(
                        args.next()
                            .ok_or_else(|| "--bpftrace expects a path".to_string())?,
                    );
                }
                "--help" | "-h" => return Err(Cli::usage()),
                other => return Err(format!("unknown stream argument `{other}`")),
            }
        }

        if !(config.signals.include_offcpu || config.signals.include_io) {
            return Err("stream requires at least one of --offcpu or --io".to_string());
        }

        Ok(config)
    }
}

fn check_helper() -> Result<(), String> {
    let pipe = env::var("AEGISAI_BPFTRACE")
        .map(SystemBpfTracePipe::new)
        .unwrap_or_default();

    pipe.check_available().map_err(|error| {
        format!("privileged helper cannot attach bpftrace eBPF probes yet: {error}")
    })
}

fn compatibility(config: SignalConfig) -> Result<(), String> {
    let pipe = env::var("AEGISAI_BPFTRACE")
        .map(SystemBpfTracePipe::new)
        .unwrap_or_default();
    let report = pipe.check_compatibility(config.include_offcpu, config.include_io);
    if pipe.check_available().is_err() {
        println!(
            "{}",
            BpfTraceCompatibilityReport::unavailable(
                bpftrace_required_fields(config.include_offcpu, config.include_io),
                "bpftrace eBPF backend is unavailable to the privileged helper",
            )
            .describe()
        );
        return Ok(());
    }

    println!("{}", report.describe());
    Ok(())
}

fn stream(config: StreamConfig) -> Result<(), String> {
    let selectors = ProcfsTargetSelectors::new(config.process_names, config.pids);
    let mut pipe = config
        .bpftrace
        .map(SystemBpfTracePipe::new)
        .unwrap_or_default();

    pipe.check_available().map_err(|error| {
        format!("bpftrace eBPF backend is not available for privileged helper: {error}")
    })?;
    pipe.start(
        &selectors,
        config.signals.include_offcpu,
        config.signals.include_io,
    )
    .map_err(format_source_error)?;

    loop {
        for line in pipe.read_lines(128, 1_000).map_err(format_source_error)? {
            println!("{line}");
        }
    }
}

fn format_source_error(error: SourceError) -> String {
    error.to_string()
}

#[cfg(test)]
mod tests {
    use super::{Cli, HelperCommand, SignalConfig, StreamConfig};

    #[test]
    fn parses_check_command() {
        let cli = Cli::parse(["--check"].into_iter().map(str::to_string)).expect("parse");

        assert_eq!(cli.command, HelperCommand::Check);
    }

    #[test]
    fn parses_stream_selectors() {
        let cli = Cli::parse(
            [
                "stream",
                "--offcpu",
                "--io",
                "--pid",
                "42",
                "--process-name",
                "ollama",
                "--bpftrace",
                "/usr/bin/bpftrace",
            ]
            .into_iter()
            .map(str::to_string),
        )
        .expect("parse");

        assert_eq!(
            cli.command,
            HelperCommand::Stream(StreamConfig {
                signals: SignalConfig {
                    include_offcpu: true,
                    include_io: true,
                },
                process_names: vec!["ollama".to_string()],
                pids: [42].into_iter().collect(),
                bpftrace: Some("/usr/bin/bpftrace".to_string()),
            })
        );
    }

    #[test]
    fn parses_compatibility_signals() {
        let cli = Cli::parse(
            ["compatibility", "--offcpu", "--io"]
                .into_iter()
                .map(str::to_string),
        )
        .expect("parse");

        assert_eq!(
            cli.command,
            HelperCommand::Compatibility(SignalConfig {
                include_offcpu: true,
                include_io: true,
            })
        );
    }

    #[test]
    fn rejects_stream_without_signal() {
        let error =
            Cli::parse(["stream"].into_iter().map(str::to_string)).expect_err("signal required");

        assert!(error.contains("at least one"));
    }

    #[test]
    fn rejects_compatibility_without_signal() {
        let error = Cli::parse(["compatibility"].into_iter().map(str::to_string))
            .expect_err("signal required");

        assert!(error.contains("at least one"));
    }
}
