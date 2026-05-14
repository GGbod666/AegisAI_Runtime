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
- Runtime startup now supports production config profile selection with
  precedence `--config-profile`, `AEGISAI_CONFIG_PROFILE`, then the local demo
  default. Named profiles are identifier-only and load non-example files from
  `configs/profiles/<name>/`, while the local demo path preserves the existing
  `*.example.toml` files.
- Named production config profiles now run strict schema validation for runtime,
  awareness, safety, and scenario policy files. Errors identify the selected
  profile, file, section, key, and violated constraint; the local demo path
  remains compatible with the existing example config shape.
- Named production config profiles now run cross-file safety validation after
  schema parsing. Enabled scenarios must stay within global duration and
  priority caps, triggers must be backed by `focus_signals`, live affinity must
  use a non-empty PID allowlist profile scope, and live cpuset writes remain
  disabled by profile validation. Cross-file errors name both involved files.
- The Debian/Ubuntu systemd packaging contract is defined in
  `docs/packaging_contract.md`. The first package target keeps
  `aegisai-runtime-daemon` rootless under `_aegisai`, installs
  `aegisai-ebpf-helper` as a separate helper boundary, uses
  `/etc/aegisai/configs/profiles/production/` for the selected production
  profile, and leaves installer implementation to `AegisAI_Runtime-ufp`.

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
  `2026-05-10T03:37:57Z` and `2026-05-10T03:48:11Z` helper validation entries;
  the profile records kernel `6.8.0-110-generic` and distro
  `Ubuntu 24.04.4 LTS`
- Strict P2 smoke bucket audit fix on 2026-05-13 passed:
  `python3 -m unittest bench.scripts.test_helper_portability_smoke` (`4`
  tests), full bench script unittest discovery (`21` tests), shell syntax,
  `git diff --check`, and the original missing-bpftrace reproduction. The
  reproduction now exits `1`, records final bucket `helper unavailable`, and
  writes `Overall result: FAIL` at
  `/tmp/aegisai_audit_helper_unavailable_fixed_final2/helper_portability.md`.

Inference smoke artifact regression coverage on 2026-05-13 also passed:

- `bash -n bench/scripts/inference_tail_guard_ollama_smoke.sh`
- `python3 -m unittest bench.scripts.test_inference_tail_guard_ollama_smoke`
  (`2` tests)
- `python3 -m unittest discover -s bench/scripts -p 'test_*.py'` (`17` tests)
- The smoke harness now supports `AEGISAI_AB_RUN_ENV_ONLY=1` for deterministic
  `run.env` / acceptance-baseline provenance checks without launching Ollama,
  stress, or daemon workloads.

Runtime production profile selector coverage on 2026-05-13 also passed:

- `cargo fmt --all -- --check`
- `cargo test -p runtime_orchestrator` (`14` tests)
- `cargo test -p aegisai-runtime-daemon` (`87` tests)
- The selected-profile loader rejects empty, path-like, and dotted names before
  startup; missing profile roots fail before file reads; daemon CLI/env/default
  precedence is covered.

Production config schema validation on 2026-05-13 also passed:

- `cargo fmt --all -- --check`
- `cargo test -p runtime_orchestrator` (`23` tests)
- `cargo test -p aegisai-runtime-daemon` (`87` tests)
- Named production profiles reject unknown keys, missing required fields,
  invalid classifier rule keys, invalid focus signals, invalid scenario names,
  invalid pin strategies, out-of-range `raise_nice`, and zero durations with
  profile/file/section/key constraint context. The local demo example path
  remains permissive for compatibility.

Production config cross-file safety validation on 2026-05-13 also passed:

- `cargo fmt --all -- --check`
- `cargo test -p runtime_orchestrator` (`28` tests)
- `cargo test -p aegisai-runtime-daemon` (`87` tests)
- `cargo clippy -p runtime_orchestrator --all-targets -- -D warnings`
- `git diff --check`
- Named production profiles reject scenario durations above
  `global_safety.max_boost_duration_ms`, `raise_nice` deltas outside
  `global_safety.max_priority_delta`, triggers whose required signals are absent
  from `collection.focus_signals`, live affinity without `pid_allowlist` mode
  plus a non-empty allowlist, and `use_cpuset = true`. Cross-file errors include
  both involved config files.

Repository status sync on 2026-05-13 also passed:

