# Current Status

_Last reviewed: 2026-05-11_

This is the compact factual snapshot. Active task state lives in `bd`; accepted
task history lives in `docs/acceptance_ledger.md`; stage rules live in
`docs/strategy.md`.

## Snapshot

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
- Live CPU affinity planning lives in `agent/actuator/src/cpu_affinity.rs` and
  is covered by online/allowed CPU target tests.
- `inference_tail_guard` and `tool_call_booster` both have repeated A/B report
  paths with a strict distinction between control evidence and host-level
  benefit.

Latest product-evidence status:

- Inference Tail Guard: `PASS`. The controlled sample-sizing follow-up kept
  model, prompt, stress shape, concurrency, and live affinity/nice pairing
  fixed, increased samples per mode from `4` to `8`, and produced stable
  live-guarded jitter benefit with effective host-level actions.
- Tool Call Booster: `FAIL`. The live guarded run passed contracts and audit
  checks, but did not achieve the configured repeated latency improvement.

## Latest Verification Baseline

The latest accepted baseline passed:

- `cargo fmt --all -- --check`
- `cargo test --workspace`
- `cargo clippy --all-targets --all-features -- -D warnings`
- `python3 -m unittest discover -s bench/tool_call_booster -p 'test_*.py'`
- `python3 -m unittest discover -s bench/scripts -p 'test_*.py'`
- `for f in bench/scripts/*.sh; do bash -n "$f" || exit 1; done`
- `AEGISAI_VERIFY_LOG=/tmp/aegisai_acceptance_verify_workspace.md bash bench/scripts/verify_workspace.sh`
- `AEGISAI_VERIFY_LOG=/tmp/aegisai_acceptance_toolchain_preflight.md bash bench/scripts/toolchain_preflight.sh`
- `AEGISAI_VERIFY_LOG=/tmp/aegisai_acceptance_inference_preflight.md bash bench/scripts/inference_tail_guard_preflight.sh`

The `/tmp` log override kept this pass from appending to
`docs/verification_log.md`.

## Artifact Index

Inference Tail Guard:

| run id | artifact | live effective action count | verdict |
| --- | --- | --- | --- |
| `live_guarded_phase4_sample_sizing_20260511T000000Z` | `.cache/aegisai/inference_tail_guard_phase4/live_guarded_phase4_sample_sizing_20260511T000000Z/phase4_runs.csv` | `3` | `PASS`: live guarded jitter improved `2/3` comparable rounds, mean delta `5.89%` |
| `live_guarded_phase4_sample_sizing_20260511T000000Z` | `.cache/aegisai/inference_tail_guard_phase4/live_guarded_phase4_sample_sizing_20260511T000000Z/phase4_aggregate.csv` | `3` | `PASS`: stable live guarded benefit with effective actions |
| `live_guarded_phase4_calibrated_20260510T043859Z` | `.cache/aegisai/inference_tail_guard_phase4/live_guarded_phase4_calibrated_20260510T043859Z/phase4_runs.csv` | `3` | `FAIL`: noisy workload; stable benefit not proven |
| `live_guarded_phase4_calibrated_20260510T043859Z` | `.cache/aegisai/inference_tail_guard_phase4/live_guarded_phase4_calibrated_20260510T043859Z/phase4_aggregate.csv` | `3` | `FAIL`: noisy workload; stable benefit not proven |

`docs/mvp_benefit_report.md` is the human-readable report for this run.

Tool Call Booster:

| run id | artifact | contract verdict | benefit verdict |
| --- | --- | --- | --- |
| `live_guarded_tcb_issue_94s_final_20260510T053527Z` | `.cache/aegisai/tool_call_booster/live_guarded_tcb_issue_94s_final_20260510T053527Z/tool_call_booster_benefit_report.md` | `PASS` | `FAIL`: `live_guarded` improved `0/3` comparable rounds by at least `5.0%` |
| `live_guarded_tcb_issue_94s_final_20260510T053527Z` | `.cache/aegisai/tool_call_booster/live_guarded_tcb_issue_94s_final_20260510T053527Z/tool_call_booster_summary.csv` | `PASS` | `FAIL` |

Helper validation:

| signal | verification entry | artifact root | result |
| --- | --- | --- | --- |
| `offcpu_time` | `2026-05-10T03:37:57Z - Helper-backed offcpu_time validation` | `/tmp/aegisai-jtt/artifacts` | helper ready; raw stream attached and emitted `348` events; daemon recorded `8` normalized events |
| `io_latency` | `2026-05-10T03:48:11Z - Helper-backed io_latency validation` | `/tmp/aegisai-jtt/artifacts` | helper ready; block tracepoints exposed required fields; raw stream emitted `4005` events; daemon recorded `8` normalized events |

Future helper conclusions should use these buckets: `helper unavailable`,
`tracepoint incompatible`, `no workload events`, or `validated signal`.

## Open Gap Index

- `AegisAI_Runtime-79d` — prove or reproducibly falsify Tool Call Booster guarded
  latency benefit.
- `AegisAI_Runtime-cqv` — add production config profiles and schema validation.
- `AegisAI_Runtime-51c` — validate eBPF helper portability across Linux kernels.
- `AegisAI_Runtime-14r` — decide and implement a real `WarmupExecutor` side
  effect, if the product requires one.
- `AegisAI_Runtime-7h5` — add a cpuset/background dry-run planner after the
  live isolation safety boundary.
- `AegisAI_Runtime-ufp` — package runtime daemon and helper for production
  deployment.
- `AegisAI_Runtime-0ry` — plan deferred dashboard, GPU, and adaptive policy
  extensions.

Use:

```bash
bd ready
bd show <issue-id>
```

## Restart Context

Start a future session by reading `docs/status.md`, `docs/strategy.md`, the
latest `docs/mvp_benefit_report.md`, and relevant append-only entries in
`docs/verification_log.md`. Preserve the strict benefit gates: effective live
action and stable repeated benefit are both required.

## Review Risks

- Large files remain in `agent/runtime_daemon/src/source.rs`,
  `agent/actuator/src/backend.rs`, `agent/explain_tune/src/engine.rs`,
  `agent/runtime_orchestrator/src/runtime_orchestrator.rs`,
  `agent/policy_engine/src/engine.rs`, and
  `bench/scripts/inference_tail_guard_ollama_smoke.sh`; future changes should be
  narrow and test-led.
- `linux-command` can change real process scheduler state. Keep
  `--confirm-live-actuator` and PID allowlist mandatory.
- `docs/mvp_benefit_report.md` is a generated `PASS` from a live guarded run;
  keep `PASS` restricted to effective live action plus stable repeated benefit.
- The Tool Call Booster live guarded report is also intentionally a `FAIL`:
  contracts and audit passed, but repeated latency benefit did not.
