# Handoff

## Current State

The project has moved from architecture-only skeleton work into runnable pre-integration infrastructure.

The current Windows-side development baseline is:

- unified runtime contracts are in place
- orchestrator mainline is wired
- `runtime_daemon` exists and runs with mock sources
- actuator backends are split from lifecycle control
- Linux probe planning exists
- Linux probe reading is now split into `LinuxProbeDriver -> DriverBackedProbeEventReader`
- Linux probe preflight now validates tracefs / tracepoint / kprobe prerequisites before real loading
- Linux syscall execution is now split into `LinuxProcessStateProvider + LinuxSyscallApplier + LinuxSyscallExecutor`
- a Linux-only `linux-command` daemon backend is now available for command-backed preflight validation
- `git_control` now provides repository discovery, dirty-state snapshots, and checkpoint naming plans

## What Is Done

### 1. Unified control-loop mainline

The shared mainline is active:

`collector -> classifier -> policy_engine -> actuator -> metrics`

The orchestrator composes the real crates instead of private shadow implementations.

### 2. Runnable daemon entrypoint

`agent/runtime_daemon/` now provides:

- CLI entrypoint
- `EventSource` abstraction
- `MetadataProvider` abstraction
- `RuntimeLoop`
- mock development path for Windows
- lazy `ProbeEventReader` startup and shutdown tracking
- reader config scaffolding for partial attach policy and ring-buffer sizing
- `LinuxProbeDriver` as the attach / poll / stop seam for real probe ingestion
- `DriverBackedProbeEventReader` as the managed reader wrapper for future Linux probe wiring
- `PreflightLinuxProbeDriver` for Linux-side attach-point validation before real eBPF loading

Current runnable command:

```powershell
cargo run -p aegisai-runtime-daemon -- --repo-root . --source mock --metadata demo --actuator-backend noop
```

### 3. Actuator backend split

`agent/actuator/` now has:

- `Actuator` for lease tracking and rollback
- `NoopActuatorBackend`
- `RecordingActuatorBackend`
- `LinuxActuatorBackend`
- `LinuxSyscallExecutor` interface
- `LinuxSyscallApplier`
- `CommandLinuxSyscallApplier`
- `PlannedOnlyLinuxSyscallExecutor`
- `LinuxProcessStateProvider`
- `ProcfsLinuxProcessStateProvider`
- `LinuxCapturedState`
- `LinuxRollbackReport`

This means Linux syscall work can be added later without changing orchestrator wiring.

### 4. Git state helper

`agent/git_control/` now provides:

- repository root discovery
- branch / HEAD / ahead-behind / dirty-state snapshots
- normalized checkpoint naming plans for future experiment checkpoints
- a small CLI for `status` and `checkpoint --label ...`

### 4. Linux probe planning

`LinuxProbeSource` now supports:

- `focus_signals -> planned probes` mapping
- separation of kernel-probe signals vs runtime-only signals
- `ProbeEventReader` interface
- `StaticProbeEventReader` for adapter tests
- probe-event to source-event adaptation

Current `linux` source mode is intentionally a planning skeleton, not a real reader.

## Verification Status

The following pass at the end of today:

- `cargo check`
- `cargo test`
- `cargo clippy --all-targets --all-features -- -D warnings`
- `cargo fmt --all -- --check`

Current test count:

- `81` tests passing

Known environment warning:

- Rust tooling prints `could not canonicalize path C:\Users\Administrator`
- this has not blocked build or test execution

## Windows vs Linux Split

### Windows now

Use Windows for:

- architecture and integration work
- mock-source control-loop validation
- daemon CLI verification
- backend contract design

### Linux later

Use Linux VM for:

- real eBPF probe loading
- real probe event reader wiring
- `/proc`-based metadata validation
- real syscall executor wiring
- benchmark runs and final measurements

## Immediate Next Step

The next implementation target is:

### A. Real Linux reader implementation

The reader scaffold now exists. The next step is to plug in a real Linux reader behind it:

- real attach / detach logic inside `LinuxProbeDriver`
- ring-buffer ownership and polling loop inside the driver implementation
- partial startup handling backed by real probe failures
- startup and shutdown summaries populated from real driver state

### B. Real Linux syscall executor implementation

The rollback state scaffold now exists. The next step is to replace placeholder capture with real Linux state:

- original nice capture
- original affinity capture
- cpuset membership capture
- real syscall apply / rollback through `LinuxSyscallApplier`
- real rollback success / failure reporting

These should still be implemented as safe scaffolding first, without requiring Linux execution on Windows.

## Linux VM Checklist

The first Linux-side validation checklist now lives at:

`docs/linux_vm_checklist.md`

## Resume Plan For Tomorrow

1. Start with the real Linux `LinuxProbeDriver` implementation behind the managed reader scaffold.
2. Then replace `CommandLinuxSyscallApplier` with a lower-level Linux syscall applier.
3. Use `git_control` to define the checkpoint convention we will follow before Linux VM runs.
4. Re-run full workspace verification.
5. Only after that prepare the Linux VM execution checklist.
