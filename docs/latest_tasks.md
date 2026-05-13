# Latest Task List

_Regenerated: 2026-05-13_

This is the current executable queue, not a historical gap ledger. `bd` is the
source of truth for task state; this file mirrors the latest open work and
points to the durable evidence that closed the previous parent gaps.

## Current Repository State

- `bd` after this status sync: `70` total issues, `3` open, `0` in progress,
  `67` closed.
- Current open issues:
  - `AegisAI_Runtime-ufp`
  - `AegisAI_Runtime-0ry`
  - `AegisAI_Runtime-0ry.1`
- `AegisAI_Runtime-51c` is closed. Its helper portability acceptance is covered
  by closed children `51c.1` through `51c.4`.
- `AegisAI_Runtime-cqv` is closed. Its production config profile acceptance is
  covered by closed children `cqv.1` through `cqv.3`.
- `AegisAI_Runtime-8le` is closed. Beads Dolt sync is configured as a local-only
  filesystem remote and `bd dolt push` succeeds.

## Current Evidence Snapshot

The latest accepted runtime and benchmark baseline remains the one summarized
in `docs/status.md`:

- Rust formatting, workspace tests, workspace clippy, Python unittest
  discovery, shell syntax checks, workspace/toolchain/inference preflights, and
  `bd lint` have accepted `PASS` records.
- Controlled Linux source ingestion smoke records nonzero procfs-derived daemon
  events without live scheduler writes.
- Helper-backed `offcpu_time` and `io_latency` portability has two `gg-vm`
  kernel profiles, compatibility taxonomy, raw event counts, normalized daemon
  counts, and a strict result-layer regression test.
- Named production config profiles support CLI/env/default selection, strict
  schema validation, and cross-file safety validation while preserving the local
  demo example path.
- Debian/Ubuntu systemd packaging is specified in
  `docs/packaging_contract.md`, but installer/service implementation is still
  open under `AegisAI_Runtime-ufp`.

Audit boundaries that still matter:

- Direct Linux source preflight is a startup/partial-probe check. Use
  `bash bench/scripts/linux_source_ingestion_smoke.sh` for ingestion proof.
- Inference preflight does not run a model or start stress load.
- Upstream `bd preflight` still prints Beads' own Go/Nix template; use
  `bash bench/scripts/project_preflight.sh` for this Rust workspace.
- Live `linux-command` can change real process scheduler state. Keep
  `--confirm-live-actuator` and PID allowlists mandatory.
- Live cpuset/cgroup writes remain disabled.

## Priority Rule

1. **P3/P4 production handoff work**: packaging/service implementation comes
   before deferred product extensions.
2. **P4 deferred extension planning**: dashboard, GPU coordination, and online
   adaptive policy stay behind production config, helper portability,
   packaging, and cpuset/background safety gates.
3. **No new runtime behavior** should be added while splitting deferred
   extension planning issues.

## Ready Work

### 1. Implement Daemon/Helper Packaging

- Issue: `AegisAI_Runtime-ufp`
- Status: `OPEN`
- Current boundary: `AegisAI_Runtime-ufp.1` is closed and
  `docs/packaging_contract.md` defines the first target as Debian/Ubuntu
  systemd.
- Scope:
  - implement the package/service path described by the contract
  - keep `aegisai-runtime-daemon` rootless under `_aegisai`
  - keep `aegisai-ebpf-helper` as the separate privileged helper boundary
  - install the selected production profile under
    `/etc/aegisai/configs/profiles/production/`
  - fail unsupported prerequisites clearly
  - cover rollback and uninstall behavior
- Verification:
  - shell syntax for installer/service scripts
  - dry-run or VM smoke path for package/service behavior
  - `bd lint`
  - `git diff --check`

### 2. Split Deferred Extensions Into Evidence-Gated Work

- Issue: `AegisAI_Runtime-0ry.1`
- Parent: `AegisAI_Runtime-0ry`
- Status: `OPEN`
- Scope:
  - create separate future work items for dashboard, GPU coordination, and
    online adaptive policy
  - state prerequisites, non-goals, required safety evidence, required
    benchmark evidence, and verification gate for each item
  - avoid runtime code changes
- Verification:
  - `bd lint`
  - docs-only review
  - `git diff --check`

### 3. Close Deferred Extension Parent After Split

- Issue: `AegisAI_Runtime-0ry`
- Status: `OPEN`
- Dependency: complete `AegisAI_Runtime-0ry.1`
- Scope:
  - confirm dashboard, GPU coordination, and online adaptive policy no longer
    live in one vague bucket
  - keep all extension items deferred behind production config, helper
    portability, packaging, and cpuset/background safety evidence
- Verification:
  - `bd show AegisAI_Runtime-0ry`
  - `bd ready`

## Recently Closed Parent Gaps

- `AegisAI_Runtime-51c` — helper portability across Linux kernels. Closed after
  compatibility taxonomy, two-kernel helper matrix, controlled Linux ingestion
  smoke, and startup failure taxonomy coverage were complete.
- `AegisAI_Runtime-cqv` — production config profiles and schema validation.
  Closed after profile selection, strict production schema validation, and
  cross-file safety validation were complete.
- `AegisAI_Runtime-8le` — Beads Dolt remote sync. Closed after local-only
  filesystem sync policy was configured and `bd dolt push` succeeded.
