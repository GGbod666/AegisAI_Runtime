# Latest Task List

_Generated: 2026-05-10_

This is a precise execution list derived from `README.md`, `docs/status.md`,
`docs/strategy.md`, `docs/architecture.md`, and the current open `bd` issues.
`bd` remains the source of truth for task state; this file explains the concrete
work slices expected under each open gap.

## Work Ordering

1. Product evidence first: finish or falsify `inference_tail_guard` and
   `tool_call_booster` benefit claims before broad production work.
2. Portability and production config next: remove host/config ambiguity before
   packaging.
3. Live cpuset/background isolation and real warmup side effects require explicit
   safety boundaries before implementation.
4. Dashboard, GPU, and adaptive policy planning stays deferred until MVP benefit
   evidence is settled.

## T1. Re-run Inference Tail Guard With One Controlled Noise Variable

- Issue: `AegisAI_Runtime-2kz`
- Current evidence: `live_guarded_phase4_calibrated_20260510T043859Z` recorded
  `3` effective live host-level `taskset` actions, but the report failed as
  `noisy_workload`.
- Scope:
  - choose exactly one variable to change from the latest run: stress worker
    count, sample count, concurrency, prompt/model, or affinity/nice pairing
  - keep model, prompt shape, modes, PID allowlist, live confirmation, and
    artifact naming explicit in the run metadata
  - run `baseline`, `noop_observation`, `dry_run`, and `live_guarded` in the
    same report batch
- Acceptance:
  - new `docs/mvp_benefit_report.md` is generated from artifacts, not edited by
    hand
  - report includes `changed_variable`, mode contracts, per-round data, stable
    trend check, live effective action count, and failure/pass reason
  - `PASS` appears only if effective live action and stable repeated benefit are
    both present
- Verification:
  - `AEGISAI_VERIFY_LOG=/tmp/aegisai_inference_preflight.md bash bench/scripts/inference_tail_guard_preflight.sh`
  - live run command from `docs/strategy.md` with explicit
    `AEGISAI_LIVE_PID_ALLOWLIST`
  - `python3 -m unittest discover -s bench/scripts -p 'test_*.py'`

## T2. Add Reproducible Failure Classification For Inference Runs

- Issue: `AegisAI_Runtime-2kz`
- Current gap: the latest failure says `noisy_workload`, but the next run should
  distinguish between action ineffectiveness, workload variance, insufficient
  samples, and no measurable benefit.
- Scope:
  - verify that report code classifies `action_effectiveness`,
    `noisy_workload`, `insufficient_sample_size`, and `no_measurable_benefit`
    from observable fields
  - add or update report tests if any classification branch lacks coverage for
    the current artifact shape
  - record the classification in the generated report and CSV summary
- Acceptance:
  - each failed live guarded report has one explicit primary failure cause
  - ineffective/no-op live actions cannot be reported as host benefit
  - failure text points to the metric or contract field that drove the verdict
- Verification:
  - `python3 -m unittest discover -s bench/scripts -p 'test_*.py'`
  - `bash -n bench/scripts/inference_tail_guard_phase4_report.sh`

## T3. Re-run Tool Call Booster Guarded Benefit With Stable Executor Controls

- Issue: `AegisAI_Runtime-79d`
- Current evidence:
  `live_guarded_tcb_issue_94s_final_20260510T053527Z` has contract `PASS`, but
  benefit `FAIL` because `live_guarded` improved `0/3` comparable rounds by at
  least `5.0%`.
- Scope:
  - freeze executor workload shape, tool payload, concurrency, modes, and round
    count before running
  - keep `WarmupExecutor` plan/audit-only unless `AegisAI_Runtime-14r` changes
    that boundary
  - capture baseline, noop, dry-run, and live guarded summaries in one artifact
    root
- Acceptance:
  - report includes latency deltas, trigger counts, rollback counts, action
    errors, contract verdict, benefit verdict, and comparable-round count
  - noop/dry-run improvements are labeled control evidence only
  - live guarded `PASS` requires clean contract plus repeated latency
    improvement versus baseline
- Verification:
  - `python3 -m unittest discover -s bench/tool_call_booster -p 'test_*.py'`
  - `AEGISAI_TCB_MODES=baseline,noop_observation,dry_run,live_guarded AEGISAI_CONFIRM_LIVE_ACTUATOR=1 bash bench/scripts/tool_call_booster_real_executor_harness.sh`

## T4. Decide The Real WarmupExecutor Boundary Before Coding It

- Issue: `AegisAI_Runtime-14r`
- Current gap: `warmup_executor = true` exists in config examples, but the
  backend currently records deferred apply and no-op rollback only.
