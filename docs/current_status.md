# Current Repository Status

_Last reviewed: 2026-05-10_

This file is the concise factual status snapshot. The detailed prioritized work
plan lives in `docs/task_list.md`; the task-state source of truth remains `bd`.

## Audit Snapshot

The repository currently has a runnable Rust workspace for the AegisAI Runtime control loop:

`collector -> classifier -> policy_engine -> actuator -> metrics`

Implemented and verified capabilities:

- `runtime_daemon` can run the mock control-loop path and the Linux procfs preflight path.
- Linux source fallback observes `run_queue_delay`, `cpu_migration`, and `major_page_fault` through procfs-derived signals.
- Metadata enrichment supports procfs process name, cmdline, cgroup, parent fields, and demo/static metadata.
- Actuator backends include safe `noop`, planning `linux-skeleton`, auditable `linux-command-dry-run`, and guarded `linux-command` behind explicit confirmation and PID allowlist.
- `inference_tail_guard` and `tool_call_booster` both trigger in deterministic/mock or harnessed paths.
- Helper-backed `offcpu_time` and `io_latency` observations have controlled
  workload validation through `aegisai-ebpf-helper`; see the helper validation
  artifact index below.
- Phase 4 benefit reporting now refuses to claim MVP benefit unless live guarded
  actions produce effective host-level changes and repeated stable benefit.
- Latest live benefit status: live action is effective, but stable benefit is
  still below the acceptance threshold. `docs/mvp_benefit_report.md` is the
  latest source for this result.

## Latest Verified Baseline

Passed:

- `bash bench/scripts/verify_workspace.sh`
  - `cargo check --workspace`
  - `cargo test --workspace`
  - `cargo fmt --all -- --check`
  - `cargo clippy --all-targets --all-features -- -D warnings`
  - mock daemon smoke
  - Linux source preflight smoke
- `for f in bench/scripts/*.sh; do bash -n "$f" || exit 1; done`
- `bash bench/scripts/toolchain_preflight.sh`
- `bash bench/scripts/inference_tail_guard_preflight.sh`

Notes:

- The latest workspace verification produced `Overall result: PASS` in `docs/verification_log.md`.
- The Linux source preflight is allowed to process zero live events; it validates startup/configuration safety, not real workload benefit.
- The baseline verification above is not a live `ollama` A/B proof; live benefit
  evidence is summarized in `docs/mvp_benefit_report.md`.

## Latest Live Benefit Artifact Index

| run id | CSV | live effective action count | FAIL reason |
| --- | --- | --- | --- |
| `live_affinity_online_fix_phase4_20260503T043809Z` | `.cache/aegisai/inference_tail_guard_phase4/live_affinity_online_fix_phase4_20260503T043809Z/phase4_runs.csv` | `3` | live action is effective, but stable benefit is below threshold |
| `live_affinity_online_fix_phase4_20260503T043809Z` | `.cache/aegisai/inference_tail_guard_phase4/live_affinity_online_fix_phase4_20260503T043809Z/phase4_aggregate.csv` | `3` | live action is effective, but stable benefit is below threshold |

## Helper Validation Artifact Index

Beads issue `AegisAI_Runtime-jtt` is closed. The durable artifact record is in
`docs/verification_log.md`:

| signal | verification entry | artifact root | helper / tracepoint result | daemon result |
| --- | --- | --- | --- | --- |
| `offcpu_time` | `2026-05-10T03:37:57Z - Helper-backed offcpu_time validation` | `/tmp/aegisai-jtt/artifacts` | helper ready; raw stream attached and emitted `348` events; initial bpftrace `str(args->next_comm)` incompatibility was fixed | `8` normalized events, total `165842`, max `21169` |
| `io_latency` | `2026-05-10T03:48:11Z - Helper-backed io_latency validation` | `/tmp/aegisai-jtt/artifacts` | helper ready; block tracepoints expose `dev` and `sector`; raw stream attached and emitted `4005` events | `8` normalized events, total `5013`, max `712` |