- `git fetch --prune`
- `bd lint`
- `git diff --check`
- `cargo fmt --all -- --check`
- `cargo test -p aegisai-runtime-daemon` (`87` tests)
- `cargo clippy --all-targets --all-features -- -D warnings`
- `bash bench/scripts/project_preflight.sh --check`
- Status search confirmed no stale open-status claims for closed
  `AegisAI_Runtime-cqv`, `AegisAI_Runtime-51c`, or `AegisAI_Runtime-8le`.
- The first preflight attempt exposed a real clippy issue:
  `CliConfig::parse` was only used by tests. It is now `#[cfg(test)]`, leaving
  runtime behavior unchanged and restoring the documented clippy gate.

System audit refresh on 2026-05-13 also passed its non-live gates:

- Scope: comprehensive current-state audit for design alignment, function
  behavior, syntax, compilation, latent risks, limitations, and next planning.
- Branch state before doc sync: `main...origin/main`, no code diff.
- `code-review-graph` snapshot: `1457` nodes, `11611` edges, `63` files,
  languages `rust`, `bash`, and `python`; no changed files detected against
  `HEAD`; architecture communities showed `0` cross-community edges and `0`
  warnings.
- Route/design verdict: current implementation still follows the documented
  route `collector -> classifier -> policy_engine -> actuator -> metrics`, with
  a rootless daemon, narrow helper boundary, bounded reversible actions, strict
  benefit gates, and scenario-specific policy lines. No evidence was found that
  the repository has drifted into a generic monitoring platform, scheduler
  replacement, or broad live-cgroup writer.
- Functional smoke verdict: mock daemon processed `3` events, triggered
  `inference_tail_guard` and `tool_call_booster`, applied `2` noop actions,
  recorded `2` tick rollbacks, and wrote audit highlights to
  `/tmp/aegisai_audit_runtime_mock_20260513.md`.
- Linux procfs ingestion verdict: `bash bench/scripts/linux_source_ingestion_smoke.sh`
  passed with `processed_events=4` and `run_queue_delay` observations under
  `linux-skeleton`; no live scheduler writes were made.
- Current-host helper-backed signal revalidation did not pass:
  `bash bench/scripts/helper_portability_smoke.sh` exited `1` with final bucket
  `helper unavailable` at
  `.cache/aegisai/helper_portability/helper_portability_gg-vm_6_8_0_111_generic_20260513T070947Z/helper_portability.md`.
  Historical helper validation remains indexed below, but fresh validation on
  this host is blocked until the approved helper/bpftrace privilege path is
  restored or the environment issue is documented as intentional.
- `cargo fmt --all -- --check`
- `cargo check --workspace`
- `cargo test --workspace` (`222` tests across workspace test binaries; doc
  tests ran with no failures)
- `cargo clippy --all-targets --all-features -- -D warnings`
- `python3 -m unittest discover -s bench/tool_call_booster -p 'test_*.py'`
  (`14` tests)
- `python3 -m unittest discover -s bench/scripts -p 'test_*.py'` (`21` tests)
- `for f in bench/scripts/*.sh; do bash -n "$f" || exit 1; done`
- `bash bench/scripts/project_preflight.sh --check`
- `bd lint`
- `git diff --check`
- CLI caveat: `cargo run -p aegisai-runtime-daemon -- --help` prints complete
  usage text but exits through the usage-error path with status `1`; tracked as
  `AegisAI_Runtime-dxh`.
- Code-structure caveat: graph analysis still flags large/high-degree files and
  functions, but `AegisAI_Runtime-76k` recorded explicit coverage decisions for
  the current hotspot set. Future decomposition should stay attached to active
  behavior work, not standalone cleanup.

Current-host helper-backed signal revalidation on 2026-05-13 passed:

- First default-helper reproduction still failed:
  `bash bench/scripts/helper_portability_smoke.sh` exited `1` with final
  bucket `helper unavailable` at
  `.cache/aegisai/helper_portability/helper_portability_gg-vm_6_8_0_111_generic_20260513T123334Z/helper_portability.md`.
- The approved helper privilege path was restored for validation by setting
  `AEGISAI_EBPF_HELPER` to an ignored artifact-local wrapper that executes only
  `target/debug/aegisai-ebpf-helper` via `sudo -n`; the daemon remained
  rootless and used `linux-skeleton`.
