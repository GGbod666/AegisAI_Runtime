# Latest Task List

_Regenerated: 2026-05-14_

`bd` is the source of truth. This file is only the current prioritized todo
queue. Historical evidence belongs in `docs/status.md`,
`docs/acceptance_ledger.md`, and `docs/verification_log.md`.

## Queue

Next stage focus: make Inference Tail Guard visibly stronger than the current
MVP proof. The existing accepted run proved host-level benefit, but the effect
was modest: `lat P95` improved `2.90%` and jitter improved `6.42%` in
`live_guarded_phase4_sample_sizing_20260511T000000Z`.

Target for the next proof:

- primary: `lat P95` or `lat P99` improves by at least `15%`
- secondary: jitter improves by at least `15%`
- stability: `4/5` comparable rounds improve, or `3/3` rounds all improve
- guardrail: `TTFT P95/P99` regression stays under `5%`
- evidence: effective live host action, clean rollback, and side-effect summary

Ready now:

- `AegisAI_Runtime-20w`: implement the guarded owned-cgroup isolation applier.
  This is now unblocked by the attribution and dry-run planner artifacts. Keep
  live cgroup writes limited to an administrator-created AegisAI-owned cgroup
  v2 subtree, with explicit confirmation, bounded affected sets, hard
  rejections, apply audit, and rollback evidence.

Blocked follow-up:

- `AegisAI_Runtime-t49`: run the Phase 5 Tail Guard isolation A/B proof. It
  is now blocked only on `AegisAI_Runtime-20w`.

## Ordering Rules

- Finish validation gaps before expanding runtime scope.
- Do Tail Guard attribution before implementing stronger live isolation.
- Keep background demotion dry-run-only until affected sets, rejection reasons,
  and rollback capture requirements are visible in artifacts.
- Keep live cgroup writes limited to an administrator-created AegisAI-owned
  cgroup v2 subtree, with explicit confirmation and rollback evidence.
- Keep production packaging before deferred dashboard/GPU/adaptive extensions.
- Do not add new runtime behavior while doing deferred-extension planning.

## Recently Completed

- `AegisAI_Runtime-4cc`: Tail Guard attribution now has
  `docs/tail_guard_attribution_report.md` and
  `.cache/aegisai/inference_tail_guard_tail_attribution/live_guarded_phase4_sample_sizing_20260511T000000Z/`.
  Current result is `NOT_PROVEN_HELPER_GAP`: visible duration-backed scheduler
  attribution peaks at `1.57%` of P95, below the `15%` P95/P99 target, while
  helper-backed `offcpu_time` / `io_latency` are absent from the Phase 4
  artifacts.
- `AegisAI_Runtime-33i`: helper portability now writes
  `helper_signal_availability.json/csv`. Current-host rerun
  `helper_portability_gg-vm_6_8_0_111_generic_20260514T064925Z` remains
  `helper unavailable`; both helper-backed signals are explicitly `excluded`
  for Phase 5 planning.
- `AegisAI_Runtime-8iy`: Tail Guard background demotion now has a dry-run-only
  planner in `bench/scripts/inference_tail_guard_background_demotion_planner.py`
  and report `docs/tail_guard_background_demotion_plan.md`. The current host
  plan found `0/8` affected candidates, rejected unknown and interactive
  processes, recorded rollback capture requirements, and performed no live
  mutation.
- `AegisAI_Runtime-0ry.3`: deferred GPU coordination now has an
  observe/plan-only evidence gate in `docs/gpu_coordination_gate.md` and
  `bench/scripts/gpu_coordination_gate.py`. Verification artifacts are under
  `.cache/aegisai/gpu_coordination_gate/codex_gpu_coordination_gate_20260514T000000Z/`.
- `AegisAI_Runtime-0ry.2`: deferred observability dashboard now has a
  read-only evidence gate in `docs/observability_dashboard_gate.md` and
  `bench/scripts/observability_dashboard_gate.py`. Verification artifacts are
  under
  `.cache/aegisai/observability_dashboard_gate/codex_observability_dashboard_gate_20260514T000000Z/`.
- `AegisAI_Runtime-0ry.4`: deferred online adaptive policy now has a
  shadow-only evidence gate in `docs/adaptive_policy_gate.md` and
  `bench/scripts/adaptive_policy_gate.py`. Verification artifacts are under
  `.cache/aegisai/adaptive_policy_gate/codex_adaptive_policy_gate_20260514T000000Z/`.
