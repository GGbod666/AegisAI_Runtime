# actuator

`actuator` owns bounded action lifecycle management for the AegisAI Runtime control loop.

The crate is now split into two layers:

- `Actuator`: lease tracking, expiry, rollback scheduling, and audit-field merging
- execution backends: pluggable implementations for different host environments

For the Linux path, the backend is split one level further:

- `LinuxActuatorBackend`: AegisAI-facing backend wrapper
- `LinuxSyscallExecutor`: Linux execution hook that will later own real syscall wiring
- `LinuxProcessStateProvider`: capture hook for original nice / affinity / cpuset state
- `LinuxSyscallApplier`: syscall-application hook for apply / rollback execution
- `LinuxCapturedState`: typed placeholder for original nice / affinity / cpuset state
- `LinuxRollbackReport`: typed audit report for restore, missing-state, and future failure paths

## Current Backends

- `NoopActuatorBackend`
  Safe default for Windows-side development and integration testing.

- `RecordingActuatorBackend`
  Deterministic backend for backend-focused tests and operation tracing.

- `LinuxActuatorBackend`
  Linux integration skeleton reserved for the later VM validation phase.
  It does not execute real syscalls yet; it builds syscall plans, captures placeholder restore state, and hands them to a planned-only executor.

- `ProcfsLinuxProcessStateProvider`
  Linux-only state provider that captures original scheduler-facing state from `/proc` before real syscall wiring lands.

- `PlannedLinuxSyscallApplier`
  Default applier used during Windows-side development. It records what would be applied and rolled back without issuing real syscalls.

- `CommandLinuxSyscallApplier`
  Linux-oriented preflight applier that maps action plans onto host commands such as `renice` and `taskset`. It bounds nice / affinity inputs, refuses uncaptured restore state and PID 0, and emits per-command audit details for VM validation.

- `DryRunLinuxCommandRunner`
  Command runner for Linux VM preflight rehearsals. It builds the same command arguments as the host runner but returns auditable `dry_run:` details without invoking host commands.

## Why This Split Exists

We need to keep Windows-side development productive without pretending that Windows is the final validation host for scheduling actions.

This backend split lets us:

- verify the control loop on Windows with a safe backend
- preserve a stable interface for future Linux syscalls
- keep rollback behavior and lease timing inside one shared place
- avoid hard-coding Linux execution details into the orchestrator
