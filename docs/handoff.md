# Handoff

_Updated: 2026-05-03_

## Current State

The repository is in an evidence-hardening phase.

The runnable control-loop mainline is wired:

`collector -> classifier -> policy_engine -> actuator -> metrics`

Current runtime capabilities:

- `runtime_daemon` runs with deterministic `mock` source validation.
- Linux preflight uses procfs-backed source behavior for `run_queue_delay`,
  `cpu_migration`, and `major_page_fault`.
- Metadata enrichment supports procfs and demo/static providers.
- Actuator modes include `noop`, `linux-skeleton`,
  `linux-command-dry-run`, and guarded `linux-command`.
- `linux-command` remains gated by explicit confirmation plus PID allowlist.
- `inference_tail_guard` and `tool_call_booster` both have working trigger
  paths; only Inference Tail Guard currently has a Phase 4 benefit report.

Use `docs/current_status.md` as the compact source for current status, active
TODO issue IDs, and next-stage direction.

## Latest Audit Verification

The 2026-05-03 audit passed:

```bash
bash bench/scripts/verify_workspace.sh
```

This ran:

- `cargo check --workspace`
- `cargo test --workspace`
- `cargo fmt --all -- --check`
- `cargo clippy --all-targets --all-features -- -D warnings`
- mock daemon smoke
- Linux source preflight smoke

Additional checks passed:

```bash
for f in bench/scripts/*.sh; do bash -n "$f" || exit 1; done
bash bench/scripts/toolchain_preflight.sh
bash bench/scripts/inference_tail_guard_preflight.sh
```

The latest workspace verification entry in `docs/verification_log.md` ends with
`Overall result: PASS`.

## Current Evidence

Strong evidence:

- Rust workspace compiles, tests, formats, and passes clippy with warnings
  denied.
- The mock daemon smoke triggers both `inference_tail_guard` and
  `tool_call_booster` and completes rollback lifecycle accounting.
- Linux source preflight exits cleanly with `--allow-partial-probes`.
- Toolchain preflight confirms the required local development/eBPF tools are
  present on this host.

Bounded evidence:

- Procfs fallback covers current safe Linux preflight signals, but real eBPF
  off-CPU and I/O latency ingestion is still pending.
- `linux-command-dry-run` proves command planning and rollback audit shape
  without changing host state.
- The latest Phase 4 report shows live guarded trends but deliberately reports
  `FAIL` because no effective live host-level actuator changes were recorded.

Not proven:

- Host-level MVP performance benefit from effective live guarded actions.
- Repeated Tool Call Booster baseline-vs-guarded benefit.
- Production service packaging or unattended daemon deployment.

## Active Follow-Up Issues

- `AegisAI_Runtime-s6f` — Prove effective live Inference Tail Guard actuator
  benefit.
- `AegisAI_Runtime-4nv` — Complete real eBPF signal coverage for off-CPU and
  I/O latency.
- `AegisAI_Runtime-bx1` — Turn Tool Call Booster harness into repeated A/B
  benefit proof.
- `AegisAI_Runtime-azv` — Harden audit coverage for actuator and runtime hot
  paths.

Use:

```bash
bd ready
bd show <issue-id>
```

## Recommended Next Route

1. Keep `qwen2.5:0.5b` as the default first model for comparable Ollama runs.
2. Reconfirm safe state with `verify_workspace.sh` and the two preflight scripts.
3. Run controlled live guarded experiments only when a PID allowlist,
   permissions, and experiment window are explicit.
4. Regenerate `docs/mvp_benefit_report.md` only from real artifacts and keep the
   strict effective-action gate.
5. Wire missing eBPF-backed off-CPU/I/O signals behind the existing source
   abstraction.
6. Promote Tool Call Booster from lifecycle trigger proof to repeated benefit
   proof.

## Safe Commands

Reconfirm workspace:

```bash
bash bench/scripts/verify_workspace.sh
```

Reconfirm host readiness:

```bash
bash bench/scripts/toolchain_preflight.sh
bash bench/scripts/inference_tail_guard_preflight.sh
```

Preview planned command actions safely:

```bash
AEGISAI_DAEMON_BACKEND=linux-command-dry-run \
  bash bench/scripts/inference_tail_guard_ollama_smoke.sh
```

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

> Continue from the 2026-05-03 evidence-hardening state in
> `/home/gg/AegisAI_Runtime`. Read `docs/current_status.md`,
> `docs/handoff.md`, `docs/next_stage.md`, and the latest entries in
> `docs/verification_log.md`. Preserve the strict Phase 4 benefit gate: no
> effective live actuator action means no MVP benefit claim. Use beads for task
> tracking and start from the active issues listed in `docs/current_status.md`.
