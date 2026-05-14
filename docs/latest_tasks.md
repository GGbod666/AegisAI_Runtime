# Latest Task List

_Regenerated: 2026-05-13_

`bd` is the source of truth. This file is only the current prioritized todo
queue. Historical evidence belongs in `docs/status.md`,
`docs/acceptance_ledger.md`, and `docs/verification_log.md`.

## Queue

Production packaging is complete, so the deferred extension planning items are
now unblocked. They remain future work and must still pass their own evidence
gates before any runtime behavior is added.

### 1. Evidence-Gate Deferred Online Adaptive Policy

- Issue: `AegisAI_Runtime-0ry.4`
- Priority: `P4`
- Why now: production packaging is complete, but adaptive policy remains a
  high-risk extension that must start in shadow mode with operator approval
  gates.
- Scope:
  - keep the first slice suggestion/shadow-only
  - prove safety invariants, drift checks, bounded state retention, rollback,
    and freeze behavior before any live mutation
  - compare against the existing static policy baseline
- Verify:
  - deterministic replay tests
  - safety invariant tests
  - shadow-mode smoke
  - benchmark report with artifact paths
  - `bd lint`
  - `git diff --check`

### 2. Evidence-Gate Deferred GPU Coordination

- Issue: `AegisAI_Runtime-0ry.3`
- Priority: `P4`
- Why now: production packaging is complete, but GPU coordination requires
  explicit device isolation, privilege, fallback, and benchmark evidence.
- Scope:
  - keep the first slice observe/plan-only
  - define NVIDIA/non-NVIDIA scope and unsupported-host behavior
  - require deny-by-default live actions and target allowlists before any
    mutation path
- Verify:
  - telemetry parser tests
  - unsupported-host smoke
  - dry-run planner proof on a GPU host
  - safety rejection matrix
  - benchmark report with artifact paths
  - `bd lint`
  - `git diff --check`

### 3. Evidence-Gate Deferred Observability Dashboard

- Issue: `AegisAI_Runtime-0ry.2`
- Priority: `P4`
- Why now: production packaging is complete, but the dashboard must stay
  read-only and consume stable telemetry/artifacts instead of becoming an
  actuator or source of benefit truth.
- Scope:
  - consume runtime audit output, verification artifacts, and stable telemetry
  - keep dashboard mode read-only
  - exclude live policy/profile editing, helper control, and scheduler actions
- Verify:
  - focused parser/export tests
  - local smoke against recorded artifacts
  - docs/status update
  - `bd lint`
  - `git diff --check`

## Ordering Rules

- Finish validation gaps before expanding runtime scope.
- Keep production packaging before deferred dashboard/GPU/adaptive extensions.
- Do not add new runtime behavior while doing deferred-extension planning.