- `AEGISAI_EBPF_HELPER=/home/gg/AegisAI_Runtime/.cache/aegisai/helper_portability/current_host_privileged_wrapper/aegisai-ebpf-helper bash bench/scripts/helper_portability_smoke.sh`
  passed on `gg-vm` kernel `6.8.0-111-generic`; helper compatibility reported
  `compatible`, raw helper streams emitted `615` `offcpu_time` events and
  `9782` `io_latency` events, and rootless daemon runs normalized `8` events
  for each signal.
- Artifact:
  `.cache/aegisai/helper_portability/helper_portability_gg-vm_6_8_0_111_generic_20260513T123531Z/helper_portability.md`.

High-degree runtime hotspot coverage audit on 2026-05-13 passed:

- `CliConfig::parse_with_env`: covered by daemon CLI/env/default/override,
  invalid profile, source/backend, PID allowlist, live-actuator, warmup, and
  verification-log parser tests in `cargo test -p aegisai-runtime-daemon`.
  The explicit `--help` exit behavior remains separate as
  `AegisAI_Runtime-dxh`.
- `build_linux_rollback_report`: existing direct tests covered nice/affinity
  success, failure, mixed ordering, missing captured state, and disabled cpuset
  noise. This audit added direct live-guard branch coverage for rejected
  rollback targets and nice-only affinity rollback skips.
- `BpfTracePipe::start`: covered by source tests for successful off-CPU/I/O
  event flow, helper unavailable, tracepoint incompatibility, stdout/stderr
  start failures, malformed/unsupported probe lines, stop cleanup, helper args,
  and field inventory.
- `LinuxProbeDriver::poll_events`: covered by source tests for driver-backed
  batching, procfs deltas, missing counters, target exit tolerance, procfs
  fallback, and combined procfs plus bpftrace polling.
- `RuntimeOrchestrator::process_event`: covered by orchestrator tests for
  inference and tool-call triggers, cooldown, expiry-before-apply ordering,
  action and rollback traces, lifecycle audit fields, safety clamp fields, and
  PID allowlist classification.
- `bench/scripts/inference_tail_guard_ollama_smoke.sh::run_mode`: full live
  execution remains an integration path because it starts Ollama, stress, and
  daemon workloads. Deterministic script coverage stays at the `run.env` /
  acceptance-baseline provenance boundary with shell syntax validation; broader
  live benefit proof remains in the existing benchmark artifact gates.
- Verification: `cargo fmt --all -- --check`,
  `cargo test -p aegisai-actuator`, `cargo test -p aegisai-runtime-daemon`,
  `cargo test -p runtime_orchestrator runtime_orchestrator::tests`,
  `python3 -m unittest bench.scripts.test_inference_tail_guard_ollama_smoke`,
  and `bash -n bench/scripts/inference_tail_guard_ollama_smoke.sh`.

Audit caveats:

- Linux source preflight passed with `processed_events=0`; this is a safe
  startup/partial-probe check, not an ingestion or benefit proof.
- The 2026-05-13 audit reran controlled Linux source ingestion and observed
  `processed_events=4`; that is procfs ingestion proof, not helper-backed eBPF
  proof and not live benchmark benefit proof.
- Controlled Linux source ingestion smoke passed on 2026-05-12 with
  `processed_events=4` and `run_queue_delay` observations. Hosts that cannot
  expose readable procfs counters or positive controlled-worker deltas return
  `SKIPPED` with exit code `77`.
