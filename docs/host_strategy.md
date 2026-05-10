# Host Strategy

Boundary: this file records the host split rationale. Use
`linux_vm_checklist.md` for executable Linux validation steps, `next_stage.md`
for current stage gates, and `bd` for active tasks.

## Development Host

Primary development can happen on Windows or any non-production development
host.

Non-Linux development hosts are used for:

- architecture work
- crate integration
- mock-source development
- control-loop verification
- configuration and documentation updates

Non-Linux hosts are intentionally not treated as final eBPF validation hosts.

## Validation Host

System-level validation happens on a Linux host or Linux VM.

Linux is required for:

- real `/proc` metadata enrichment
- real `/proc` scheduler-state capture for actuator rollback safety
- real probe-backed event ingestion
- eBPF program loading and validation
- runtime scheduling actions such as `nice` and `sched_setaffinity` / `taskset`
- benchmark data collection and final experiment results

## Phase Split

### Phase A: Non-Linux Buildout

Build and verify the runtime skeleton with:

- `MockEventSource`
- `StaticMetadataProvider`
- `NoopActuatorBackend`
- Linux probe planning and source-shape validation
- orchestrator integration tests
- local daemon runs

### Phase B: Linux System Validation

Complete Linux-only work on a Linux host or VM:

- dependency installation
- kernel and eBPF prerequisites
- helper-backed `offcpu_time` / `io_latency` stream validation
- Linux state capture / restore validation
- benchmark execution
- final metrics collection

Current disposition:

- Helper-backed `offcpu_time` and `io_latency` have been validated on Linux host
  `gg-vm`.
- Live guarded Inference Tail Guard and Tool Call Booster experiments have run
  on Linux and produced honest `FAIL` benefit verdicts rather than unproven
  `PASS` claims.
- Further Linux work is now about product evidence and portability, not first
  contact with Linux.

## Current Rule

When a feature can be validated with mock inputs and pure Rust integration, use
that path first.

When a feature depends on kernel behavior, `/proc`, scheduling syscalls, or eBPF
execution, validate it on Linux and record artifacts. Do not infer Linux benefit
from mock, noop, or dry-run behavior.
