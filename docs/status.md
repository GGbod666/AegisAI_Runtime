# Current Status

_Last reviewed: 2026-05-13_

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
- Controlled Linux source ingestion smoke now validates procfs-derived event
  ingestion with a short-lived PID allowlist and `linux-skeleton`, recording
  `processed_events > 0` plus signal observations without live scheduler
  writes.
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
- Tool Call Booster daemon audit highlights now inline
  `tool_call_stage`, `tool_call_id`, `action_kind`, and `effective` on apply
  detail records so reports can attribute effective scheduler actions to
  executor / retrieval / rerank stages.

Latest product-evidence status:

- Inference Tail Guard: `PASS`. The controlled sample-sizing follow-up kept
  model, prompt, stress shape, concurrency, and live affinity/nice pairing
  fixed, increased samples per mode from `4` to `8`, and produced stable
  live-guarded jitter benefit with effective host-level actions.
- Tool Call Booster: `PASS`. The fixed-work live guarded run passed contracts,
  generated `tool_call_booster_stage_effectiveness.csv`, improved `3/3`
  comparable rounds above the configured `5.0%` latency-improvement threshold
  with average delta `-26.832%`, and recorded executor/retrieval/rerank
  `stage_effectiveness=PASS`.

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

Latest audit refresh on 2026-05-11 also passed:

- `cargo check --workspace`
- `for f in bench/scripts/*.sh; do bash -n "$f" || exit 1; done`
- `AEGISAI_VERIFY_LOG=/tmp/aegisai_audit_verify_workspace_20260511.md bash bench/scripts/verify_workspace.sh`
- `AEGISAI_VERIFY_LOG=/tmp/aegisai_audit_toolchain_preflight_20260511.md bash bench/scripts/toolchain_preflight.sh`
- `AEGISAI_VERIFY_LOG=/tmp/aegisai_audit_inference_preflight_20260511.md bash bench/scripts/inference_tail_guard_preflight.sh`
- `bd lint`
- `bash bench/scripts/linux_source_ingestion_smoke.sh`

Project preflight template replacement on 2026-05-12 also passed:

- `bash bench/scripts/project_preflight.sh`
- `bash bench/scripts/project_preflight.sh --check`
- `bash -n bench/scripts/project_preflight.sh`
- `bd preflight` boundary check: upstream Go/Nix output remains visible and is
  documented as irrelevant to this Rust workspace
- `bd lint`

BpfTracePipe startup failure taxonomy coverage on 2026-05-12 also passed:

- `cargo test -p aegisai-runtime-daemon source::tests` (`32` source tests)
- `cargo fmt --all -- --check`

Helper compatibility preflight classification on 2026-05-12 also passed:

- `cargo test -p aegisai-runtime-daemon` (`80` daemon tests)
- source diagnostics now record kernel version, bpftrace version, tracefs root,
  requested helper tracepoints, and required tracepoint fields before helper
  streams start

Two-kernel helper portability matrix work on 2026-05-12 also passed:

- `cargo test -p aegisai-runtime-daemon source::tests` (`40` source tests)
- `cargo test -p aegisai-ebpf-helper` (`5` helper CLI tests)
- `bash bench/scripts/helper_portability_smoke.sh`: `PASS` on `gg-vm`
  kernel `6.8.0-111-generic`; raw helper streams emitted `624` `offcpu_time`
  events and `12209` `io_latency` events; rootless daemon runs normalized `8`
  events for each signal
- Matrix profile `6.8.0-110-generic` remains backed by the historical
  `2026-05-10T03:37:57Z` and `2026-05-10T03:48:11Z` helper validation entries

Inference smoke artifact regression coverage on 2026-05-13 also passed:

- `bash -n bench/scripts/inference_tail_guard_ollama_smoke.sh`
- `python3 -m unittest bench.scripts.test_inference_tail_guard_ollama_smoke`
  (`2` tests)
- `python3 -m unittest discover -s bench/scripts -p 'test_*.py'` (`17` tests)
- The smoke harness now supports `AEGISAI_AB_RUN_ENV_ONLY=1` for deterministic
  `run.env` / acceptance-baseline provenance checks without launching Ollama,
  stress, or daemon workloads.

Audit caveats:

- Linux source preflight passed with `processed_events=0`; this is a safe
  startup/partial-probe check, not an ingestion or benefit proof.
- Controlled Linux source ingestion smoke passed on 2026-05-12 with
  `processed_events=4` and `run_queue_delay` observations. Hosts that cannot
  expose readable procfs counters or positive controlled-worker deltas return
  `SKIPPED` with exit code `77`.
- Inference preflight intentionally does not run `ollama run`, pull a model, or
  start `stress-ng` load.
