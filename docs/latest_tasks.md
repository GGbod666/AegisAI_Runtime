# Latest Task List

_Generated: 2026-05-11_

This is a precise execution list derived from `bd`, `README.md`,
`docs/status.md`, `docs/strategy.md`, and `docs/architecture.md`. `bd` remains
the source of truth for task state; this file explains the concrete work slices
expected under each open gap.

## Current Evidence Disposition

- Inference Tail Guard: strict live guarded benefit proof is accepted for
  `live_guarded_phase4_sample_sizing_20260511T000000Z`.
- Tool Call Booster: the stable executor-control run
  `live_guarded_tcb_stable_executor_20260511T000000Z` has contract `PASS` and
  benefit `FAIL`; this is the current reproducible falsification for guarded
  scheduler benefit on this host/run shape.
- `WarmupExecutor` has an implemented explicit command boundary and remains
  reported separately from scheduler benefit.
- Live cpuset/background writes remain disabled; only the follow-up dry-run
  planner is open.

## Work Ordering

1. Keep policy safety semantics consistent across generic and scenario paths.
2. Remove production config ambiguity before packaging or unattended service
   work.
3. Validate helper portability and probe compatibility across host profiles.
4. Add cpuset/background dry-run planning before any live cgroup write work.
5. Package only after config, helper, and safety boundaries are explicit.
6. Dashboard, GPU, and adaptive policy planning stays deferred.

## T1. Harden Generic Policy Safety Cap Normalization

- Issue: `AegisAI_Runtime-vv2`
- Current gap: Tool Call Booster locally normalizes negative priority caps and
  non-finite or zero affinity ratios, but generic policy and non-TCB paths can
  still rely on raw `SafetyConfig` values.
- Scope:
  - apply equivalent priority delta and affinity ratio semantics in the shared
    policy path
  - keep the existing Tool Call Booster audit behavior intact
  - add targeted tests for generic/non-TCB normalization
- Acceptance:
  - invalid or hostile safety cap values cannot produce broader-than-allowed
    scheduler actions
  - scenario-specific behavior stays unchanged except for normalized shared
    safety semantics
- Verification:
  - focused policy tests
  - `cargo test -p aegisai-policy-engine`

## T2. Implement Production Config Profile Selection

- Issue: `AegisAI_Runtime-cqv`
- Current gap: runtime loads fixed `configs/*/*.example.toml` through a minimal
  parser; there is no production profile selection or discovery contract.
- Scope:
  - add a profile selector with precedence: CLI flag, environment variable,
    documented local-development default
  - validate profile names as identifiers: lowercase letters, digits, `_`, `-`
  - reject path separators, `.` segments, empty names, and absolute paths
  - keep existing example-file loader as test/demo compatibility path
- Acceptance:
  - production path does not silently load `*.example.toml`
  - missing profile files fail before partial startup
  - tests cover valid profile, invalid names, missing profile root, and local
    default compatibility
- Verification:
  - targeted tests in `agent/runtime_orchestrator`
  - `cargo test -p aegisai-runtime-orchestrator`
  - `cargo test -p aegisai-runtime-daemon` if CLI wiring changes

## T3. Add Production Config Schema And Cross-File Safety Checks

- Issue: `AegisAI_Runtime-cqv`
- Current gap: config parsing accepts the current example shape but does not
  enforce a complete production schema.
- Scope:
  - validate TOML syntax, known keys, required fields, enum values, numeric
    ranges, and duration limits
  - reject unknown production keys unless explicitly documented as metadata
  - cross-check scenario actions against global safety limits and source
    `focus_signals`
  - report profile, file, section, key, and violated constraint in errors
- Acceptance:
  - invalid `raise_nice`, boost duration, focus signal, scenario enablement, and
    unknown-key cases have tests
  - production schema errors are deterministic and actionable
  - demo/example compatibility remains intact
- Verification:
  - `cargo test -p aegisai-runtime-orchestrator`
  - `cargo test --workspace` if shared config contracts change

## T4. Validate Helper Portability On At Least Two Kernel Profiles

- Issue: `AegisAI_Runtime-51c`
- Current evidence: helper-backed `offcpu_time` and `io_latency` are validated
  on `gg-vm`; I/O depends on block tracepoint fields such as `dev` and `sector`.
