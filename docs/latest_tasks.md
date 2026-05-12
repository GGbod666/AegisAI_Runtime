# Latest Task List

_Regenerated: 2026-05-12_

This is the current priority plan, not a historical gap inventory. It is based
on the latest repository state: all local Rust/Python/shell checks pass, the
controlled Linux source ingestion smoke records nonzero procfs-derived daemon
events, the project preflight path lists the Rust/Python/shell readiness gates,
and code-review-graph still marks live-action and source/report paths as
high-degree hotspots. `bd` remains the source of truth for status.

## Priority Rule

The order below is deliberate:

1. **P1: trust and safety blockers**. Fix anything that can make validation
   conclusions false, widen live actions, or hide a rollback/CLI safety bug.
2. **P2: production-readiness blockers**. Add the config, helper, and artifact
   evidence needed before packaging or unattended operation.
3. **P3: packaging boundary**. Define install/service boundaries only after
   config and live-safety semantics are explicit.
4. **P4: deferred extensions**. Dashboard, GPU, and adaptive policy stay behind
   the safety, config, helper, and packaging gates.

Within the same priority band, execute tasks in the order shown here. `bd ready`
may display equal-priority issues in a different order; this document is the
current planning order.

## Current Evidence Snapshot

- `cargo fmt --all -- --check`: `PASS`
- `cargo check --workspace`: `PASS`
- `cargo test --workspace`: `PASS`
- `cargo clippy --all-targets --all-features -- -D warnings`: `PASS`
- `python3 -m unittest discover -s bench/tool_call_booster -p 'test_*.py'`:
  `PASS`, `14` tests
- `python3 -m unittest discover -s bench/scripts -p 'test_*.py'`: `PASS`,
  `15` tests
- `for f in bench/scripts/*.sh; do bash -n "$f" || exit 1; done`: `PASS`
- `AEGISAI_VERIFY_LOG=/tmp/aegisai_audit_verify_workspace_20260511.md bash bench/scripts/verify_workspace.sh`:
  `PASS`; mock daemon `processed_events=3`, Linux preflight `processed_events=0`
- `AEGISAI_VERIFY_LOG=/tmp/aegisai_audit_toolchain_preflight_20260511.md bash bench/scripts/toolchain_preflight.sh`:
  `PASS`
- `AEGISAI_VERIFY_LOG=/tmp/aegisai_audit_inference_preflight_20260511.md bash bench/scripts/inference_tail_guard_preflight.sh`:
  `PASS`
- `bash bench/scripts/linux_source_ingestion_smoke.sh`: `PASS`;
  `processed_events=4`, `run_queue_delay` signal observation present
- `bd lint`: `PASS`

Open evidence gaps:

- Direct Linux source preflight is still a startup/partial-probe check; use
  `bench/scripts/linux_source_ingestion_smoke.sh` for ingestion proof.
- Inference preflight does not run a model or start stress load.
- Upstream `bd preflight` in bd `1.0.3` still prints Beads' own Go/Nix template;
  this repository's active readiness path is
  `bash bench/scripts/project_preflight.sh`.
- Graph analysis reports `20` untested hotspots and `16` files/classes with at
  least `500` lines.

## P1. Trust And Safety Blockers

These tasks come first because they affect whether later test results can be
trusted or whether live-control boundaries are safe.

### 1. Add Controlled Linux Source Ingestion Smoke

- Issue: `AegisAI_Runtime-51c.3`
- Status: `DONE` on 2026-05-12.
- Why first: current `verify_workspace.sh` can pass with Linux
  `processed_events=0`; until this is fixed, Linux source validation proves
  startup only, not event ingestion.
- Scope:
  - create or select a short-lived allowlisted process
  - run daemon with `linux-skeleton` or `linux-command-dry-run`
  - require at least one procfs-derived signal to reach the daemon summary
  - document exact skip conditions for hosts that cannot emit procfs deltas
- Acceptance:
  - smoke records `processed_events > 0`
  - summary includes `signal_observations` for at least one of
    `run_queue_delay`, `cpu_migration`, or `major_page_fault`
  - no live scheduler state is changed
  - failure and skip states are distinguishable
  - command is documented in `docs/linux_validation.md`
- Verification:
  - `bash bench/scripts/linux_source_ingestion_smoke.sh`: `PASS`;
    `processed_events=4`, `run_queue_delay` observation present
  - no Rust source code changed, so `cargo test -p aegisai-runtime-daemon` was
    not required for this task

