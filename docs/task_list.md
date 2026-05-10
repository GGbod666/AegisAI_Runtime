# Evidence-Hardening Acceptance Ledger

_Updated: 2026-05-10_

This file records the acceptance result for the 19 evidence-hardening tasks that
were previously listed here. `bd` remains the task-state source of truth; this
document is a human-readable ledger and a pointer to the remaining product gaps.

## Acceptance Conclusion

The 19 listed tasks are accepted against the current repository state.

System validation run during acceptance:

- `cargo fmt --all -- --check`
- `cargo test --workspace`
- `cargo clippy --all-targets --all-features -- -D warnings`
- `python3 -m unittest discover -s bench/tool_call_booster -p 'test_*.py'`
- `python3 -m unittest discover -s bench/scripts -p 'test_*.py'`
- `for f in bench/scripts/*.sh; do bash -n "$f" || exit 1; done`
- `AEGISAI_VERIFY_LOG=/tmp/aegisai_acceptance_verify_workspace.md bash bench/scripts/verify_workspace.sh`
- `AEGISAI_VERIFY_LOG=/tmp/aegisai_acceptance_toolchain_preflight.md bash bench/scripts/toolchain_preflight.sh`
- `AEGISAI_VERIFY_LOG=/tmp/aegisai_acceptance_inference_preflight.md bash bench/scripts/inference_tail_guard_preflight.sh`

The `AEGISAI_VERIFY_LOG=/tmp/...` override was used so this acceptance pass did
not rewrite or append to `docs/verification_log.md`.

## Current Blocking Shape

No item from the original 19-task list remains open. The remaining project
blockers are product evidence and productionization gaps:

```text
P1 Inference Tail Guard MVP benefit (AegisAI_Runtime-2kz)
  -> strict MVP benefit PASS, or a reproducible FAIL reason beyond the current noisy workload

P2 Tool Call Booster guarded latency benefit (AegisAI_Runtime-79d)
  -> guarded scheduler benefit PASS, or a reproducible FAIL reason

P3/P4 production and extension gaps
  -> config profiles, helper portability, WarmupExecutor side effects,
     live cpuset/background isolation, deployment packaging, dashboard/GPU/adaptive planning
```

The latest `bd list --status=open` output is the authoritative active-work list.

## Accepted Task Ledger

### 1. Align status docs with the latest benefit report

Status: accepted.

Evidence:

- Closed issue: `AegisAI_Runtime-bmz`.
- Status docs now use the same conclusion: effective live action has been
  observed, but stable repeated benefit is not proven.
- This acceptance pass removed stale references that treated closed issues as
  active next work.

### 2. Keep artifact pointers visible

Status: accepted.

Evidence:

- `docs/current_status.md` points to `docs/mvp_benefit_report.md`.
- `docs/mvp_benefit_report.md` records the latest Phase 4 run ID,
  `live_guarded_phase4_calibrated_20260510T043859Z`, verdict, controls, and CSV
  artifact paths.

### 3. Validate helper-backed `offcpu_time`

Status: accepted.

Evidence:

- Closed issue: `AegisAI_Runtime-jtt`.
- `docs/verification_log.md` entry
  `2026-05-10T03:37:57Z - Helper-backed offcpu_time validation` records helper
  readiness, host details, attach command, `348` raw helper events, daemon exit
  `0`, and `8` normalized daemon observations.

### 4. Validate helper-backed `io_latency`

Status: accepted.

Evidence:

- Closed issue: `AegisAI_Runtime-jtt`.
- `docs/verification_log.md` entry
  `2026-05-10T03:48:11Z - Helper-backed io_latency validation` records block
  tracepoint field compatibility, helper readiness, attach command, `4005` raw
  helper events, daemon exit `0`, and `8` normalized daemon observations.

### 5. Record helper validation artifacts

Status: accepted.

Evidence:

- `docs/current_status.md`, `docs/handoff.md`, and `docs/next_stage.md` index
  the helper validation artifacts.
- The conclusion taxonomy remains explicit: `helper unavailable`,
  `tracepoint incompatible`, `no workload events`, and `validated signal`.

### 6. Extract CPU affinity planning

Status: accepted.

Evidence:

- Closed issue: `AegisAI_Runtime-v2y`.
- `agent/actuator/src/cpu_affinity.rs` owns topology discovery, online CPU
  filtering, allowed CPU intersection, target selection, and rollback target
  formatting.
- `agent/actuator/src/backend.rs` uses `CpuAffinityPlanner` instead of keeping
  CPU target planning inline in the backend hot path.

### 7. Test live `taskset` target selection

Status: accepted.

Evidence:

- `cargo test --workspace` passes.
- `cpu_affinity` tests cover `/proc/<pid>/status` `Cpus_allowed_list`,
  `/sys/devices/system/cpu/online` intersections, restricted VM masks, empty
  intersections, reserved/low-contention selection, and deterministic rollback
  targets.

### 8. Preserve the strict Phase 4 gate

Status: accepted.

Evidence:

- Closed issue: `AegisAI_Runtime-v2y`.
- `bench/scripts/test_inference_tail_guard_phase4_report.py` covers noop/dry-run
  improvements, zero effective live actions, priority-limited live actions,
  failed live trends with effective actions, successful live trends, noisy
  workload classification, and insufficient sample size classification.

### 9. Re-run live guarded Phase 4

Status: accepted with `FAIL` result.

Evidence:

