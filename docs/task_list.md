# Evidence-Hardening Task List

_Updated: 2026-05-10_

This task list is derived from the current README, `bd ready`, and the latest
MVP benefit report. `bd` remains the task-state source of truth; this file
spells out dependencies, priority, and concrete acceptance checks.

## Current Blocking Shape

The repository has a runnable control loop and guarded live actuator path, but
the latest MVP benefit report still returns `FAIL`: live guarded mode produced
effective host-level `taskset` actions, while the stable tail-latency benefit
threshold was not met. Helper-backed off-CPU and I/O real-signal validation is
complete and indexed in `docs/current_status.md` and `docs/verification_log.md`.

Semantic dependencies:

```text
P0 status/doc consistency
  -> P2 Tool Call Booster I/O/retrieval evidence is more credible

P1 live CPU affinity planning reliability (AegisAI_Runtime-v2y)
  -> P1/P2 Inference Tail Guard live benefit tuning (AegisAI_Runtime-lql)
      -> MVP benefit PASS, or an explicit reproducible FAIL reason

P1 live actuator reliability
  -> P2 Tool Call Booster live guarded benefit proof (AegisAI_Runtime-94s)

P2 hot-path tests
  -> supports every live/benchmark change above
```

`bd blocked` currently reports no hard blockers. The dependencies above are
engineering order, not beads blocker records.

## P0: Status Consistency

### 1. Align status docs with the latest benefit report

Problem:

- Some docs still describe the latest Phase 4 result as having no effective live
  actuator action.
- The current README and report say live guarded actions were effective, but the
  stable improvement threshold was not met.

Acceptance:

- `current_status.md`, `next_stage.md`, `handoff.md`, `roadmap.md`, and `mvp.md`
  all use the same conclusion: effective live action observed, stable benefit
  not proven.
- No active task list points at closed beads issues as the next work item.

### 2. Keep artifact pointers visible

Problem:

- The latest report has CSV artifact paths, but the next operator should not
  have to search logs to find the relevant run.

Acceptance:

- `current_status.md` points to `docs/mvp_benefit_report.md`.
- `mvp_benefit_report.md` keeps the run ID, verdict, and artifact paths.

## P1: Real Signal Evidence - Completed

### 3. Validate helper-backed `offcpu_time`

Beads issue: `AegisAI_Runtime-jtt` (closed)

Work:

- Install or expose `aegisai-ebpf-helper` on a Linux validation host.
- Prepare a controlled off-CPU workload.
- Run the rootless daemon with Linux source and helper-backed probes.
- Capture daemon summary, helper readiness, attach status, event count, and
  shutdown status.

Acceptance:

- Normalized `offcpu_time` `SourceEvent` observations appear in daemon output.
- If attachment fails, the failure records the exact helper/bpftrace/kernel
  compatibility reason.
- Result: `docs/verification_log.md` entry
  `2026-05-10T03:37:57Z - Helper-backed offcpu_time validation` records
  helper readiness, host details, the off-CPU attach command, `348` raw helper
  events, daemon exit `0`, and `8` normalized daemon observations.

### 4. Validate helper-backed `io_latency`

Beads issue: `AegisAI_Runtime-jtt` (closed)

Work:

- Prepare a controlled block I/O workload.
- Confirm host block tracepoint fields used by bpftrace are compatible.
- Run daemon/helper ingestion and record emitted I/O observations.

Acceptance:

- Normalized `io_latency` `SourceEvent` observations appear in daemon output.
- If the kernel tracepoint layout is incompatible, the report records the
  tracepoint and field that failed.
- Result: `docs/verification_log.md` entry
  `2026-05-10T03:48:11Z - Helper-backed io_latency validation` records block
  tracepoint field compatibility, helper readiness, the I/O attach command,
  `4005` raw helper events, daemon exit `0`, and `8` normalized daemon
  observations.

### 5. Record helper validation artifacts

Beads issue: `AegisAI_Runtime-jtt` (closed)

Work:

- Preserve commands, host details, helper readiness, attach result, event
  counts, and partial-probe behavior.

Acceptance:

- The result is referenced from the relevant report or status doc.
- The conclusion distinguishes "helper unavailable", "tracepoint incompatible",
  "no workload events", and "validated signal".
- Result: `docs/current_status.md` now references the `jtt` artifact records and
  defines the conclusion taxonomy. The completed 2026-05-10 runs are
  `validated signal`; helper absence, tracepoint mismatch, and zero-event runs
  remain distinct failure buckets for future validations.

## P1: Live Affinity Reliability

### 6. Extract CPU affinity planning

Beads issue: `AegisAI_Runtime-v2y`

Work:

- Move topology discovery, online CPU filtering, allowed CPU intersection,
  reserved-core selection, low-contention selection, and rollback-target
  planning out of the actuator backend hot file.

Acceptance:

- A dedicated planner module covers configured CPU vs online CPU mismatch.
- Inference Tail Guard live affinity uses the planner without changing the
  Phase 4 benefit gate.