Conclusion taxonomy for future helper runs:

- `helper unavailable`: helper readiness command fails, helper is absent, or the
  helper lacks the privileges needed to attach eBPF probes.
- `tracepoint incompatible`: helper is available, but the kernel/bpftrace
  script cannot attach because an expected tracepoint or field is missing or
  has an incompatible type.
- `no workload events`: helper attaches without stderr incompatibility, but the
  controlled workload produces zero raw helper events or zero normalized daemon
  observations before the timeout.
- `validated signal`: helper readiness passes, attach/stream succeeds, raw
  helper events are observed, and the daemon summary records normalized
  `SourceEvent` observations for the target signal. Both 2026-05-10 `jtt`
  runs reached this state.

## Functional Completion

Completed or standing:

- Core module boundaries and shared contracts.
- Config loading from repo-root examples.
- Awareness/classifier rules for process, cmdline, parent, cgroup, tag markers, and PID allowlist.
- Inference Tail Guard policy path with cooldown, bounded action plans, metrics traces, and rollback lifecycle.
- Tool Call Booster policy path and real executor lifecycle harness entrypoint.
- Verification scripts and append-only verification log.
- Helper-backed off-CPU and I/O latency real-signal validation for
  `AegisAI_Runtime-jtt`.

Partially complete:

- eBPF crate has probe descriptors, filters, registry, and event validation.
  Linux runtime source now keeps the main daemon rootless, combines procfs-backed
  sched/fault signals with `aegisai-ebpf-helper` for real `offcpu_time` and
  `io_latency`, falls back cleanly when the helper is unavailable, and has
  controlled-workload helper validation artifacts.
- Live Linux command backend is guarded and auditable. The current benefit
  report records effective host-level `taskset` actions, but the stable benefit
  threshold is still not met.
- Tool Call Booster has lifecycle detection and trigger proof, but not a repeated baseline-vs-guarded benefit report.
- Explain/tune can build reports from metrics, but online tuning remains outside the current scope.

Not complete:

- Proven host-level MVP benefit from effective live guarded actions and stable
  repeated benefit.
- Production daemon packaging/service management.
- Dashboard, GPU coordination, adaptive policy learning, or background isolation.

## Active TODO Index

Detailed task breakdown and dependencies are in `docs/task_list.md`.

Current open product issues:

- `AegisAI_Runtime-lql` — Tune live Inference Tail Guard affinity benefit.
- `AegisAI_Runtime-94s` — Run controlled Tool Call Booster live guarded benefit proof.
- `AegisAI_Runtime-v2y` — Modularize live CPU affinity planning.

Use:

```bash
bd show <issue-id>
bd ready
```

## Next Correct Stage

The next major stage is not more scaffolding. It is evidence hardening:

1. Continue from the latest run indexed above: `live_guarded` already produced effective host-level actuator changes.
2. Keep the Phase 4 benefit gate strict: effective live action plus stable repeated benefit are both required.
3. Promote Tool Call Booster from trigger/harness proof to repeated A/B benefit proof.
4. Add targeted tests around the high-risk hot paths identified by the code graph: actuator rollback reports, Linux command apply/rollback failures, procfs sampling edge cases, runtime source behavior, and benefit report interpretation.

## Review Risks

- Large files remain in `agent/runtime_daemon/src/source.rs`, `agent/actuator/src/backend.rs`, `agent/explain_tune/src/engine.rs`, and `agent/runtime_orchestrator/src/runtime_orchestrator.rs`; future changes should be narrow and test-led.
- `linux-command` can change real process scheduler state. Keep `--confirm-live-actuator` and PID allowlist mandatory.
- The current `docs/mvp_benefit_report.md` is intentionally a `FAIL`: it records
  effective live actions, but the stable improvement threshold was not met.