- Scope:
  - select at least two supported kernel profiles, for example Ubuntu generic
    and another distro/kernel line
  - run helper readiness, raw helper stream attach, controlled off-CPU workload,
    and controlled block I/O workload on each profile
  - capture kernel version, bpftrace version, tracefs path, tracepoint field
    inventory, raw event counts, and daemon normalized event counts
- Acceptance:
  - each profile ends in exactly one bucket: `helper unavailable`,
    `tracepoint incompatible`, `no workload events`, or `validated signal`
  - incompatible tracepoints name the exact missing probe or field
  - validated profiles record artifact root and command lines
- Verification:
  - `AEGISAI_VERIFY_LOG=/tmp/aegisai_toolchain_preflight.md bash bench/scripts/toolchain_preflight.sh`
  - helper validation flow from `docs/linux_validation.md`
  - append to `docs/verification_log.md` only when intentionally recording
    durable evidence

## T5. Add Helper Compatibility Fallbacks Or Explicit Skips

- Issue: `AegisAI_Runtime-51c`
- Current gap: if block tracepoint fields differ by kernel, the helper path
  needs compatibility handling or a precise skip reason.
- Scope:
  - isolate probe/field compatibility checks before starting the daemon run
  - select compatible bpftrace scripts by available field set, or skip with a
    structured `tracepoint incompatible` result
  - keep procfs-backed `run_queue_delay`, `cpu_migration`, and
    `major_page_fault` available under `--allow-partial-probes`
- Acceptance:
  - helper unavailability and tracepoint incompatibility produce different
    messages
  - partial-probe startup still succeeds when optional helper signals are
    unavailable
  - tests cover helper unavailable, incompatible tracepoint, and no workload
    events
- Verification:
  - targeted runtime daemon source tests
  - `cargo test -p aegisai-runtime-daemon`

## T6. Add A Minimal Cpuset/Background Dry-Run Planner

- Issue: `AegisAI_Runtime-7h5`
- Depends on: closed safety-boundary issue `AegisAI_Runtime-otk`
- Current gap: live cpuset/background writes are disabled by design, but there
  is no deterministic dry-run plan showing what would be touched or rejected.
- Scope:
  - generate planned cpuset/background isolation actions without writing cgroups
  - include target pids/cgroups, proposed CPU set, original state capture plan,
    rollback plan, and safety rejection reason
  - integrate with policy audit fields without changing live host state
- Acceptance:
  - dry-run output is deterministic and test-covered
  - rejected plans include exact reason: unsafe root, missing classification,
    empty CPU set, missing rollback state, or overbroad process set
  - live writes remain disabled
- Verification:
  - focused actuator/policy tests
  - `cargo test -p aegisai-actuator`
  - `cargo test -p aegisai-policy-engine`

## T7. Package Runtime Daemon And Privileged Helper For One Linux Target

- Issue: `AegisAI_Runtime-ufp`
- Current gap: no service packaging, installer, or unattended daemon management
  exists.
- Scope:
  - choose one first packaging target: systemd unit plus install script for a
    Debian/Ubuntu-like host, or an explicit alternative
  - package rootless daemon and privileged helper separately
  - define user/group, binary paths, config profile path, log path, helper
    capability/root boundary, uninstall, and rollback behavior
- Acceptance:
  - installer refuses unsupported kernel/cgroup/helper prerequisites with clear
    errors
  - daemon service does not run as root
  - helper install path documents and checks its privilege boundary
  - uninstall restores service files and leaves operator-owned configs intact
- Verification:
  - shell syntax checks for installer scripts
  - dry-run install test in a temp root if scripts support it
  - Linux VM smoke from `docs/linux_validation.md`

## T8. Plan Deferred Dashboard, GPU, And Adaptive Policy Extensions

- Issue: `AegisAI_Runtime-0ry`
- Current gap: deferred extensions are intentionally outside the MVP loop but
  still need bounded design notes before any implementation starts.
- Scope:
  - identify what evidence each extension would need before becoming active
  - keep GPU coordination and adaptive policy out of the hot path until safety
    and measurement contracts exist
  - avoid adding UI or online learning work ahead of production config,
    helper portability, and packaging
- Acceptance:
  - extension plans state prerequisites, non-goals, and verification gates
  - no runtime behavior changes are introduced by planning work
- Verification:
  - design update only
  - no code verification unless implementation is explicitly added
