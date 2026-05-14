# Evidence-Hardening Acceptance Ledger

_Updated: 2026-05-11_

This file records the accepted 19-task evidence-hardening pass. It is a ledger,
not an active tracker. Use `bd` for active work and `docs/status.md` for the
current gap index.

Post-ledger note, 2026-05-12: `AegisAI_Runtime-51c.1` and
`AegisAI_Runtime-51c.2` are now complete. Helper compatibility diagnostics
classify `helper unavailable`, `tracepoint incompatible`, and compatible
tracepoint field inventory before helper streams start, and the two-kernel
`gg-vm` matrix covers kernels `6.8.0-110-generic` and `6.8.0-111-generic` with
`validated signal` outcomes.

Post-ledger note, 2026-05-13: `AegisAI_Runtime-vsl` fixed the helper
portability smoke result layer. The script now parses compatibility diagnostics
before event counts, exits nonzero for `helper unavailable` or
`tracepoint incompatible`, and reserves `no workload events` for compatible
helper diagnostics with zero raw or normalized events.

Post-ledger note, 2026-05-13: the remaining parent status was synced after
follow-up acceptance. `AegisAI_Runtime-51c` is closed after helper compatibility
taxonomy, two-kernel helper matrix, controlled Linux ingestion smoke, and
BpfTracePipe startup failure coverage. `AegisAI_Runtime-cqv` is closed after
profile selection, strict production schema validation, and cross-file safety
validation. `AegisAI_Runtime-8le` is closed with a local-only Beads Dolt
filesystem remote.

## Acceptance Conclusion

The 19 evidence-hardening tasks are accepted against the current repository
state.

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

The `AEGISAI_VERIFY_LOG=/tmp/...` override kept this acceptance pass from
rewriting or appending to `docs/verification_log.md`.

## Accepted Items

| # | item | accepted evidence |
| --- | --- | --- |
| 1 | Align status docs with latest benefit report | Closed `AegisAI_Runtime-bmz`; status and report agree that effective live action exists but stable benefit is unproven. |
| 2 | Keep artifact pointers visible | Status points to `docs/mvp_benefit_report.md` and latest Phase 4 artifacts. |
| 3 | Validate helper-backed `offcpu_time` | Closed `AegisAI_Runtime-jtt`; log entry `2026-05-10T03:37:57Z` records helper readiness, `348` raw helper events, and `8` normalized daemon observations. |
| 4 | Validate helper-backed `io_latency` | Closed `AegisAI_Runtime-jtt`; log entry `2026-05-10T03:48:11Z` records compatible block fields, `4005` raw helper events, and `8` normalized daemon observations. |
| 5 | Record helper validation artifacts | Status docs index artifacts and standard conclusion buckets: `helper unavailable`, `tracepoint incompatible`, `no workload events`, `validated signal`. |
| 6 | Extract CPU affinity planning | Closed `AegisAI_Runtime-v2y`; planning lives in `agent/actuator/src/cpu_affinity.rs`. |
| 7 | Test live `taskset` target selection | Workspace tests cover online/allowed CPU intersections, restricted VM masks, empty intersections, target selection, and rollback targets. |
| 8 | Preserve the strict Phase 4 gate | Report tests reject noop/dry-run and ineffective live actions as benefit proof. |
| 9 | Re-run live guarded Phase 4 | Closed `AegisAI_Runtime-lql`; at that acceptance point the run recorded `3` effective actions and verdict `FAIL` because repeated benefit was not met. Later sample-sizing artifacts now provide the current Inference Tail Guard `PASS`. |
| 10 | Tune one experiment variable at a time | Phase 4 artifacts record `changed_variable`; the old failing run recorded `affinity_nice_interaction` and failure cause `noisy_workload`; the current accepted proof records `sample_sizing`. |
| 11 | Update MVP benefit report from artifacts | `docs/mvp_benefit_report.md` is artifact-backed and keeps `PASS` restricted to effective live action plus stable repeated benefit. |
| 12 | Run controlled live guarded Tool Call Booster proof | Closed `AegisAI_Runtime-94s`; at that acceptance point the run had contract `PASS`, benefit `FAIL`. Later fixed-work guarded artifacts now provide the current Tool Call Booster scheduler-benefit `PASS`. |
| 13 | Verify tool-call audit continuity | Runtime/orchestrator and benchmark tests cover lifecycle audit fields, rollback trace preservation, summaries, and benefit interpretation. |
| 14 | Decide real `WarmupExecutor` boundary | `AegisAI_Runtime-14r` defines explicit command-backed warmup: default deferred audit, bounded apply command only when configured, rollback no-op audit, and reports separate warmup counts from scheduler benefit. |
| 15 | Harden actuator rollback tests | Closed `AegisAI_Runtime-03b`; tests cover apply success with rollback failure, missing capture state, leases, live guards, and audit fields. |
| 16 | Harden Linux source and procfs edge tests | Closed `AegisAI_Runtime-2s3`; tests cover zero-event preflight, partial probes, missing procfs fields, process exits, helper unavailability, and sampling. |
| 17 | Harden benefit report interpretation tests | Closed `AegisAI_Runtime-n3y`; report tests prevent observation-only, dry-run, priority-limited, or ineffective live actions from producing `PASS`. |
| 18 | Define production config profile boundaries | Closed `AegisAI_Runtime-5bx`; durable boundaries now live in `docs/architecture.md`. |
| 19 | Split hotspot files only with active behavior work | Closed `AegisAI_Runtime-5bx`; hotspot split boundaries now live in `docs/architecture.md`. |
| 20 | Expand CLI parser edge-case tests | Closed `AegisAI_Runtime-d42`; runtime daemon tests cover duplicate and empty PID allowlists, unknown source/backend values, missing verification log paths, and warmup command boundaries. |

## Remaining Gap Classes

The accepted pass did not complete production readiness. Since this ledger was
created, Inference Tail Guard benefit has been proven for the latest controlled
run shape, Tool Call Booster guarded latency benefit has been proven for the
latest fixed-work guarded run shape while the older stable executor-control run
remains a historical non-controlled workload `FAIL`, the `WarmupExecutor`
boundary has been implemented, the live cpuset/background safety boundary has
been documented, policy safety cap normalization has been completed, the
cpuset/background dry-run planner has been completed, helper portability and
production config profile parents have been closed, Beads Dolt sync has been
configured, and the runtime daemon explicit help path now exits successfully
without weakening invalid-argument failures. The Debian/Ubuntu systemd
packaging path now exists under `packaging/debian-systemd/` with a rootless
daemon unit, separate helper install path, production-profile staging,
preflight checks, remove/purge behavior, and dry-run smoke coverage. Deferred
online adaptive policy planning now has a shadow-only evidence gate with
deterministic replay, safety invariant tests, drift/freeze handling, bounded
retention, rollback-plan checks, and static-baseline comparison artifacts. The
remaining blockers are:

- unblocked but still evidence-gated deferred runtime extensions:
  `AegisAI_Runtime-0ry.2` (dashboard) and `AegisAI_Runtime-0ry.3` (GPU
  coordination)

See `docs/status.md` for the current issue index and `bd ready` for executable
work.