### 2. Replace Project Preflight Template

- Issue: `AegisAI_Runtime-awq`
- Status: `DONE` on 2026-05-12.
- Why now: `bd preflight` currently prints Go/Nix checks, so a future handoff can
  report the wrong readiness gates even when Cargo/Python/shell checks pass.
- Scope:
  - added `bench/scripts/project_preflight.sh` as the visible project preflight
    path
  - listed actual Rust/Python/shell/bench gates in the project path
  - kept `bd lint` clean
- Acceptance:
  - active readiness instructions include `cargo fmt --all -- --check`
  - active readiness instructions include `cargo test --workspace`
  - active readiness instructions include
    `cargo clippy --all-targets --all-features -- -D warnings`
  - active readiness instructions include both Python unittest discovery commands
  - active readiness instructions include shell syntax and bench preflight gates
  - upstream `bd preflight` Go/Nix commands are explicitly marked irrelevant to
    this repository
- Verification:
  - `bash bench/scripts/project_preflight.sh`: `PASS`
  - `bash bench/scripts/project_preflight.sh --check`: `PASS`
  - `bash -n bench/scripts/project_preflight.sh`: `PASS`
  - `bd preflight`: boundary confirmed; output still shows upstream Beads Go/Nix
    template and is explicitly marked irrelevant
  - `bd lint`: `PASS`

### 3. Normalize Generic Safety Caps

- Issue: `AegisAI_Runtime-vv2.1`
- Parent: `AegisAI_Runtime-vv2`
- Status: `DONE` on 2026-05-12.
- Why P1: invalid global safety caps can affect action breadth. This must be
  fixed before production config work makes more paths selectable.
- Scope:
  - added shared `SafetyConfig` normalization helpers for priority delta and
    affinity ratio caps
  - applied normalized caps in generic policy code and scenario policy code
  - preserved Tool Call Booster audit output and existing benefit interpretation
- Acceptance:
  - negative `max_priority_delta` cannot widen scheduler actions
  - zero, non-finite, or invalid affinity ratios cannot widen scheduler actions
  - valid caps still produce expected plans
  - existing Tool Call Booster policy tests keep passing
- Verification:
  - `cargo fmt --all -- --check`: `PASS`
  - `cargo test -p aegisai-policy-engine`: `PASS`; `14` tests
  - `cargo test -p aegisai-runtime-contracts`: `PASS`
  - `cargo test -p runtime_orchestrator tool_call_trace_preserves_safety_clamp_audit_fields`:
    `PASS`

### 4. Implement Cpuset Dry-Run Rejection Matrix

- Issue: `AegisAI_Runtime-7h5.1`
- Parent: `AegisAI_Runtime-7h5`
- Status: `DONE` on 2026-05-12.
- Why P1: cpuset/background writes are disabled, but the next safe step is a
  deterministic rejection planner. Without it, future cgroup work lacks a test
  boundary.
- Scope:
  - added `agent/actuator/src/cpuset_dry_run.rs` as a dry-run-only planner
    with no cgroupfs write path
  - emits deterministic rejection reason strings plus target pid/cgroup context
  - emits capture and rollback plan context when rollback capture is available
- Acceptance:
  - unsafe cgroup root is rejected: `PASS`
  - missing classification is rejected: `PASS`
  - empty computed CPU set is rejected: `PASS`
  - missing rollback capture is rejected: `PASS`
  - overbroad process set is rejected: `PASS`
  - unsupported live write mode is rejected: `PASS`
  - live writes remain disabled: `PASS`
- Verification:
  - `cargo fmt --all -- --check`: `PASS`
  - `cargo test -p aegisai-actuator`: `PASS`; `44` tests

### 5. Directly Test Linux Rollback Report Builder

- Issue: `AegisAI_Runtime-yxb`
- Status: `DONE` on 2026-05-12.
- Why P1: `build_linux_rollback_report` is the top graph hub
  (`degree=102`) and is directly tied to live-action audit credibility.
- Scope:
  - added focused direct tests around rollback audit composition in
    `agent/actuator/src/backend.rs`
  - avoided backend runtime refactors
  - preserved existing backend field names and report text
- Acceptance:
  - tests cover successful nice rollback: `PASS`
  - tests cover failed nice rollback: `PASS`
  - tests cover successful affinity rollback: `PASS`
  - tests cover failed affinity rollback: `PASS`
  - tests cover mixed nice/affinity action reports: `PASS`
  - tests cover missing captured state: `PASS`
  - tests prove disabled cpuset actions do not emit rollback noise: `PASS`
