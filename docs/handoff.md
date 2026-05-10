# Handoff

_Updated: 2026-05-10_

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
- Helper-backed `offcpu_time` and `io_latency` have controlled-workload
  validation artifacts recorded for `AegisAI_Runtime-jtt`.

Use `docs/current_status.md` for compact status, `docs/task_list.md` for active
tasks, and `docs/next_stage.md` for stage direction.

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
- `AegisAI_Runtime-jtt` helper validation reached `validated signal` for both
  `offcpu_time` and `io_latency`; see `docs/verification_log.md` entries
  `2026-05-10T03:37:57Z` and `2026-05-10T03:48:11Z`.

Bounded evidence:

- Procfs fallback covers current safe Linux preflight signals, and runtime
  source wiring now keeps the daemon rootless while using `aegisai-ebpf-helper`
  for off-CPU and I/O latency ingestion.
- `linux-command-dry-run` proves command planning and rollback audit shape
  without changing host state.
- The latest Phase 4 report deliberately reports `FAIL`: effective live
  host-level `taskset` actions were recorded, but stable benefit was not proven.

Helper validation artifact summary:

- Artifact root: `/tmp/aegisai-jtt/artifacts`.
- Host/tooling: `gg-vm`, Linux `6.8.0-110-generic`, `bpftrace v0.20.2`, tracefs
  mounted at `/sys/kernel/tracing`.
- Readiness: `AEGISAI_BPFTRACE=/usr/bin/bpftrace /tmp/aegisai-jtt/bin/aegisai-ebpf-helper --check` exited `0`; helper mode was `4755 root:root`.
- `offcpu_time`: `timeout 8s ... stream --offcpu --pid 5705` ended by timeout
  after `348` raw helper events and zero stderr lines; daemon exited `0` with
  `8` normalized events, total `165842`, max `21169`.
- `io_latency`: block tracepoint formats exposed `dev` and `sector`;
  `timeout 10s ... stream --io --process-name ollama` ended by timeout after
  `4005` raw helper events and zero stderr lines; daemon exited `0` with `8`
  normalized events, total `5013`, max `712`.
- Future conclusions should use these buckets: `helper unavailable`,
  `tracepoint incompatible`, `no workload events`, or `validated signal`.

Not proven:

- Host-level MVP performance benefit from effective live guarded actions.
- Repeated Tool Call Booster baseline-vs-guarded benefit.
- Production service packaging or unattended daemon deployment.

## Active Follow-Up Issues

- `AegisAI_Runtime-lql` — Tune live Inference Tail Guard affinity benefit.
- `AegisAI_Runtime-94s` — Run controlled Tool Call Booster live guarded benefit proof.
- `AegisAI_Runtime-v2y` — Modularize live CPU affinity planning.

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
5. Promote Tool Call Booster from lifecycle trigger proof to repeated benefit
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

> Continue from the 2026-05-10 evidence-hardening state in
> `/home/gg/AegisAI_Runtime`. Read `docs/current_status.md`,
> `docs/task_list.md`, `docs/next_stage.md`, and the latest entries in
> `docs/verification_log.md`. Preserve the strict Phase 4 benefit gate:
> effective live action and stable repeated benefit are both required. Use beads
> for task tracking and start from the active issues listed in `bd ready`.
