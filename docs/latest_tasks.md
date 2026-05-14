# Latest Task List

_Regenerated: 2026-05-13_

`bd` is the source of truth. This file is only the current prioritized todo
queue. Historical evidence belongs in `docs/status.md`,
`docs/acceptance_ledger.md`, and `docs/verification_log.md`.

## Queue

### 1. Implement Daemon/Helper Packaging

- Issue: `AegisAI_Runtime-ufp`
- Priority: `P4`
- Why now: production config/profile work is complete, but the daemon/helper
  service and package path is still missing.
- Scope:
  - implement the Debian/Ubuntu systemd package/service path in
    `docs/packaging_contract.md`
  - keep `aegisai-runtime-daemon` rootless under `_aegisai`
  - keep `aegisai-ebpf-helper` as a separate privileged helper boundary
  - install the selected production profile under
    `/etc/aegisai/configs/profiles/production/`
  - fail unsupported prerequisites clearly
  - cover rollback and uninstall behavior
- Verify:
  - shell syntax for installer/service scripts
  - dry-run or VM smoke path for package/service behavior
  - `bd lint`
  - `git diff --check`

### 2. Close Deferred Extension Parent

- Issue: `AegisAI_Runtime-0ry`
- Priority: `P4`
- Why now: close only after the extension planning bucket is split.
- Scope:
  - confirm dashboard, GPU coordination, and online adaptive policy are separate
    deferred work items:
    `AegisAI_Runtime-0ry.2`, `AegisAI_Runtime-0ry.3`, and
    `AegisAI_Runtime-0ry.4`
  - confirm `AegisAI_Runtime-0ry.1` is closed
  - keep them behind production packaging, helper portability, and safety gates
- Verify:
  - `bd show AegisAI_Runtime-0ry`
  - `bd ready`

## Blocked Deferred Extensions

These are intentionally blocked behind `AegisAI_Runtime-ufp` and should not
enter active implementation until production packaging is complete.

- `AegisAI_Runtime-0ry.2` — observability dashboard. Prerequisites,
  non-goals, safety evidence, benchmark evidence, and verification gate are
  recorded in the issue.
- `AegisAI_Runtime-0ry.3` — GPU coordination. Prerequisites, non-goals, safety
  evidence, benchmark evidence, and verification gate are recorded in the
  issue.
- `AegisAI_Runtime-0ry.4` — online adaptive policy. Prerequisites, non-goals,
  safety evidence, benchmark evidence, and verification gate are recorded in
  the issue.

## Ordering Rules

- Finish validation gaps before expanding runtime scope.
- Keep production packaging before deferred dashboard/GPU/adaptive extensions.
- Do not add new runtime behavior while doing deferred-extension planning.
