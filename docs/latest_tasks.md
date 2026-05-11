# Latest Task List

_Generated: 2026-05-11_

This file is synchronized from the latest repository audit. `bd` remains the
source of truth for status and ownership; every task below has a concrete bead
ID, bounded scope, acceptance criteria, and verification command or evidence
gate.

## Current Audit Evidence

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
  `PASS`
- `AEGISAI_VERIFY_LOG=/tmp/aegisai_audit_toolchain_preflight_20260511.md bash bench/scripts/toolchain_preflight.sh`:
  `PASS`
- `AEGISAI_VERIFY_LOG=/tmp/aegisai_audit_inference_preflight_20260511.md bash bench/scripts/inference_tail_guard_preflight.sh`:
  `PASS`
- `bd lint`: `PASS`

Audit limits:

- The Linux source smoke in `verify_workspace.sh` passed with
  `processed_events=0`; it proves startup and partial-probe degradation, not
  real Linux ingestion or benefit.
- The inference preflight does not run `ollama run`, pull a model, or start
  `stress-ng` load.
- `bd doctor` is unsupported in embedded mode, and `bd preflight` still prints a
  Go/Nix checklist that is not this project's quality gate.
- code-review-graph reported `1286` nodes, `10276` edges, `20` untested
  hotspots, and `28` files/classes with at least `300` lines.

## Execution Order

1. Fix project process/tooling accuracy so future readiness checks are not
   misleading.
2. Close direct test gaps around live-action rollback, CLI parsing, helper
   startup, and benchmark artifact writing.
3. Normalize policy safety semantics before production config can enable more
   paths.
4. Split production config into selector, schema, and cross-file safety checks.
5. Prove Linux helper/procfs portability with structured host evidence.
6. Add cpuset/background dry-run rejection behavior before any live cgroup work.
7. Define packaging boundaries only after config and privilege boundaries are
   explicit.
8. Split deferred extensions into evidence-gated epics without runtime changes.

## T1. Replace Project Preflight Template

- Issue: `AegisAI_Runtime-awq`
- Problem: `bd preflight` still prints Go/Nix commands, which can mislead
  release readiness for this Rust workspace.
- Scope:
  - replace or clearly override the generic checklist with this repo's gates
  - document the intended preflight command sequence in the same place developers
    will actually see it
  - keep `bd lint` clean after the change
- Acceptance:
  - readiness instructions include `cargo fmt --all -- --check`
  - readiness instructions include `cargo test --workspace`
  - readiness instructions include
    `cargo clippy --all-targets --all-features -- -D warnings`
  - readiness instructions include both Python unittest discovery commands
  - readiness instructions include shell syntax checks and the three bench
    preflight scripts
  - no Go/Nix gate appears as an active requirement for this repository
- Verification:
  - `bd preflight`
  - `bd lint`

## T2. Directly Test Linux Rollback Report Builder

- Issue: `AegisAI_Runtime-yxb`
- Problem: code-review-graph flags
  `agent/actuator/src/backend.rs::build_linux_rollback_report` as a high-degree
  hotspot with no direct test mapping.
- Scope:
  - add focused tests around rollback audit composition
  - avoid broad backend refactors
  - preserve existing backend field names and report text
- Acceptance:
  - tests cover successful nice rollback
  - tests cover failed nice rollback
  - tests cover successful affinity rollback
  - tests cover failed affinity rollback
  - tests cover mixed nice/affinity action reports
  - tests cover missing captured state
  - tests prove disabled cpuset actions do not emit rollback noise
- Verification:
  - `cargo test -p aegisai-actuator`

## T3. Expand CLI Parser Edge-Case Tests

- Issue: `AegisAI_Runtime-d42`
- Problem: `CliConfig::parse` is a high-degree hotspot. Existing tests cover
  common live flags, but edge cases need to be explicit before adding production
  profile flags.
- Scope:
  - add parser tests only; do not redesign the CLI in this task
  - cover live actuator and warmup command boundary conditions
  - include production profile interactions if that flag lands first
- Acceptance:
  - duplicate `--live-pid-allowlist` behavior is deterministic
  - whitespace and empty PID elements are rejected or normalized by test-covered
    rule
  - unknown source/backend names produce deterministic errors
  - `--verification-log` missing value is rejected
  - warmup command argument boundaries are covered
  - production profile flag interactions are covered if available
- Verification:
  - `cargo test -p aegisai-runtime-daemon`

## T4. Add BpfTracePipe Startup Failure Taxonomy Tests

- Issue: `AegisAI_Runtime-51c.4`
- Parent: `AegisAI_Runtime-51c`
- Problem: `BpfTracePipe::start` is a helper startup hotspot; helper failure
  modes need stable classification before portability work.
- Scope:
  - add focused runtime daemon source tests
  - distinguish startup failures from malformed event parsing
  - keep partial-probe reporting messages stable
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

## T5. Add Inference Smoke Artifact Tests

