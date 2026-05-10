# Handoff

_Updated: 2026-05-10_

## Current State

The repository is in the post-acceptance product-evidence phase.

The runnable control-loop mainline is wired:

`collector -> classifier -> policy_engine -> actuator -> metrics`

Current runtime capabilities:

- `runtime_daemon` runs with deterministic `mock` source validation.
- Linux preflight uses procfs-backed source behavior for `run_queue_delay`,
  `cpu_migration`, and `major_page_fault`.
- `aegisai-ebpf-helper` has controlled-workload validation artifacts for
  helper-backed `offcpu_time` and `io_latency`.
- Metadata enrichment supports procfs and demo/static providers.
- Actuator modes include `noop`, `linux-skeleton`, `linux-command-dry-run`, and
  guarded `linux-command`.
- Live CPU affinity planning is modularized in `agent/actuator/src/cpu_affinity.rs`.
- `linux-command` remains gated by explicit confirmation plus PID allowlist.
- Inference Tail Guard and Tool Call Booster both have repeated A/B report paths.

Use `docs/current_status.md` for compact status, `docs/task_list.md` for the
accepted 19-task ledger, and `docs/next_stage.md` for current stage direction.

## Latest Acceptance Verification

The 2026-05-10 acceptance pass ran without modifying `docs/verification_log.md`
by redirecting `AEGISAI_VERIFY_LOG` to `/tmp`.

Passed:

```bash
cargo fmt --all -- --check
cargo test --workspace
cargo clippy --all-targets --all-features -- -D warnings
python3 -m unittest discover -s bench/tool_call_booster -p 'test_*.py'
python3 -m unittest discover -s bench/scripts -p 'test_*.py'
for f in bench/scripts/*.sh; do bash -n "$f" || exit 1; done
AEGISAI_VERIFY_LOG=/tmp/aegisai_acceptance_verify_workspace.md bash bench/scripts/verify_workspace.sh
AEGISAI_VERIFY_LOG=/tmp/aegisai_acceptance_toolchain_preflight.md bash bench/scripts/toolchain_preflight.sh
AEGISAI_VERIFY_LOG=/tmp/aegisai_acceptance_inference_preflight.md bash bench/scripts/inference_tail_guard_preflight.sh
```

`verify_workspace.sh` reported `Overall result: PASS`; the Linux source smoke
processed zero events, which is acceptable for preflight because it validates
startup/configuration safety rather than live workload benefit.

## Current Evidence

Strong evidence:

- Rust workspace compiles, tests, formats, and passes clippy with warnings
  denied.
- The mock daemon smoke triggers both `inference_tail_guard` and
  `tool_call_booster` and completes rollback lifecycle accounting.
- Linux source preflight exits cleanly with `--allow-partial-probes`.
- Toolchain preflight confirms the required local development/eBPF tools are
  present on this host.
- `AegisAI_Runtime-jtt` helper validation reached `validated signal` for both
  `offcpu_time` and `io_latency`; see `docs/verification_log.md` entries
  `2026-05-10T03:37:57Z` and `2026-05-10T03:48:11Z`.
- CPU affinity planner tests cover configured-vs-online CPU mismatch, restricted
  VM masks, empty intersections, reserved/low-contention selection, and
  deterministic rollback targets.
- Benefit report tests prevent noop/dry-run or ineffective live actions from
  producing `PASS`.

Bounded evidence:

- Procfs fallback covers current safe Linux preflight signals, and runtime
  source wiring keeps the daemon rootless while using `aegisai-ebpf-helper` for
  off-CPU and I/O latency ingestion.
- `linux-command-dry-run` proves command planning and rollback audit shape
  without changing host state.
- Tool Call Booster proves lifecycle recognition, stage audit continuity, and
  guarded report contracts, but not repeated latency benefit.

Not proven:

- Host-level MVP performance benefit from effective live guarded actions.
- Repeated Tool Call Booster guarded latency benefit.
- Production service packaging or unattended daemon deployment.

## Latest Artifacts

Inference Tail Guard:

- Report: `docs/mvp_benefit_report.md`
- Run ID: `live_guarded_phase4_calibrated_20260510T043859Z`
- Artifact root:
  `.cache/aegisai/inference_tail_guard_phase4/live_guarded_phase4_calibrated_20260510T043859Z`
- Result: `FAIL`
- Key fact: mode contracts passed and `live_effective_action_count_total=3`, but
  stable repeated benefit was not proven; failure cause is `noisy_workload`.

Tool Call Booster:

- Report:
  `.cache/aegisai/tool_call_booster/live_guarded_tcb_issue_94s_final_20260510T053527Z/tool_call_booster_benefit_report.md`
- Run ID: `live_guarded_tcb_issue_94s_final_20260510T053527Z`
- Result: contract `PASS`, benefit `FAIL`
- Key fact: `live_guarded` improved `0/3` comparable rounds by at least `5.0%`.

Helper validation:

- Artifact root: `/tmp/aegisai-jtt/artifacts`.
- Host/tooling: `gg-vm`, Linux `6.8.0-110-generic`, `bpftrace v0.20.2`, tracefs
  mounted at `/sys/kernel/tracing`.
- `offcpu_time`: `348` raw helper events and `8` normalized daemon events.
- `io_latency`: `4005` raw helper events and `8` normalized daemon events.
- Future conclusions should use these buckets: `helper unavailable`,
  `tracepoint incompatible`, `no workload events`, or `validated signal`.

## Open Follow-Up Issues

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

## Recommended Next Route

1. Keep `qwen2.5:0.5b` as the default first model for comparable Ollama runs.
2. Reconfirm safe state with `verify_workspace.sh` and the two preflight scripts,
   redirecting `AEGISAI_VERIFY_LOG` to `/tmp` unless an append-only log entry is
   intentionally desired.
3. Run controlled live guarded experiments only when a PID allowlist,
   permissions, and experiment window are explicit.
4. Regenerate `docs/mvp_benefit_report.md` only from real artifacts and keep the
   strict effective-action gate.
5. Continue Tool Call Booster guarded experiments from the current contract-PASS,
   benefit-FAIL baseline.

## Live Experiment Guardrail

Run live guarded mode only with explicit approval of the experiment window:

```bash
AEGISAI_AB_MODES=baseline,noop_observation,dry_run,live_guarded \
AEGISAI_CONFIRM_LIVE_ACTUATOR=1 \
AEGISAI_LIVE_PID_ALLOWLIST=<pid,...> \
  bash bench/scripts/inference_tail_guard_phase4_report.sh
```

`linux-command` may call real `renice` and, if enabled, `taskset` against the
target PID. Keep `cpuset` disabled until the nice/affinity path is repeatedly
clean.

## Resume Prompt

If a future session needs a direct restart prompt:

> Continue from the 2026-05-10 post-acceptance product-evidence state in
> `/home/gg/AegisAI_Runtime`. Read `docs/current_status.md`,
> `docs/task_list.md`, `docs/next_stage.md`, and the latest relevant entries in
> `docs/verification_log.md`. Preserve the strict benefit gates: effective live
> action and stable repeated benefit are both required. Use beads for task
> tracking and start from `bd ready`.
