# Current Repository Status

_Last reviewed: 2026-05-03_

This file is the concise status and TODO index for the repository. The source of truth for task tracking remains `bd`; issue IDs below point at the active beads records.

## Audit Snapshot

The repository currently has a runnable Rust workspace for the AegisAI Runtime control loop:

`collector -> classifier -> policy_engine -> actuator -> metrics`

Implemented and verified capabilities:

- `runtime_daemon` can run the mock control-loop path and the Linux procfs preflight path.
- Linux source fallback observes `run_queue_delay`, `cpu_migration`, and `major_page_fault` through procfs-derived signals.
- Metadata enrichment supports procfs process name, cmdline, cgroup, parent fields, and demo/static metadata.
- Actuator backends include safe `noop`, planning `linux-skeleton`, auditable `linux-command-dry-run`, and guarded `linux-command` behind explicit confirmation and PID allowlist.
- `inference_tail_guard` and `tool_call_booster` both trigger in deterministic/mock or harnessed paths.
- Phase 4 benefit reporting now refuses to claim MVP benefit unless live guarded actions produce effective host-level changes.

## Verification On 2026-05-03

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
- No live `ollama` A/B run was started during this audit because that requires an explicit experiment window and live actuator decision.

## Functional Completion

Completed or standing:

- Core module boundaries and shared contracts.
- Config loading from repo-root examples.
- Awareness/classifier rules for process, cmdline, parent, cgroup, tag markers, and PID allowlist.
- Inference Tail Guard policy path with cooldown, bounded action plans, metrics traces, and rollback lifecycle.
- Tool Call Booster policy path and real executor lifecycle harness entrypoint.
- Verification scripts and append-only verification log.

Partially complete:

- eBPF crate has probe descriptors, filters, registry, and event validation, but the live runtime still relies on procfs fallback for current preflight signals.
- Live Linux command backend is guarded and auditable, but current benefit artifacts show no effective live actuator changes in the most recent Phase 4 report.
- Tool Call Booster has lifecycle detection and trigger proof, but not a repeated baseline-vs-guarded benefit report.
- Explain/tune can build reports from metrics, but online tuning remains outside the current scope.

Not complete:

- Real off-CPU and I/O latency eBPF event ingestion in the runtime loop.
- Proven host-level MVP benefit from effective live guarded actions.
- Production daemon packaging/service management.
- Dashboard, GPU coordination, adaptive policy learning, or background isolation.

## Active TODO Index

- `AegisAI_Runtime-s6f` — Prove effective live Inference Tail Guard actuator benefit.
- `AegisAI_Runtime-4nv` — Complete real eBPF signal coverage for off-CPU and I/O latency.
- `AegisAI_Runtime-bx1` — Turn Tool Call Booster harness into repeated A/B benefit proof.
- `AegisAI_Runtime-azv` — Harden audit coverage for actuator and runtime hot paths.

Use:

```bash
bd show <issue-id>
bd ready
```

## Next Correct Stage

The next major stage is not more scaffolding. It is evidence hardening:

1. Run a controlled Linux live experiment where `live_guarded` produces at least one effective host-level actuator change.
2. Keep the Phase 4 benefit gate strict: no effective live action means no MVP benefit claim.
3. Wire real eBPF off-CPU and I/O latency events behind the existing source boundary.
4. Promote Tool Call Booster from trigger/harness proof to repeated A/B benefit proof.
5. Add targeted tests around the high-risk hot paths identified by the code graph: actuator rollback reports, Linux command apply/rollback failures, procfs sampling edge cases, runtime source behavior, and benefit report interpretation.

## Review Risks

- Large files remain in `agent/runtime_daemon/src/source.rs`, `agent/actuator/src/backend.rs`, `agent/explain_tune/src/engine.rs`, and `agent/runtime_orchestrator/src/runtime_orchestrator.rs`; future changes should be narrow and test-led.
- `linux-command` can change real process scheduler state. Keep `--confirm-live-actuator` and PID allowlist mandatory.
- The current `docs/mvp_benefit_report.md` is intentionally a `FAIL`: it records trend evidence but no effective live actuator action.