- The 2026-05-13 default-helper portability reruns on `gg-vm` kernel
  `6.8.0-111-generic` failed as `helper unavailable` when the helper ran
  without the restored privilege path. Cite the current-host validation only
  when `AEGISAI_EBPF_HELPER` points at the privileged helper boundary; packaging
  still needs to implement a durable approved helper path.
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
| `offcpu_time` + `io_latency` current-host revalidation, `6.8.0-111-generic` | `2026-05-13T12:35:32Z - Helper Portability Smoke` | `.cache/aegisai/helper_portability/helper_portability_gg-vm_6_8_0_111_generic_20260513T123531Z` | `validated signal`; helper compatibility `compatible`; raw streams emitted `615` off-CPU and `9782` I/O events; daemon recorded `8` normalized events for each signal |
| `offcpu_time` + `io_latency` default-helper current-host reproduction, `6.8.0-111-generic` | `2026-05-13T12:33:34Z - Helper Portability Smoke` | `.cache/aegisai/helper_portability/helper_portability_gg-vm_6_8_0_111_generic_20260513T123334Z` | `FAIL`: final bucket `helper unavailable`; target/debug helper reported bpftrace eBPF backend unavailable without the restored helper privilege path |
| `offcpu_time` + `io_latency` earlier current-host revalidation attempt, `6.8.0-111-generic` | `2026-05-13T07:09:47Z - Helper portability smoke during system audit` | `.cache/aegisai/helper_portability/helper_portability_gg-vm_6_8_0_111_generic_20260513T070947Z` | `FAIL`: final bucket `helper unavailable`; target/debug helper reported bpftrace eBPF backend unavailable |
| `offcpu_time` + `io_latency` portability matrix, `6.8.0-111-generic` | `2026-05-12T14:42:07Z - Two-kernel helper portability matrix` | `.cache/aegisai/helper_portability/helper_portability_gg_vm_6_8_0_111_20260512T141448Z` | `validated signal`; helper compatibility `compatible`; raw streams emitted `624` off-CPU and `12209` I/O events; daemon recorded `8` normalized events for each signal |
| `offcpu_time`, `6.8.0-110-generic` / `Ubuntu 24.04.4 LTS` | `2026-05-10T03:37:57Z - Helper-backed offcpu_time validation` | `/tmp/aegisai-jtt/artifacts` | helper ready; raw stream attached and emitted `348` events; daemon recorded `8` normalized events |
| `io_latency`, `6.8.0-110-generic` / `Ubuntu 24.04.4 LTS` | `2026-05-10T03:48:11Z - Helper-backed io_latency validation` | `/tmp/aegisai-jtt/artifacts` | helper ready; block tracepoints exposed required fields; raw stream emitted `4005` events; daemon recorded `8` normalized events |

Future helper conclusions should use these buckets: `helper unavailable`,
`tracepoint incompatible`, `no workload events`, or `validated signal`.
Compatibility diagnostics now separate the first two buckets from zero-event
workloads before portability matrix runs. The smoke script also enforces those
buckets at the result layer before event-count classification.

## Open Gap Index

Current `bd` state after runtime daemon help exit fix: `78` total issues,
`5` open, `0` in progress, `3` blocked, `73` closed.
`docs/latest_tasks.md` now contains only the active prioritized todo queue;
historical evidence remains in this file, `docs/acceptance_ledger.md`, and
`docs/verification_log.md`.

- `AegisAI_Runtime-ufp` — implement the daemon/helper packaging path from
  `docs/packaging_contract.md`. `AegisAI_Runtime-ufp.1` is complete: the first
  target is Debian/Ubuntu systemd, with rootless daemon user/group, binary
  paths, production profile path, log path, helper privilege boundary,
  prerequisite behavior, rollback, and uninstall rules defined. The remaining
  open work is installer/service implementation plus dry-run or VM smoke
  verification.
- `AegisAI_Runtime-0ry` — close the deferred extension parent after
  `AegisAI_Runtime-0ry.1` completed the split. `bd ready` now lists this
  parent for closure validation.
- `AegisAI_Runtime-0ry.2` — deferred observability dashboard, blocked behind
  production packaging. The issue records prerequisites, non-goals, read-only
  safety evidence, benchmark evidence, and a verification gate.
- `AegisAI_Runtime-0ry.3` — deferred GPU coordination, blocked behind
  production packaging. The issue records prerequisites, non-goals, device and
  privilege safety evidence, benchmark evidence, and a verification gate.
- `AegisAI_Runtime-0ry.4` — deferred online adaptive policy, blocked behind
  production packaging. The issue records prerequisites, non-goals, shadow-mode
  safety evidence, benchmark evidence, and a verification gate.

Recently closed:

- `AegisAI_Runtime-dxh` — normalized explicit
  `aegisai-runtime-daemon --help` behavior so usage prints to stdout and exits
  `0`, while invalid/incomplete arguments still exit nonzero. Verification:
  `cargo test -p aegisai-runtime-daemon --test cli_help`,
  `cargo test -p aegisai-runtime-daemon`, direct `cargo run -p
  aegisai-runtime-daemon -- --help`, direct `cargo run -p
  aegisai-runtime-daemon -- --repo-root`, and `git diff --check`.

