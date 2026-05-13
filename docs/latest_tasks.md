# Latest Task List

_Regenerated: 2026-05-13_

`bd` is the source of truth. This file is only the current prioritized todo
queue. Historical evidence belongs in `docs/status.md`,
`docs/acceptance_ledger.md`, and `docs/verification_log.md`.

## Queue

### 1. Normalize Runtime Daemon Help Exit Behavior

- Issue: `AegisAI_Runtime-dxh`
- Priority: `P3`
- Why now: `aegisai-runtime-daemon --help` prints usage but exits `1`.
- Scope:
  - make explicit `--help` print the current usage text and exit `0`
  - keep invalid or incomplete CLI arguments exiting nonzero
  - avoid unrelated CLI text churn
- Verify:
  - targeted runtime daemon CLI tests
  - `cargo test -p aegisai-runtime-daemon`
  - `git diff --check`

### 2. Audit High-Degree Runtime Hotspot Coverage

- Issue: `AegisAI_Runtime-76k`
- Priority: `P3`
- Why now: graph review identified high-degree or bridge points where future
  behavior changes carry outsized risk.
- Scope:
  - review direct coverage for `CliConfig::parse_with_env`,
    `build_linux_rollback_report`, `BpfTracePipe::start`,
    `LinuxProbeDriver::poll_events`, `RuntimeOrchestrator::process_event`, and
    the large source/config/backend/bench script files
  - add targeted tests only where coverage is truly missing
  - record any remaining decomposition decision without broad cleanup
- Verify:
  - targeted crate or script tests for any added coverage
  - `cargo test --workspace` or a narrower justified command
  - `git diff --check`

### 3. Implement Daemon/Helper Packaging

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

### 4. Split Deferred Extensions Into Evidence-Gated Work

- Issue: `AegisAI_Runtime-0ry.1`
- Priority: `P4`
- Why now: dashboard, GPU coordination, and online adaptive policy should not
  remain one vague future bucket.
- Scope:
  - create separate future work items for dashboard, GPU coordination, and
    online adaptive policy
  - state prerequisites, non-goals, safety evidence, benchmark evidence, and
    verification gate for each item
  - avoid runtime code changes
- Verify:
  - `bd lint`
  - docs-only review
  - `git diff --check`

### 5. Close Deferred Extension Parent

- Issue: `AegisAI_Runtime-0ry`
- Priority: `P4`
- Blocked by: `AegisAI_Runtime-0ry.1`
- Why now: close only after the extension planning bucket is split.
- Scope:
  - confirm dashboard, GPU coordination, and online adaptive policy are separate
    deferred work items
  - keep them behind production packaging, helper portability, and safety gates
- Verify:
  - `bd show AegisAI_Runtime-0ry`
  - `bd ready`

## Ordering Rules

- Finish validation gaps before expanding runtime scope.
- Keep production packaging before deferred dashboard/GPU/adaptive extensions.
- Do not add new runtime behavior while doing deferred-extension planning.
