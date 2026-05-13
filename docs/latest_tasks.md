# Latest Task List

_Regenerated: 2026-05-13_

This is the current executable queue, not a historical gap ledger. `bd` is the
source of truth for task state; this file mirrors the latest open work and
points to the durable evidence that closed the previous parent gaps.

## Current Repository State

- `bd` after this audit sync: `74` total issues, `6` open, `0` in progress,
  `68` closed.
- Current open issues:
  - `AegisAI_Runtime-3gz`
  - `AegisAI_Runtime-dxh`
  - `AegisAI_Runtime-76k`
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
- The 2026-05-13 system audit reran non-live gates:
  `cargo fmt --all -- --check`, `cargo check --workspace`,
  `cargo test --workspace`, `cargo clippy --all-targets --all-features -- -D warnings`,
  Tool Call Booster Python tests, bench script Python tests, shell syntax,
  `bash bench/scripts/project_preflight.sh --check`, `bd lint`, and
  `git diff --check`.
- The same audit confirmed design alignment with the documented
  `collector -> classifier -> policy_engine -> actuator -> metrics` route,
  rootless daemon/helper split, bounded action boundary, and strict benefit
  gates.
- Controlled Linux source ingestion smoke records nonzero procfs-derived daemon
  events without live scheduler writes.
- Fresh current-host helper portability validation did not pass:
  `bench/scripts/helper_portability_smoke.sh` reported final bucket
  `helper unavailable` under
  `.cache/aegisai/helper_portability/helper_portability_gg-vm_6_8_0_111_generic_20260513T070947Z`.
  Historical helper-backed signal evidence remains documented, but current-host
  revalidation is now tracked by `AegisAI_Runtime-3gz`.
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
2. **P2/P3 audit follow-ups**: restore or explicitly explain current-host
   helper validation, normalize CLI help behavior, and decide direct coverage
   for high-degree hotspots before expanding runtime scope.
3. **P4 deferred extension planning**: dashboard, GPU coordination, and online
   adaptive policy stay behind production config, helper portability,
   packaging, and cpuset/background safety gates.
4. **No new runtime behavior** should be added while splitting deferred
   extension planning issues.

## Ready Work

### 1. Revalidate Helper-Backed Signals On Current Host

- Issue: `AegisAI_Runtime-3gz`
- Status: `OPEN`
- Current boundary: the 2026-05-13 audit run on `gg-vm` kernel
  `6.8.0-111-generic` failed with final bucket `helper unavailable`.
- Scope:
  - restore the approved helper/bpftrace privilege path, or document the
    current host as intentionally unable to provide helper-backed signals
  - rerun `bash bench/scripts/helper_portability_smoke.sh`
  - record the final bucket, artifact path, kernel, distro, bpftrace/helper
    diagnostics, raw helper event counts, and normalized daemon event counts
- Verification:
  - `bash bench/scripts/helper_portability_smoke.sh`
  - `bd lint`
  - `git diff --check`

### 2. Normalize Runtime Daemon Help Exit Behavior

- Issue: `AegisAI_Runtime-dxh`
- Status: `OPEN`
- Scope:
  - make explicit `--help` print the current usage text and exit `0`
  - preserve nonzero exits for invalid or incomplete CLI arguments
  - keep the usage text stable unless tests require a precise correction
- Verification:
  - targeted runtime daemon CLI tests
  - `cargo test -p aegisai-runtime-daemon`
  - `git diff --check`

### 3. Audit High-Degree Runtime Hotspot Coverage

- Issue: `AegisAI_Runtime-76k`
- Status: `OPEN`
- Scope:
  - review direct coverage for `CliConfig::parse_with_env`,
    `build_linux_rollback_report`, `BpfTracePipe::start`,
    `LinuxProbeDriver::poll_events`, `RuntimeOrchestrator::process_event`, and
    large source/config/backend/bench script files
  - add targeted tests only where risk is real and coverage is missing
  - record decomposition decisions without broad cleanup
- Verification:
  - targeted crate or script tests for any added coverage
  - `cargo test --workspace` or a narrower justified command
  - `git diff --check`

### 4. Implement Daemon/Helper Packaging

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

### 5. Split Deferred Extensions Into Evidence-Gated Work

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

### 6. Close Deferred Extension Parent After Split

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
