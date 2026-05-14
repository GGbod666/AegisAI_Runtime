# Latest Task List

_Regenerated: 2026-05-13_

`bd` is the source of truth. This file is only the current prioritized todo
queue. Historical evidence belongs in `docs/status.md`,
`docs/acceptance_ledger.md`, and `docs/verification_log.md`.

## Queue

No active queued work is listed in this snapshot. Run `bd ready` for the
current source of truth before starting the next item.

## Ordering Rules

- Finish validation gaps before expanding runtime scope.
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
