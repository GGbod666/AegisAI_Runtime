# Next Stage Plan

_Updated: 2026-05-10_

## Current Conclusion

The repository is past the original "make the runtime loop runnable" stage.
The current workspace has a verified control-loop mainline:

`collector -> classifier -> policy_engine -> actuator -> metrics`

The latest audit passed:

- `bash bench/scripts/verify_workspace.sh`
- `bash bench/scripts/toolchain_preflight.sh`
- `bash bench/scripts/inference_tail_guard_preflight.sh`
- shell syntax checks for every script in `bench/scripts/*.sh`

The correct next phase is therefore not additional scaffolding. It is evidence
hardening: prove that the guarded Linux actuator can create effective host-level
changes and that those changes produce repeatable latency benefit.

For compact status, see `docs/current_status.md`. For detailed tasks and
dependencies, see `docs/task_list.md`.

## Next Major Stage: Evidence-Hardened MVP

Primary objective:

Prove or falsify the MVP benefit claim under controlled Linux conditions.

The current `docs/mvp_benefit_report.md` intentionally reports `FAIL`: live
guarded mode recorded effective host-level `taskset` actions, but the repeated
A/B result did not meet the stable improvement threshold. That is the right
gate. The project should not claim a runtime performance win until both live
action effectiveness and repeated benefit are proven.

## Required Work

### Completed: Real eBPF Signal Validation

Beads issue: `AegisAI_Runtime-jtt`

Result:

- helper-backed `offcpu_time` and `io_latency` observations were validated with
  controlled workloads on 2026-05-10
- the main daemon remained rootless and used `aegisai-ebpf-helper` as the narrow
  privileged boundary
- helper readiness, commands, host details, tracepoint compatibility, raw event
  counts, daemon event counts, and partial-probe behavior are recorded in
  `docs/verification_log.md`

Artifact references:

- `2026-05-10T03:37:57Z - Helper-backed offcpu_time validation`: `348` raw
  helper events and `8` normalized daemon events.
- `2026-05-10T03:48:11Z - Helper-backed io_latency validation`: tracepoint
  fields compatible, `4005` raw helper events, and `8` normalized daemon events.
- Future helper conclusions should distinguish `helper unavailable`,
  `tracepoint incompatible`, `no workload events`, and `validated signal`; both
  completed runs are `validated signal`.

### 1. Live CPU Affinity Reliability

Beads issue: `AegisAI_Runtime-v2y`

Goal:

- extract CPU topology discovery, online CPU filtering, allowed CPU
  intersection, and target selection out of the actuator backend hot file
- keep rollback semantics explicit
- preserve the strict Phase 4 benefit gate

Exit checks:

- dedicated planner tests cover configured vs online CPU mismatch and effective
  `taskset` targets
- Inference Tail Guard live affinity uses the planner without weakening report
  rules

### 2. Effective Live Inference Tail Guard

Beads issue: `AegisAI_Runtime-lql`

Goal:

- tune strategy parameters, CPU selection, stress shape, sample sizing, and
  runtime behavior
- keep PID allowlist and explicit confirmation mandatory
- regenerate the Phase 4 report from real artifacts

Exit checks:

- live guarded mode records effective host-level actuator changes
- Phase 4 report compares baseline, noop observation, dry-run, and live guarded
  modes across repeated rounds
- report verdict is `PASS` only if the strict trend and effective-action rules
  are both satisfied

### 3. Tool Call Booster Benefit Proof

Beads issue: `AegisAI_Runtime-94s`

Goal:

- promote the current real executor lifecycle harness from trigger proof to
  repeated A/B benefit proof
- compare baseline, noop observation, dry-run, and any guarded mode with
  comparable latency metrics

Exit checks:

- repeated tool-call benchmark records executor, retrieval, rerank, and
  background samples
- report includes latency deltas, trigger counts, rollback counts, and an
  explicit PASS/FAIL verdict
- dry-run and noop are treated as closed-loop evidence, not host benefit proof

### 4. Hot-Path Test Hardening

Goal:

- add narrow tests around the riskiest high-degree paths identified by the code
  graph
- avoid broad refactors while the runtime evidence gates are still moving

Priority areas:

- actuator rollback reports and missing capture state
- Linux command apply/rollback failures
- procfs sampling edge cases
- runtime source startup/poll/shutdown behavior
- benefit report interpretation when live actions are no-op

Exit checks:

- targeted tests cover the listed areas
- `cargo test --workspace` and clippy pass

## Explicit Non-Goals For This Stage

- dashboard work
- GPU coordination
- online adaptive policy learning
- production service packaging
- cpuset/background throttling beyond guarded experiments
- broad module decomposition not tied to a failing test or active evidence gate

## Recommended Command Sequence

Safe reconfirmation:

```bash
bash bench/scripts/verify_workspace.sh
bash bench/scripts/toolchain_preflight.sh
bash bench/scripts/inference_tail_guard_preflight.sh
```

Effective live benefit proof, only inside an approved experiment window:

```bash
AEGISAI_AB_MODES=baseline,noop_observation,dry_run,live_guarded \
AEGISAI_CONFIRM_LIVE_ACTUATOR=1 \
AEGISAI_LIVE_PID_ALLOWLIST=<pid,...> \
  bash bench/scripts/inference_tail_guard_phase4_report.sh
```

Tool Call Booster trigger/harness reconfirmation:

```bash
bash bench/scripts/tool_call_booster_real_executor_harness.sh
```

## Stage Exit

This stage is complete only when the project can say one of the following with
evidence:

- MVP benefit proven: effective live guarded actions were observed and repeated
  tail-latency metrics passed the strict Phase 4 gate.
- MVP benefit not proven yet: the runtime closed loop works, but the live action
  or benefit trend still fails, with artifacts explaining why.

Either outcome is acceptable if the report is honest and reproducible.