- `AegisAI_Runtime-76k` — recorded coverage/decomposition decisions for
  `CliConfig::parse_with_env`, `build_linux_rollback_report`,
  `BpfTracePipe::start`, `LinuxProbeDriver::poll_events`,
  `RuntimeOrchestrator::process_event`, and the Ollama smoke `run_mode` shell
  path. Added targeted rollback-report live-guard tests for target rejection
  and nice-only affinity rollback skips. No broad hotspot decomposition was
  made; future splits remain tied to active behavior work.
- `AegisAI_Runtime-3gz` — revalidated helper-backed `offcpu_time` and
  `io_latency` on current `gg-vm` kernel `6.8.0-111-generic` after restoring
  the helper privilege path through an ignored artifact-local wrapper. The
  passing artifact is
  `.cache/aegisai/helper_portability/helper_portability_gg-vm_6_8_0_111_generic_20260513T123531Z/helper_portability.md`;
  final bucket `validated signal`, raw streams `offcpu_raw=615` and
  `io_raw=9782`, daemon-normalized events `8` for each signal. The default
  unprivileged helper reproduction remains documented as `helper unavailable`.
- `AegisAI_Runtime-mqr` — simplified `docs/latest_tasks.md` to the active todo
  queue and added the missing Beads dependency from `AegisAI_Runtime-0ry` to
  `AegisAI_Runtime-0ry.1`.
- `AegisAI_Runtime-cqv` — closed the production config profile parent after
  `AegisAI_Runtime-cqv.1`, `AegisAI_Runtime-cqv.2`, and
  `AegisAI_Runtime-cqv.3` completed selector, strict schema validation, and
  cross-file safety validation acceptance.
- `AegisAI_Runtime-51c` — closed the helper portability parent after
  `AegisAI_Runtime-51c.1` through `AegisAI_Runtime-51c.4` completed
  compatibility taxonomy, two-kernel helper matrix, controlled Linux ingestion
  smoke, and BpfTracePipe startup failure coverage.
- `AegisAI_Runtime-ufp.1` — defined the Debian/Ubuntu systemd packaging
  contract in `docs/packaging_contract.md`. The contract names `_aegisai` as
  the rootless daemon user/group, `/usr/bin/aegisai-runtime-daemon` and
  `/usr/lib/aegisai/aegisai-ebpf-helper` as binary paths,
  `/etc/aegisai/configs/profiles/production/` as the selected profile path, and
  `/var/log/aegisai/runtime-daemon.md` as the daemon verification log. It keeps
  helper-backed eBPF mode behind an explicit privilege boundary and leaves
  installer code to the parent packaging task.
- `AegisAI_Runtime-8le` — configured Beads Dolt remote sync as local-only for
  this repository. `bd dolt remote list` shows `origin` at
  `file:///home/gg/AegisAI_Runtime/.beads/backup/dolt-remote/AegisAI_Runtime`,
  and plain `bd dolt push` completes successfully.
- `AegisAI_Runtime-vsl` — fixed helper portability smoke bucket
  classification. Compatibility diagnostics are parsed before event counts; a
  helper unavailable or tracepoint incompatible status writes the matching final
  bucket, records `Overall result: FAIL`, and exits nonzero. Regression tests
  cover helper unavailable, tracepoint incompatible, compatible zero events, and
  daemon compatibility failure.
- `AegisAI_Runtime-cqv.1` — added named production profile cross-file safety
  validation after schema parsing. Tests cover duration and priority caps,
  enabled trigger/focus-signal consistency, live affinity PID allowlist scope,
  disabled live cpuset writes, and errors naming both files involved.
- `AegisAI_Runtime-cqv.3` — added strict production config schema validation
  for named profiles. Errors include profile, file, section, key, and
  constraint context; tests cover unknown keys, missing fields, invalid focus
  signals, invalid classifier rule keys, invalid scenarios, invalid pin
  strategies, out-of-range `raise_nice`, zero durations, and local demo
  compatibility.
- `AegisAI_Runtime-cqv.2` — added runtime production profile selection:
  `--config-profile` overrides `AEGISAI_CONFIG_PROFILE`, which overrides the
  local demo default; named profiles load non-example files from
  `configs/profiles/<name>/`, invalid profile names are rejected, missing
  profile roots fail before partial startup, and the existing example config
  path remains compatible.
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
  `agent/runtime_orchestrator/src/config.rs`,
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