- `bd doctor` is unsupported in embedded mode. Upstream `bd preflight` in
  bd `1.0.3` still prints Beads' own Go/Nix template; that output is explicitly
  irrelevant to this Rust workspace. Use
  `bash bench/scripts/project_preflight.sh` for the active project readiness
  checklist and `bash bench/scripts/project_preflight.sh --check` to execute it.

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
| `codex_fixed_work_guarded_final_20260511T141942Z` | `.cache/aegisai/tool_call_booster/codex_fixed_work_guarded_final_20260511T141942Z/tool_call_booster_benefit_report.md` | `PASS` | `PASS`: `live_guarded` improved `3/3` comparable rounds; average delta `-26.832%`, median delta `-26.367%` |
| `codex_fixed_work_guarded_final_20260511T141942Z` | `.cache/aegisai/tool_call_booster/codex_fixed_work_guarded_final_20260511T141942Z/tool_call_booster_summary.csv` | `PASS` | `PASS` |
| `codex_fixed_work_guarded_final_20260511T141942Z` | `.cache/aegisai/tool_call_booster/codex_fixed_work_guarded_final_20260511T141942Z/tool_call_booster_stage_effectiveness.csv` | `PASS` | `PASS`: executor, retrieval, and rerank stages all reported `stage_effectiveness=PASS` |
| `live_guarded_tcb_fixed_work_verified_pass_20260511T135213Z` | `.cache/aegisai/tool_call_booster/live_guarded_tcb_fixed_work_verified_pass_20260511T135213Z/tool_call_booster_summary.csv` | `PASS` | `PASS`: `live_guarded` improved `3/3` comparable rounds; average delta `-21.495%`, median delta `-23.040%` |
| `live_guarded_tcb_fixed_work_verified_pass_20260511T135213Z` | `.cache/aegisai/tool_call_booster/live_guarded_tcb_fixed_work_verified_pass_20260511T135213Z/tool_call_booster_stage_effectiveness.csv` | `PASS` | `PASS`: executor, retrieval, and rerank stages all reported `stage_effectiveness=PASS` |
| `live_guarded_tcb_stage_effectiveness_gate_20260511T132616Z` | `.cache/aegisai/tool_call_booster/live_guarded_tcb_stage_effectiveness_gate_20260511T132616Z/tool_call_booster_stage_effectiveness.csv` | `PASS` | `FAIL`: artifact present; `live_guarded` executor/retrieval/rerank stages all reported `LATENCY_NOT_IMPROVED` |
| `live_guarded_tcb_stage_effectiveness_gate_20260511T132616Z` | `.cache/aegisai/tool_call_booster/live_guarded_tcb_stage_effectiveness_gate_20260511T132616Z/tool_call_booster_summary.csv` | `PASS` | `FAIL`: `live_guarded` improved `0/3` comparable rounds by at least `5.0%` |
| `live_guarded_tcb_stable_executor_20260511T000000Z` | `.cache/aegisai/tool_call_booster/live_guarded_tcb_stable_executor_20260511T000000Z/tool_call_booster_benefit_report.md` | `PASS` | `FAIL`: `live_guarded` improved `0/3` comparable rounds by at least `5.0%`; average delta `1.077%`, median delta `0.200%` |
| `live_guarded_tcb_stable_executor_20260511T000000Z` | `.cache/aegisai/tool_call_booster/live_guarded_tcb_stable_executor_20260511T000000Z/tool_call_booster_summary.csv` | `PASS` | `FAIL` |
| `live_guarded_tcb_issue_94s_final_20260510T053527Z` | `.cache/aegisai/tool_call_booster/live_guarded_tcb_issue_94s_final_20260510T053527Z/tool_call_booster_benefit_report.md` | `PASS` | `FAIL`: `live_guarded` improved `0/3` comparable rounds by at least `5.0%` |
| `live_guarded_tcb_issue_94s_final_20260510T053527Z` | `.cache/aegisai/tool_call_booster/live_guarded_tcb_issue_94s_final_20260510T053527Z/tool_call_booster_summary.csv` | `PASS` | `FAIL` |

Helper validation:

| signal | verification entry | artifact root | result |
| --- | --- | --- | --- |
| `offcpu_time` + `io_latency` portability matrix, `6.8.0-111-generic` | `2026-05-12T14:42:07Z - Two-kernel helper portability matrix` | `.cache/aegisai/helper_portability/helper_portability_gg_vm_6_8_0_111_20260512T141448Z` | `validated signal`; helper compatibility `compatible`; raw streams emitted `624` off-CPU and `12209` I/O events; daemon recorded `8` normalized events for each signal |
| `offcpu_time` | `2026-05-10T03:37:57Z - Helper-backed offcpu_time validation` | `/tmp/aegisai-jtt/artifacts` | helper ready; raw stream attached and emitted `348` events; daemon recorded `8` normalized events |
| `io_latency` | `2026-05-10T03:48:11Z - Helper-backed io_latency validation` | `/tmp/aegisai-jtt/artifacts` | helper ready; block tracepoints exposed required fields; raw stream emitted `4005` events; daemon recorded `8` normalized events |

Future helper conclusions should use these buckets: `helper unavailable`,
`tracepoint incompatible`, `no workload events`, or `validated signal`.
Compatibility diagnostics now separate the first two buckets from zero-event
workloads before portability matrix runs.

