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

- `AegisAI_Runtime-4cc`: build a Tail Guard attribution report that quantifies
  how much of current tail latency is scheduler/offcpu/migration/page-fault/I/O
  attributable before stronger isolation work starts.
- `AegisAI_Runtime-33i`: restore or explicitly classify helper-backed
  `offcpu_time` and `io_latency` evidence on the current host.
- `AegisAI_Runtime-8iy`: add a dry-run background demotion planner for
  Tail Guard, with bounded affected sets and explicit rejection of unknown or
  interactive-sensitive processes.

Blocked follow-up:

- `AegisAI_Runtime-20w`: implement the guarded owned-cgroup isolation applier.
  It depends on `AegisAI_Runtime-4cc` and `AegisAI_Runtime-8iy`.
- `AegisAI_Runtime-t49`: run the Phase 5 Tail Guard isolation A/B proof. It
  depends on `AegisAI_Runtime-4cc`, `AegisAI_Runtime-33i`,
  `AegisAI_Runtime-8iy`, and `AegisAI_Runtime-20w`.

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
