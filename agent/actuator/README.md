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
- `CpuAffinityPlanner`: parses configured affinity, intersects it with online CPUs, selects apply targets, and formats deterministic rollback targets

## Current Backends

- `NoopActuatorBackend`
  Safe default for host-independent development and integration testing.

- `RecordingActuatorBackend`
  Deterministic backend for backend-focused tests and operation tracing.

- `LinuxActuatorBackend`
  Linux integration backend that can run as a skeleton/planned backend, a dry-run command backend, or a guarded live command backend depending on CLI selection and guard configuration.

- `ProcfsLinuxProcessStateProvider`
  Linux-only state provider that captures original scheduler-facing state from `/proc` before real syscall wiring lands.

- `PlannedLinuxSyscallApplier`
  Default applier used during host-independent development. It records what would be applied and rolled back without issuing real syscalls.

- `CommandLinuxSyscallApplier`
  Linux-oriented applier that maps action plans onto host commands such as `renice` and `taskset`. It bounds nice / affinity inputs, refuses uncaptured restore state and PID 0, and emits per-command audit details for validation.
  `WarmupExecutor` is disabled by default; when a command backend is explicitly
  given a warmup command and positive timeout, it runs that bounded command and
  audits success, timeout, or failure. Rollback remains a no-op audit because
  cache/process priming is not reversible.

- `OwnedCgroupIsolationApplier`
  Low-level cgroup v2 isolation applier for the Tail Guard Phase 5 path. It is
  not connected to production profile `use_cpuset` yet. It only writes
  administrator-created AegisAI-owned cgroup subtrees after explicit
  confirmation, PID allowlist validation, process classification checks, and
  rollback capture. Apply failures disable the applier and attempt rollback
  with audited success-rate fields.

- `DryRunLinuxCommandRunner`
  Command runner for Linux VM preflight rehearsals. It builds the same command arguments as the host runner but returns auditable `dry_run:` details without invoking host commands.

## Why This Split Exists

We need to keep host-independent development productive without pretending that
mock or dry-run paths are final validation for scheduling actions.

This backend split lets us:

- verify the control loop with a safe backend before live host effects
- preserve a stable interface for future Linux syscall or cgroup implementations
- keep rollback behavior and lease timing inside one shared place
- avoid hard-coding Linux execution details into the orchestrator