### 7. Test live `taskset` target selection

Beads issue: `AegisAI_Runtime-v2y`

Work:

- Add tests for `/proc/<pid>/status` `Cpus_allowed_list` and
  `/sys/devices/system/cpu/online` intersections.
- Cover empty intersections and restricted VM CPU masks.

Acceptance:

- Tests prove the planner does not select offline or disallowed CPUs.
- Rollback target generation is deterministic.

### 8. Preserve the strict Phase 4 gate

Beads issue: `AegisAI_Runtime-v2y`

Work:

- Keep `live_effective_action_count > 0` and stable trend checks as separate
  PASS requirements.

Acceptance:

- Refactoring affinity planning does not let noop, dry-run, or ineffective live
  actions pass as MVP benefit.

## P1/P2: Inference Tail Guard Benefit Proof

### 9. Re-run live guarded Phase 4

Beads issue: `AegisAI_Runtime-lql`

Work:

- Use explicit live actuator confirmation and PID allowlist.
- Include baseline, noop observation, dry-run, and live guarded modes.
- Keep the model, concurrency, sample count, and interference shape recorded.

Acceptance:

- Mode contracts pass.
- `live_effective_action_count > 0`.
- The report gives a PASS only if the stable trend rule is met.

### 10. Tune one experiment variable at a time

Beads issue: `AegisAI_Runtime-lql`

Work:

- Evaluate CPU selection, stress shape, sample sizing, model/runtime behavior,
  and affinity/nice interaction independently.

Acceptance:

- Every run records exactly which variable changed.
- A failed result identifies whether the cause is action effectiveness, noisy
  workload, insufficient sample size, or no measurable benefit.

### 11. Update the MVP benefit report from artifacts

Beads issue: `AegisAI_Runtime-lql`

Work:

- Regenerate or update `docs/mvp_benefit_report.md` only from real artifacts.

Acceptance:

- PASS means effective live guarded action plus stable repeated benefit.
- FAIL states the specific reason without treating noop/dry-run as host benefit.

## P2: Tool Call Booster Benefit Proof

### 12. Run controlled live guarded Tool Call Booster proof

Beads issue: `AegisAI_Runtime-94s`

Work:

- Run executor startup, retrieval, rerank, and background-interference samples.
- Compare baseline, noop observation, dry-run, and live guarded modes where
  available.

Acceptance:

- Report includes latency deltas, trigger counts, rollback counts, action
  errors, and explicit PASS/FAIL verdict.

### 13. Verify tool-call audit continuity

Beads issue: `AegisAI_Runtime-94s`

Work:

- Check `tool_call_id`, stage label, duration ratio, action plan, metric trace,
  and lifecycle summary across executor/retrieval/rerank.

Acceptance:

- Each stage can be traced by `tool_call_id` from trigger to rollback.

### 14. Decide the real `WarmupExecutor` boundary

Beads issue: `AegisAI_Runtime-94s`

Work:

- Decide whether the current benefit proof is limited to nice/affinity, or
  whether a real executor warmup side effect is required.

Acceptance:

- Reports do not imply executor warmup is live if it remains plan/audit-only.

## P2: Hot-Path Test Hardening

### 15. Harden actuator rollback tests

Work:

- Cover apply success with rollback failure, missing capture state, lease
  expiration order, and backend audit fields.

Acceptance:

- `cargo test --workspace` passes.
- Failure traces explain why rollback was skipped or failed.

### 16. Harden Linux source and procfs edge tests

Work:

- Cover zero-event preflight, partial probe fallback, missing procfs fields, and
  process-exit races.

Acceptance:

- Linux preflight remains clearly scoped to startup/configuration safety, not
  benefit proof.

### 17. Harden benefit report interpretation tests

Work:

- Cover live action count zero, effective live action with failed trend,
  noop/dry-run improvements, and priority-limited actions.

Acceptance:

- The report cannot incorrectly produce PASS from observation-only evidence.

## P3: Engineering Debt

### 18. Define production config profile boundaries

Problem:

- Runtime config currently reads fixed `configs/*/*.example.toml` paths through
  a minimal TOML subset parser.

Acceptance:

- A design note exists for profile selection, schema validation, and what is
  intentionally deferred.

### 19. Split hotspot files only when attached to active work

Problem:

- Hot files include `agent/runtime_daemon/src/source.rs`,
  `agent/actuator/src/backend.rs`, `agent/explain_tune/src/engine.rs`,
  `agent/runtime_orchestrator/src/runtime_orchestrator.rs`,
  `agent/policy_engine/src/engine.rs`, and
  `bench/scripts/inference_tail_guard_ollama_smoke.sh`.

Acceptance:

- Refactors are small, behavior-preserving, and covered by targeted tests.

## Deferred For This Stage

- Production service packaging and installer.
- Dashboard.
- GPU scheduler.
- Online adaptive policy loop.
- Full background isolation.
- Live cpuset cgroup writes beyond guarded experiments.