- Scope:
  - identify the concrete target of warmup: process launch, cache priming,
    connection pool, retrieval index, or explicit no-side-effect decision
  - define what apply success, rollback/no-op, timeout, and audit fields mean
  - decide whether the side effect belongs in actuator backend, tool-call
    harness, or a new narrow executor adapter
- Acceptance:
  - design note or issue update states `implement` or `keep plan/audit-only`
    with rationale
  - if implemented, reports distinguish scheduler benefit from warmup benefit
  - tests cover apply success, timeout/failure, and rollback/audit semantics
- Verification:
  - focused Rust tests for touched actuator/runtime modules
  - `python3 -m unittest discover -s bench/tool_call_booster -p 'test_*.py'`

## T5. Implement Production Config Profile Selection

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

## T6. Add Production Config Schema And Cross-File Safety Checks

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

## T7. Validate Helper Portability On At Least Two Kernel Profiles

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

## T8. Add Helper Compatibility Fallbacks Or Explicit Skips

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

## T9. Define Live Cpuset And Background Isolation Safety Contract

- Issue: `AegisAI_Runtime-otk`
- Current gap: cpuset/background throttling exists in policy/audit surfaces, but
  live controls are not enabled.
- Scope:
  - define allowed cgroup roots, ownership requirements, rollback capture,
    maximum affected process set, and emergency restore behavior
  - specify how interactive AI tasks and background jobs are classified before
    isolation can apply
  - decide whether cpuset writes are daemon-owned, helper-owned, or explicitly
    deferred
- Acceptance:
  - safety contract names all host files or commands that may be touched
  - policy cannot enable live cpuset writes by config alone
  - rollback failure modes are auditable and leave operator instructions
- Verification:
  - design update in `docs/architecture.md` or issue notes
  - no live implementation until safety contract is reviewed

## T10. Implement A Minimal Cpuset/Background Dry-Run Planner

- Issue: `AegisAI_Runtime-otk`
- Depends on: T9
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

## T11. Package Runtime Daemon And Privileged Helper For One Linux Target

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

## T12. Add Production Startup Preflight For Packaged Runs

- Issue: `AegisAI_Runtime-ufp`
- Depends on: T5 and T11
- Scope:
  - check selected profile, kernel version, cgroup layout, procfs fields,
    helper readiness, binary versions, writable runtime/log directories, and
    live actuator disabled-by-default state
  - emit a single PASS/FAIL/SKIPPED summary suitable for service install logs
  - avoid appending to `docs/verification_log.md` during package install unless
    explicitly requested
- Acceptance:
  - preflight fails closed for missing required surfaces
  - optional tools are reported as `SKIPPED`, not hidden
  - output names the exact failed prerequisite
- Verification:
  - shell syntax checks
  - focused script tests if added
  - `AEGISAI_VERIFY_LOG=/tmp/aegisai_packaged_preflight.md` when using existing
    verification scripts

## T13. Split Deferred Dashboard, GPU, And Adaptive Policy Into Epics

- Issue: `AegisAI_Runtime-0ry`
- Current gap: README lists dashboard, GPU scheduler, and online adaptive policy
  as absent, but they are not ready for implementation until MVP evidence is
  settled.
- Scope:
  - create separate epics or issues for dashboard, GPU host coordination, and
    adaptive policy learning
  - for each, define user value, required telemetry inputs, safety boundary,
    first non-live prototype, and explicit non-goals
  - block implementation issues on product-evidence completion where necessary
- Acceptance:
  - no mixed “advanced extensions” umbrella issue remains as the only tracking
    unit
  - each epic has a first concrete task and a clear deferral condition
  - GPU/adaptive tasks cannot change live scheduler state without a safety issue
- Verification:
  - `bd list --status=open`
  - issue dependency graph shows deferred implementation behind evidence/safety
    prerequisites

## T14. Keep Hotspot Refactors Attached To Behavior Work

- Related limits: README hotspot list and `docs/architecture.md`
- Current hotspots:
  - `agent/runtime_daemon/src/source.rs`
  - `agent/actuator/src/backend.rs`
  - `agent/explain_tune/src/engine.rs`
  - `agent/runtime_orchestrator/src/runtime_orchestrator.rs`
  - `agent/policy_engine/src/engine.rs`
  - `bench/scripts/inference_tail_guard_ollama_smoke.sh`
- Scope:
  - when a task touches a hotspot, extract at most one cohesive concern
  - tie the split to the active behavior issue that required the change
  - preserve CLI/script output unless the active issue explicitly changes it
- Acceptance:
  - every hotspot split names its driving issue in commit or issue notes
  - focused tests cover the moved behavior
  - no standalone style-only hotspot split lands
- Verification:
  - crate-specific tests or script tests for the touched hotspot
  - `git diff --stat` remains scoped to the active behavior slice
