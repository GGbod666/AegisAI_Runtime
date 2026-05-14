# Debian Systemd Packaging

This directory implements the first production packaging target from
`docs/packaging_contract.md`: a Debian/Ubuntu systemd deployment for
`aegisai-runtime`.

## Artifacts

- `aegisai-runtime.service` runs `aegisai-runtime-daemon` as `_aegisai` with
  the non-mutating `linux-skeleton` backend.
- `aegisai-runtime.sysusers` defines the locked `_aegisai` service user/group.
- `aegisai-runtime.tmpfiles` creates package-owned state, log, and runtime
  directories when they are not created by systemd itself.
- `install.sh` stages or installs binaries, service files, and the selected
  production profile.
- `smoke.sh` runs the installer in a temporary `--destdir` and validates the
  staged layout without touching host system directories.

## Operator Flow

Build the release binaries first:

```bash
cargo build --release -p aegisai-runtime-daemon -p aegisai-ebpf-helper
```

Install with an explicit production profile source:

```bash
sudo packaging/debian-systemd/install.sh install \
  --profile-source /path/to/production-profile
```

The profile source must contain these non-example files:

- `runtime.toml`
- `classifier/process_rules.toml`
- `scenarios/ai_workload_awareness.toml`
- `scenarios/inference_tail_guard.toml`
- `scenarios/tool_call_booster.toml`
- `safety/default.toml`

Use `remove` to stop/disable the service and remove package-owned binaries and
unit files while keeping `/etc/aegisai`, `/var/lib/aegisai`, and
`/var/log/aegisai`. Use `purge` only when package-owned config, state, and logs
should also be removed.

## Dry Run

The dry-run path stages into a destination root and skips `systemctl`,
`systemd-sysusers`, `systemd-tmpfiles`, ownership changes, and service starts:

```bash
packaging/debian-systemd/install.sh dry-run \
  --destdir /tmp/aegisai-runtime-package-root \
  --profile-source /path/to/production-profile
```

Run the maintained smoke path:

```bash
bash packaging/debian-systemd/smoke.sh
```

To validate with real built binaries instead of smoke stubs:

```bash
cargo build -p aegisai-runtime-daemon -p aegisai-ebpf-helper
AEGISAI_PACKAGING_DAEMON_BIN=target/debug/aegisai-runtime-daemon \
AEGISAI_PACKAGING_HELPER_BIN=target/debug/aegisai-ebpf-helper \
  bash packaging/debian-systemd/smoke.sh
```
