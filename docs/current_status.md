# Current Repository Status

_Last reviewed: 2026-05-10_

This file is the concise factual status snapshot. `bd` remains the task-state
source of truth; `docs/task_list.md` now records the accepted 19-task ledger and
points at the remaining open product gaps.

## Audit Snapshot

The repository has a runnable Rust workspace for the AegisAI Runtime control
loop:

`collector -> classifier -> policy_engine -> actuator -> metrics`

Implemented and accepted capabilities:

- `runtime_daemon` can run the mock control-loop path and the Linux procfs
  preflight path.
- Linux source fallback observes `run_queue_delay`, `cpu_migration`, and
  `major_page_fault` through procfs-derived signals.
- Helper-backed `offcpu_time` and `io_latency` observations have controlled
  workload validation through `aegisai-ebpf-helper`.
- Metadata enrichment supports procfs process name, cmdline, cgroup, parent
  fields, and demo/static metadata.
- Actuator backends include safe `noop`, planning `linux-skeleton`, auditable
  `linux-command-dry-run`, and guarded `linux-command` behind explicit
  confirmation and PID allowlist.
- Live CPU affinity planning is extracted to `agent/actuator/src/cpu_affinity.rs`
  and covered by online/allowed CPU target tests.
- `inference_tail_guard` and `tool_call_booster` both have repeated A/B report
  paths with strict distinction between control evidence and host-level benefit.
- Phase 4 benefit reporting refuses to claim MVP benefit unless live guarded
  actions produce effective host-level changes and repeated stable benefit.

Latest product-evidence status:

- Inference Tail Guard: `FAIL`. Live guarded mode produced effective host-level
  `taskset` changes, but the repeated stable benefit rule was not met.
- Tool Call Booster: `FAIL`. The live guarded run passed contracts and audit
  checks, but did not achieve the configured repeated latency improvement.

## Latest Verified Baseline

Acceptance validation passed:

- `cargo fmt --all -- --check`
- `cargo test --workspace`
- `cargo clippy --all-targets --all-features -- -D warnings`
- `python3 -m unittest discover -s bench/tool_call_booster -p 'test_*.py'`
- `python3 -m unittest discover -s bench/scripts -p 'test_*.py'`
- `for f in bench/scripts/*.sh; do bash -n "$f" || exit 1; done`
- `AEGISAI_VERIFY_LOG=/tmp/aegisai_acceptance_verify_workspace.md bash bench/scripts/verify_workspace.sh`
- `AEGISAI_VERIFY_LOG=/tmp/aegisai_acceptance_toolchain_preflight.md bash bench/scripts/toolchain_preflight.sh`
- `AEGISAI_VERIFY_LOG=/tmp/aegisai_acceptance_inference_preflight.md bash bench/scripts/inference_tail_guard_preflight.sh`

Notes:

- The `/tmp` log override kept this acceptance pass from changing
  `docs/verification_log.md`.
- The Linux source preflight is allowed to process zero live events; it validates
  startup/configuration safety, not real workload benefit.
- The baseline verification above is not a live `ollama` A/B proof; live benefit
  evidence is summarized in `docs/mvp_benefit_report.md` and the artifact index
  below.

## Latest Benefit Artifact Index

### Inference Tail Guard

| run id | artifact | live effective action count | verdict |
| --- | --- | --- | --- |
| `live_guarded_phase4_calibrated_20260510T043859Z` | `.cache/aegisai/inference_tail_guard_phase4/live_guarded_phase4_calibrated_20260510T043859Z/phase4_runs.csv` | `3` | `FAIL`: noisy workload; stable benefit not proven |
| `live_guarded_phase4_calibrated_20260510T043859Z` | `.cache/aegisai/inference_tail_guard_phase4/live_guarded_phase4_calibrated_20260510T043859Z/phase4_aggregate.csv` | `3` | `FAIL`: noisy workload; stable benefit not proven |

`docs/mvp_benefit_report.md` is the current human-readable report for this run.

### Tool Call Booster

