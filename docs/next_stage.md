# Next Stage Plan

_Updated: 2026-05-03_

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

For a compact status and TODO index, see `docs/current_status.md`.

## Next Major Stage: Evidence-Hardened MVP

Primary objective:

Prove or falsify the MVP benefit claim under controlled Linux conditions.

The current `docs/mvp_benefit_report.md` intentionally reports `FAIL`: live
guarded trends were visible, but the latest run recorded no effective host-level
actuator changes. That is the right gate. The project should not claim a runtime
performance win until the live actuator is effective and the repeated A/B report
passes the strict rule.

## Required Work

### 1. Effective Live Inference Tail Guard

Beads issue: `AegisAI_Runtime-s6f`

Goal:

- run `live_guarded` in a controlled Linux window where `renice` or affinity can
  actually change the target process state
- keep PID allowlist and explicit confirmation mandatory
- regenerate the Phase 4 report with the strict effective-action gate

Exit checks:

- at least one live guarded run records effective host-level actuator changes
- Phase 4 report compares baseline, noop observation, dry-run, and live guarded
  modes across repeated rounds
- report verdict is `PASS` only if the strict trend and effective-action rules
  are both satisfied

### 2. Real eBPF Signal Coverage

Beads issue: `AegisAI_Runtime-4nv`

Goal:

- keep the existing source abstraction
- wire real eBPF-backed `offcpu_time` and `io_latency` events behind it
- preserve procfs fallback for `run_queue_delay`, `cpu_migration`, and
  `major_page_fault` while the probe path matures

Exit checks:

- Linux source emits normalized off-CPU and I/O latency `SourceEvent` records
  from controlled workloads
- verification summaries identify attached probes, event counts, and shutdown
  state
- targeted source tests plus workspace verification pass

### 3. Tool Call Booster Benefit Proof

Beads issue: `AegisAI_Runtime-bx1`

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

Beads issue: `AegisAI_Runtime-azv`

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