- Issue: `AegisAI_Runtime-fp6`
- Problem: `bench/scripts/inference_tail_guard_ollama_smoke.sh` is a large
  benefit-artifact driver; `write_run_env` and `run_mode` are graph hotspots.
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

## T6. Normalize Generic Safety Caps

- Issue: `AegisAI_Runtime-vv2.1`
- Parent: `AegisAI_Runtime-vv2`
- Problem: Tool Call Booster locally normalizes bad safety caps, but generic and
  non-TCB policy paths can still rely on raw `SafetyConfig` values.
- Scope:
  - implement shared normalization for priority delta caps
  - implement shared normalization for affinity ratio caps
  - keep Tool Call Booster audit fields and pass/fail behavior unchanged
- Acceptance:
  - negative `max_priority_delta` cannot widen scheduler actions
  - zero, non-finite, or invalid affinity ratios cannot widen scheduler actions
  - valid caps still produce expected action plans
  - existing TCB tests continue to pass
- Verification:
  - `cargo test -p aegisai-policy-engine`

## T7. Add Runtime Production Profile Selector

- Issue: `AegisAI_Runtime-cqv.2`
- Parent: `AegisAI_Runtime-cqv`
- Problem: runtime startup still loads fixed `configs/*/*.example.toml` files.
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

## T8. Add Production Config Schema Validation

- Issue: `AegisAI_Runtime-cqv.3`
- Parent: `AegisAI_Runtime-cqv`
- Problem: the current parser accepts the example shape but does not enforce a
  production schema with actionable errors.
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

## T9. Add Config Cross-File Safety Validation

- Issue: `AegisAI_Runtime-cqv.1`
- Parent: `AegisAI_Runtime-cqv`
- Problem: scenario actions, source focus signals, and global safety caps are
  not rejected together before startup.
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

## T10. Classify Helper Compatibility Before Daemon Start

- Issue: `AegisAI_Runtime-51c.1`
- Parent: `AegisAI_Runtime-51c`
- Problem: helper-backed offcpu/io paths need distinct failure buckets before
  daemon startup.
- Scope:
  - inspect helper availability
  - inspect tracefs root and tracepoint field inventory
  - classify compatibility before running a long daemon loop
  - keep procfs-backed fallback available under `--allow-partial-probes`
- Acceptance:
  - helper unavailable and tracepoint incompatible are distinct results
  - missing block tracepoint fields name the missing field
  - compatible inventory is recorded
  - no workload events remains separate from compatibility failure
- Verification:
  - `cargo test -p aegisai-runtime-daemon`
  - helper preflight command from `docs/linux_validation.md`

## T11. Run Two-Kernel Helper Portability Matrix

- Issue: `AegisAI_Runtime-51c.2`
- Parent: `AegisAI_Runtime-51c`
- Problem: durable helper validation currently covers one host profile.
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

## T12. Add Controlled Linux Source Ingestion Smoke

- Issue: `AegisAI_Runtime-51c.3`
- Parent: `AegisAI_Runtime-51c`
- Problem: current Linux source preflight can pass with `processed_events=0`.
- Scope:
  - create or select a short-lived allowlisted process
  - use `linux-skeleton` or `linux-command-dry-run`
  - prove at least one procfs-derived signal reaches daemon summary
  - document skip conditions
- Acceptance:
  - smoke records `processed_events > 0`
  - summary includes at least one `signal_observations` entry from
    `run_queue_delay`, `cpu_migration`, or `major_page_fault`
  - no live scheduler state is changed
  - skip reason is explicit when host procfs data is unavailable
  - command is documented in `docs/linux_validation.md`
- Verification:
  - new Linux ingestion smoke command
  - `cargo test -p aegisai-runtime-daemon` if helper/source code changes

## T13. Implement Cpuset Dry-Run Rejection Matrix

- Issue: `AegisAI_Runtime-7h5.1`
- Parent: `AegisAI_Runtime-7h5`
- Problem: live cpuset/background writes are disabled, but no deterministic
  dry-run rejection matrix exists.
- Scope:
  - generate rejection reasons without writing cgroupfs
  - include target pid/cgroup context
  - include capture and rollback plan context where available
- Acceptance:
  - unsafe cgroup root is rejected
  - missing classification is rejected
  - empty computed CPU set is rejected
  - missing rollback capture is rejected
  - overbroad process set is rejected
  - unsupported live write mode is rejected
  - live writes remain disabled
- Verification:
  - `cargo test -p aegisai-actuator`

## T14. Define Debian/Systemd Packaging Contract

- Issue: `AegisAI_Runtime-ufp.1`
- Parent: `AegisAI_Runtime-ufp`
- Problem: packaging work needs a precise first target before installer code.
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

## T15. Split Deferred Extensions Into Evidence-Gated Epics

- Issue: `AegisAI_Runtime-0ry.1`
- Parent: `AegisAI_Runtime-0ry`
- Problem: dashboard, GPU coordination, and adaptive policy remain one broad
  deferred bucket.
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