- Verification:
  - `cargo fmt --all -- --check`: `PASS`
  - `cargo test -p aegisai-actuator`: `PASS`; `51` tests
  - `git diff --check`: `PASS`

### 6. Expand CLI Parser Edge-Case Tests

- Issue: `AegisAI_Runtime-d42`
- Why P1: `CliConfig::parse` is the second graph hub (`degree=101`) and guards
  live actuator confirmation, PID allowlists, warmup side effects, and future
  profile selection.
- Scope:
  - add parser tests only
  - cover live actuator and warmup command boundaries
  - cover production profile interactions if that flag lands first
- Acceptance:
  - duplicate `--live-pid-allowlist` behavior is deterministic
  - whitespace and empty PID elements are rejected or normalized by a tested rule
  - unknown source/backend names produce deterministic errors
  - `--verification-log` missing value is rejected
  - warmup command argument boundaries are covered
  - production profile flag interactions are covered if available
- Verification:
  - `cargo test -p aegisai-runtime-daemon`

## P2. Production-Readiness Blockers

These tasks do not outrank the P1 trust/safety work, but they block packaging,
cross-host validation, and unattended operation.

### 7. Add BpfTracePipe Startup Failure Taxonomy Tests

- Issue: `AegisAI_Runtime-51c.4`
- Parent: `AegisAI_Runtime-51c`
- Why P2: helper portability depends on stable failure categories before testing
  multiple hosts.
- Scope:
  - add focused runtime daemon source tests
  - distinguish startup failures from malformed event parsing
  - keep partial-probe reporting text stable
- Acceptance:
  - tests distinguish missing helper or bpftrace binary
  - tests distinguish permission failure
  - tests cover stdout capture failure
  - tests cover stderr capture failure
  - tests cover malformed probe lines
  - tests cover unsupported signal
  - tests cover child stop cleanup
- Verification:
  - `cargo test -p aegisai-runtime-daemon source::tests`

### 8. Classify Helper Compatibility Before Daemon Start

- Issue: `AegisAI_Runtime-51c.1`
- Parent: `AegisAI_Runtime-51c`
- Why P2: current helper checks can conflate helper unavailability, tracepoint
  incompatibility, and no workload events.
- Scope:
  - inspect helper availability
  - inspect tracefs root and tracepoint field inventory
  - classify compatibility before long daemon runs
  - keep procfs fallback available under `--allow-partial-probes`
- Acceptance:
  - helper unavailable and tracepoint incompatible are distinct results
  - missing block tracepoint fields name the missing field
  - compatible inventory is recorded
  - no workload events remains separate from compatibility failure
- Verification:
  - `cargo test -p aegisai-runtime-daemon`
  - helper preflight command from `docs/linux_validation.md`

### 9. Run Two-Kernel Helper Portability Matrix

- Issue: `AegisAI_Runtime-51c.2`
- Parent: `AegisAI_Runtime-51c`
- Why after compatibility classification: matrix results are useful only if
  each host lands in a precise bucket.
- Scope:
  - run helper readiness on at least two supported Linux kernel profiles
  - run raw helper stream attach
  - run controlled off-CPU workload
  - run controlled block I/O workload
  - record daemon normalized event counts
- Acceptance:
  - each profile records kernel and distro
  - each profile records bpftrace version
  - each profile records tracefs root
  - each profile records tracepoint field inventory
  - each profile records raw and normalized event counts
  - each profile ends in exactly one bucket:
    `helper unavailable`, `tracepoint incompatible`, `no workload events`, or
    `validated signal`
- Verification:
  - `AEGISAI_VERIFY_LOG=/tmp/aegisai_toolchain_preflight.md bash bench/scripts/toolchain_preflight.sh`
  - helper validation flow from `docs/linux_validation.md`
  - intentional durable entry in `docs/verification_log.md`

### 10. Add Inference Smoke Artifact Tests

- Issue: `AegisAI_Runtime-fp6`
- Why P2: benchmark artifacts support benefit claims. The current scripts pass
  syntax/unit tests, but run-env output from the live smoke path is still a
  graph hotspot.
- Scope:
  - add a deterministic script-level test or harness for run-env output
  - do not run live workloads inside the unit test
  - prevent failure artifacts from looking like accepted proof
