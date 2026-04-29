# Host Strategy

## Development Host

Primary development happens on Windows.

Windows is used for:

- architecture work
- crate integration
- mock-source development
- control-loop verification
- configuration and documentation updates

Windows is intentionally not treated as the final eBPF validation host.

## Validation Host

System-level validation happens later on a Linux VM.

Linux is required for:

- real `/proc` metadata enrichment
- real `/proc` scheduler-state capture for actuator rollback safety
- real probe-backed event ingestion
- eBPF program loading and validation
- runtime scheduling actions such as `nice` and `sched_setaffinity`
- benchmark data collection and final experiment results

## Phase Split

### Phase A: Windows-first buildout

Build and verify the runtime skeleton on Windows with:

- `MockEventSource`
- `StaticMetadataProvider`
- `NoopActuatorBackend`
- Linux probe planning and source-shape validation
- orchestrator integration tests
- local daemon runs

### Phase B: Linux system validation

Copy the project into the Linux VM and complete:

- dependency installation
- kernel and eBPF prerequisites
- `ProbeEventReader` wiring for the real probe stream
- `LinuxActuatorBackend` syscall wiring and validation
- benchmark execution
- final metrics collection

## Current Rule

When a feature can be validated with mock inputs and pure Rust integration, do it on Windows now.

When a feature depends on kernel behavior, `/proc`, scheduling syscalls, or eBPF execution, design the interface now and defer the real validation to Linux.