- Closed issue: `AegisAI_Runtime-lql`.
- Latest report run: `live_guarded_phase4_calibrated_20260510T043859Z`.
- Modes: `baseline,noop_observation,dry_run,live_guarded`.
- `live_effective_action_count_total=3`; mode contracts passed.
- Verdict remains `FAIL` because the repeated benefit rule was not met.

### 10. Tune one experiment variable at a time

Status: accepted.

Evidence:

- Phase 4 artifacts and report record `changed_variable`.
- Latest run records `affinity_nice_interaction`.
- Failure classification is one of `action_effectiveness`, `noisy_workload`,
  `insufficient_sample_size`, or `no_measurable_benefit`; the latest run is
  `noisy_workload`.

### 11. Update the MVP benefit report from artifacts

Status: accepted.

Evidence:

- `docs/mvp_benefit_report.md` is artifact-backed and records controls, aggregate
  comparison, per-round comparison, stable trend check, failure diagnosis, live
  guarded contract, and CSV paths.
- The report keeps `PASS` restricted to effective live action plus stable
  repeated benefit.

### 12. Run controlled live guarded Tool Call Booster proof

Status: accepted with `FAIL` result.

Evidence:

- Closed issue: `AegisAI_Runtime-94s`.
- Artifact:
  `.cache/aegisai/tool_call_booster/live_guarded_tcb_issue_94s_final_20260510T053527Z/tool_call_booster_benefit_report.md`.
- Overall contract verdict: `PASS`.
- Overall benefit verdict: `FAIL`; `live_guarded` improved `0/3` comparable
  rounds by the configured `5.0%` threshold.

### 13. Verify tool-call audit continuity

Status: accepted.

Evidence:

- Closed issue: `AegisAI_Runtime-94s`.
- Runtime and orchestrator tests cover tool-call lifecycle audit fields,
  rollback trace preservation, duration ratios, action plans, and lifecycle
  summaries.
- `bench/tool_call_booster/test_summarize_ab.py` passes.

### 14. Decide the real `WarmupExecutor` boundary

Status: accepted.

Evidence:

- Closed issue: `AegisAI_Runtime-94s`.
- Current boundary: Tool Call Booster benefit proof covers guarded scheduler
  actions only. `WarmupExecutor` is plan/audit-only; apply records deferred
  warmup and rollback records no-op.
- Follow-up issue for a real side effect: `AegisAI_Runtime-14r`.

### 15. Harden actuator rollback tests

Status: accepted.

Evidence:

- Closed issue: `AegisAI_Runtime-03b`.
- `cargo test --workspace` passes.
- Actuator tests cover apply success with rollback failure, missing capture
  state, stable lease expiration order, live guard behavior, backend audit
  fields, and refreshed lease rollback behavior.

### 16. Harden Linux source and procfs edge tests

Status: accepted.

Evidence:

- Closed issue: `AegisAI_Runtime-2s3`.
- Runtime daemon source tests cover zero-event preflight, partial probe fallback,
  missing procfs fields, process-exit races, helper unavailability, required
  probe failures, and procfs scheduler/fault/migration sampling.
- Linux preflight remains scoped to startup/configuration safety, not benefit
  proof.

### 17. Harden benefit report interpretation tests

Status: accepted.

Evidence:

- Closed issue: `AegisAI_Runtime-n3y`.
- `python3 -m unittest discover -s bench/scripts -p 'test_*.py'` passes.
- Report tests prevent observation-only evidence, dry-run improvements,
  priority-limited actions, and ineffective live actions from producing `PASS`.

### 18. Define production config profile boundaries

Status: accepted as design boundary.

Evidence:

- Closed issue: `AegisAI_Runtime-5bx`.
- `docs/engineering_debt_boundaries.md` records profile selection rules, schema
  validation stages, compatibility defaults, and intentionally deferred areas.
- Implementation follow-up issue: `AegisAI_Runtime-cqv`.

### 19. Split hotspot files only when attached to active work

Status: accepted as process boundary.

Evidence:

- Closed issue: `AegisAI_Runtime-5bx`.
- `docs/engineering_debt_boundaries.md` records that hotspot splits are allowed
  only when attached to active behavior work and covered by targeted
  verification.
- The CPU affinity planning split was attached to `AegisAI_Runtime-v2y` and
  verified by workspace tests.

## Current Gap Index

Open issues:

- `AegisAI_Runtime-2kz`: prove or reproducibly falsify Inference Tail Guard MVP
  benefit after the current noisy live run.
- `AegisAI_Runtime-79d`: prove or reproducibly falsify Tool Call Booster guarded
  latency benefit.
- `AegisAI_Runtime-cqv`: add production config profiles and schema validation.
- `AegisAI_Runtime-51c`: validate helper portability across Linux kernels.
- `AegisAI_Runtime-14r`: decide and implement a real `WarmupExecutor` side
  effect, if the product requires one.
- `AegisAI_Runtime-otk`: define live cpuset and background isolation safety
  boundaries.
- `AegisAI_Runtime-ufp`: package runtime daemon and helper for production
  deployment.
- `AegisAI_Runtime-0ry`: plan deferred dashboard, GPU, and adaptive policy
  extensions.

## Deferred For This Stage

- Dashboard.
- GPU scheduler.
- Online adaptive policy loop.
- Full background isolation.
- Live cpuset cgroup writes beyond guarded experiments.
- Production service packaging and installer.