| run id | artifact | contract verdict | benefit verdict |
| --- | --- | --- | --- |
| `live_guarded_tcb_issue_94s_final_20260510T053527Z` | `.cache/aegisai/tool_call_booster/live_guarded_tcb_issue_94s_final_20260510T053527Z/tool_call_booster_benefit_report.md` | `PASS` | `FAIL`: `live_guarded` improved `0/3` comparable rounds by at least `5.0%` |
| `live_guarded_tcb_issue_94s_final_20260510T053527Z` | `.cache/aegisai/tool_call_booster/live_guarded_tcb_issue_94s_final_20260510T053527Z/tool_call_booster_summary.csv` | `PASS` | `FAIL` |

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
  `SourceEvent` observations for the target signal.

Both 2026-05-10 `jtt` runs reached `validated signal`.

## Functional Completion

Accepted:

- Core module boundaries and shared contracts.
- Config loading from repo-root example files for local/demo use.
- Awareness/classifier rules for process, cmdline, parent, cgroup, tag markers,
  and PID allowlist.
- Inference Tail Guard policy path with cooldown, bounded action plans, metrics
  traces, benefit gate, and rollback lifecycle.
- Tool Call Booster policy path, real executor lifecycle harness, audit
  continuity, and repeated A/B benefit report.
- Rootless daemon plus narrow helper-backed off-CPU and I/O latency ingestion.
- Guarded Linux command backend with live nice/affinity controls, explicit
  confirmation, PID allowlist, and rollback audit.
- Hot-path tests around actuator rollback, Linux source/procfs edge cases, and
  benefit report interpretation.
- Production config profile and hotspot-split boundaries as design notes.

Still open:

- Proven host-level MVP benefit from effective live guarded actions and stable
  repeated benefit.
- Proven Tool Call Booster guarded latency benefit.
- Production config profiles and schema validation.
- Cross-kernel helper portability beyond the current validated host.
- Real `WarmupExecutor` side effect, if the product requires one.
- Live cpuset/background isolation safety boundary and implementation.
- Production daemon/helper packaging, installer, dashboard, GPU coordination,
  and online adaptive policy learning.

## Open Issue Index

- `AegisAI_Runtime-2kz` — prove or reproducibly falsify Inference Tail Guard MVP
  benefit after the noisy live run.
- `AegisAI_Runtime-79d` — prove or reproducibly falsify Tool Call Booster guarded
  latency benefit.
- `AegisAI_Runtime-cqv` — add production config profiles and schema validation.
- `AegisAI_Runtime-51c` — validate eBPF helper portability across Linux kernels.
- `AegisAI_Runtime-14r` — decide and implement a real `WarmupExecutor` side
  effect, if needed.
- `AegisAI_Runtime-otk` — define live cpuset and background isolation boundary.
- `AegisAI_Runtime-ufp` — package runtime daemon and helper for production
  deployment.
- `AegisAI_Runtime-0ry` — plan deferred dashboard, GPU, and adaptive policy
  extensions.

Use:

```bash
bd ready
bd show <issue-id>
```

## Next Correct Stage

The next major stage is no longer task-list cleanup. It is product evidence:

1. Continue from `live_guarded_phase4_calibrated_20260510T043859Z`: live guarded
   actions are effective, but the current run is classified as `noisy_workload`.
2. Keep the Phase 4 benefit gate strict: effective live action plus stable
   repeated benefit are both required.
3. Continue Tool Call Booster guarded benefit proof from
   `live_guarded_tcb_issue_94s_final_20260510T053527Z`, where contracts passed
   but benefit did not.
4. Keep productionization work behind explicit issues and do not broaden hotspot
   refactors unless they are attached to active behavior work.

## Review Risks

- Large files remain in `agent/runtime_daemon/src/source.rs`,
  `agent/actuator/src/backend.rs`, `agent/explain_tune/src/engine.rs`,
  `agent/runtime_orchestrator/src/runtime_orchestrator.rs`,
  `agent/policy_engine/src/engine.rs`, and
  `bench/scripts/inference_tail_guard_ollama_smoke.sh`; future changes should be
  narrow and test-led.
- `linux-command` can change real process scheduler state. Keep
  `--confirm-live-actuator` and PID allowlist mandatory.
- The current `docs/mvp_benefit_report.md` is intentionally a `FAIL`: it records
  effective live actions, but the stable improvement threshold was not met.
- The Tool Call Booster live guarded report is also intentionally a `FAIL`:
  contracts and audit passed, but repeated latency benefit did not.