## Open Gap Index

- `AegisAI_Runtime-cqv` / `AegisAI_Runtime-cqv.1` /
  `AegisAI_Runtime-cqv.2` / `AegisAI_Runtime-cqv.3` — add production config
  profile selection, schema validation, and cross-file safety checks.
- `AegisAI_Runtime-51c` — parent helper portability epic remains open for
  broader cross-host validation. `AegisAI_Runtime-51c.2` is complete: the
  two-kernel `gg-vm` matrix covers `6.8.0-110-generic` historical helper
  evidence and `6.8.0-111-generic` fresh helper smoke evidence. `AegisAI_Runtime-51c.1`
  is complete: helper compatibility is classified before helper stream start
  and records availability, tracefs, requested probes, and required field
  inventory. `AegisAI_Runtime-51c.3` is complete: controlled Linux ingestion
  smoke records nonzero procfs-derived daemon events. `AegisAI_Runtime-51c.4`
  is complete: BpfTracePipe startup failure taxonomy coverage now distinguishes
  missing binary/helper, permission, stdout/stderr capture, malformed line,
  unsupported signal, and stop cleanup cases.
- `AegisAI_Runtime-8le` — configure the intended Beads Dolt remote sync target;
  `bd dolt remote list` currently reports no remotes configured.
- `AegisAI_Runtime-ufp` / `AegisAI_Runtime-ufp.1` — define and then implement
  daemon/helper packaging boundaries.
- `AegisAI_Runtime-0ry` / `AegisAI_Runtime-0ry.1` — split deferred dashboard,
  GPU, and adaptive policy extensions into evidence-gated future work.

Recently closed:

- `AegisAI_Runtime-fp6` — added deterministic inference smoke `run.env`
  artifact coverage with `AEGISAI_AB_RUN_ENV_ONLY=1`; regression tests validate
  run id, modes, model/prompt/workload shape, stress/sample shape, live flags,
  artifact paths, acceptance baseline references, and failure paths that avoid
  misleading `PASS` fields.
- `AegisAI_Runtime-51c.2` — recorded the helper portability matrix across
  `gg-vm` kernels `6.8.0-110-generic` and `6.8.0-111-generic`; the current
  kernel profile passed `validated signal` with compatible helper diagnostics,
  nonzero raw helper events, and rootless daemon normalized events.
- `AegisAI_Runtime-51c.1` — added helper compatibility diagnostics before helper
  stream start; startup now distinguishes `helper unavailable`,
  `tracepoint incompatible`, and compatible field inventory from later
  zero-event workloads.
- `AegisAI_Runtime-51c.4` — added deterministic BpfTracePipe startup failure
  taxonomy tests for missing binary/helper, permission failure, stdout/stderr
  capture failure, malformed probe lines, unsupported signals, and stop
  cleanup; `cargo test -p aegisai-runtime-daemon source::tests` passed with
  `32` source tests.
- `AegisAI_Runtime-vv2` / `AegisAI_Runtime-vv2.1` — shared policy safety cap
  normalization is complete: generic and scenario policy paths use normalized
  priority delta and affinity ratio caps, invalid caps cannot widen scheduler
  actions, and Tool Call Booster audit behavior is preserved.
- `AegisAI_Runtime-7h5` / `AegisAI_Runtime-7h5.1` — cpuset/background dry-run
  planning is complete for the current safety boundary: deterministic target,
  capture, rollback, and rejection context is test-covered, and live cgroup
  writes remain disabled.
- `AegisAI_Runtime-d42` — expanded runtime daemon CLI parser edge-case coverage
  for duplicate and empty PID allowlists, unknown source/backend values,
  missing verification log paths, and warmup command boundaries; source/backend
  choice validation now fails deterministically in `CliConfig::parse`;
  `cargo test -p aegisai-runtime-daemon` passed with `67` tests.
- `AegisAI_Runtime-yxb` — added direct rollback report builder tests for
  successful and failed nice restore, successful and failed affinity restore,
  mixed report output, missing captured state, and disabled cpuset rollback
  noise suppression; `cargo test -p aegisai-actuator` passed with `51` tests.
- `AegisAI_Runtime-awq` — added `bench/scripts/project_preflight.sh` as the
  project readiness path for Cargo, Python unittest, shell syntax, workspace,
  toolchain, and inference preflight gates; marked upstream `bd preflight`
  Go/Nix output irrelevant to this repository.
- `AegisAI_Runtime-7h5.1` — added the dry-run-only cpuset/background rejection
  matrix for unsafe roots, missing classification, empty CPU sets, missing
  rollback capture, overbroad process sets, and unsupported live write mode;
  `cargo test -p aegisai-actuator` passed with `44` tests.

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
- The latest Tool Call Booster fixed-work guarded report is a scheduler-benefit
  `PASS`; the older stable executor-control `FAIL` remains useful as a
  non-controlled workload boundary, not the current overall Tool Call Booster
  verdict.