- Acceptance:
  - run-env output records run id
  - run-env output records mode
  - run-env output records model and prompt/workload shape
  - run-env output records stress shape and sample count
  - run-env output records live flags and artifact paths
  - failure paths do not write misleading `PASS` fields
- Verification:
  - `bash -n bench/scripts/inference_tail_guard_ollama_smoke.sh`
  - the new script-level test command

### 11. Add Runtime Production Profile Selector

- Issue: `AegisAI_Runtime-cqv.2`
- Parent: `AegisAI_Runtime-cqv`
- Why before schema/cross-file checks: production validation needs a real
  profile target instead of fixed `*.example.toml` paths.
- Scope:
  - add selector precedence: CLI flag, environment variable, documented local
    demo default
  - validate profile names as identifiers only
  - preserve current example compatibility for tests and local demos
- Acceptance:
  - valid profile names load non-example profile files
  - empty names are rejected
  - absolute paths are rejected
  - path separators are rejected
  - `.` segments are rejected
  - missing profile root fails before partial startup
  - CLI/env/default precedence has tests
- Verification:
  - `cargo test -p aegisai-runtime-orchestrator`
  - `cargo test -p aegisai-runtime-daemon` if CLI wiring changes

### 12. Add Production Config Schema Validation

- Issue: `AegisAI_Runtime-cqv.3`
- Parent: `AegisAI_Runtime-cqv`
- Why after selector: schema errors need to name the selected production
  profile and file.
- Scope:
  - validate known keys and required fields
  - validate enum values, numeric ranges, and duration limits
  - preserve demo/example compatibility
- Acceptance:
  - unknown production key errors include profile, file, section, key, and
    violated constraint
  - missing required field errors include the same context
  - invalid signal, scenario, and action enum cases are tested
  - invalid `raise_nice` and duration cases are tested
- Verification:
  - `cargo test -p aegisai-runtime-orchestrator`

### 13. Add Config Cross-File Safety Validation

- Issue: `AegisAI_Runtime-cqv.1`
- Parent: `AegisAI_Runtime-cqv`
- Why after schema: cross-file checks should run on validated profile data.
- Scope:
  - validate scenario action limits against global safety
  - validate enabled scenario triggers against `focus_signals`
  - validate unsupported live affinity/cpuset combinations for the selected mode
- Acceptance:
  - duration above `max_boost_duration_ms` is rejected
  - priority delta outside `max_priority_delta` is rejected
  - trigger requiring absent `focus_signals` is rejected
  - unsupported live affinity/cpuset mode is rejected
  - errors name both files involved
- Verification:
  - `cargo test -p aegisai-runtime-orchestrator`

## P3. Packaging Boundary

Packaging should not start before P1/P2 gates clarify safety, config, and helper
semantics.

### 14. Define Debian/Systemd Packaging Contract

- Issue: `AegisAI_Runtime-ufp.1`
- Parent: `AegisAI_Runtime-ufp`
- Why P3: package design is useful now, but installer code should wait until
  production config and helper boundaries are stable.
- Scope:
  - choose Debian/Ubuntu systemd or document a different first target
  - specify daemon and helper boundaries separately
  - specify rollback and uninstall behavior
- Acceptance:
  - contract names daemon user/group
  - contract names binary paths
  - contract names config profile path
  - contract names log path
  - contract states helper privilege boundary
  - contract states capabilities/root requirement
  - contract states unsupported prerequisite behavior
  - no installer code is required in this task
- Verification:
  - docs-only review
  - no code verification unless files/scripts are added

## P4. Deferred Extensions

These remain deliberately last. They should not consume implementation effort
until the control loop has production config, Linux ingestion proof, helper
portability evidence, and packaging boundaries.

### 15. Split Deferred Extensions Into Evidence-Gated Epics

- Issue: `AegisAI_Runtime-0ry.1`
- Parent: `AegisAI_Runtime-0ry`
- Why P4: dashboard, GPU coordination, and adaptive policy are future product
  directions, not blockers for current runtime correctness.
- Scope:
  - create separate child epics or tasks for dashboard
  - create separate child epics or tasks for GPU coordination
  - create separate child epics or tasks for adaptive policy
  - keep all three behind production config, helper portability, packaging, and
    cpuset/background planning gates
- Acceptance:
  - each extension item lists prerequisites
  - each extension item lists non-goals
  - each extension item lists required safety evidence
  - each extension item lists required benchmark evidence
  - each extension item lists a verification gate
  - no runtime behavior changes are made
- Verification:
  - `bd lint`
  - docs-only review
