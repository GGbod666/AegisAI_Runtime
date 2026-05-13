# Debian Systemd Packaging Contract

_Defined: 2026-05-13_

This document is the design contract for `AegisAI_Runtime-ufp.1`. It defines
the first production packaging target and install/service boundaries. It does
not add installer code, maintainer scripts, unit files, or package metadata.

## First Target

The first target is a Debian/Ubuntu system package managed by the system
`systemd` instance.

Supported baseline:

- Debian 12 or Ubuntu 24.04 LTS and later
- Linux kernel `5.15+`
- cgroup v2 or a host layout explicitly accepted by validation
- `systemd` as the service manager
- `bpftrace` available only when helper-backed eBPF signals are enabled

Non-Linux hosts, non-systemd hosts, and hosts without a usable production
profile are unsupported for the package service. They remain valid for source
development and mock/control-plane checks.

## Package Shape

The first package name is `aegisai-runtime`.

Package-owned binaries:

| component | path | owner/mode | purpose |
| --- | --- | --- | --- |
| daemon | `/usr/bin/aegisai-runtime-daemon` | `root:root`, `0755` | rootless runtime control loop |
| eBPF helper | `/usr/lib/aegisai/aegisai-ebpf-helper` | `root:root`, `0755` | fixed helper CLI for readiness, compatibility, and eBPF stream attachment |

The helper is deliberately outside the normal command path. The service passes
its path with `AEGISAI_EBPF_HELPER=/usr/lib/aegisai/aegisai-ebpf-helper` only
when helper-backed probes are enabled.

Package-owned configuration and data paths:

| purpose | path |
| --- | --- |
| production config root | `/etc/aegisai` |
| selected production profile | `/etc/aegisai/configs/profiles/production/` |
| runtime config | `/etc/aegisai/configs/profiles/production/runtime.toml` |
| classifier config | `/etc/aegisai/configs/profiles/production/classifier/process_rules.toml` |
| awareness config | `/etc/aegisai/configs/profiles/production/scenarios/ai_workload_awareness.toml` |
| inference policy config | `/etc/aegisai/configs/profiles/production/scenarios/inference_tail_guard.toml` |
| tool-call policy config | `/etc/aegisai/configs/profiles/production/scenarios/tool_call_booster.toml` |
| safety config | `/etc/aegisai/configs/profiles/production/safety/default.toml` |
| persistent state | `/var/lib/aegisai` |
| runtime directory | `/run/aegisai` |
| daemon verification log | `/var/log/aegisai/runtime-daemon.md` |

The packaged daemon must be started with `--repo-root /etc/aegisai` and
`--config-profile production` so it loads the non-example profile files already
covered by production-profile validation.

## Users And Groups

The package creates a locked system user and group with `sysusers.d`:

- user: `_aegisai`
- primary group: `_aegisai`
- shell: `/usr/sbin/nologin`
- home: no interactive home; package state lives under `/var/lib/aegisai`

The daemon service runs as:

```ini
User=_aegisai
Group=_aegisai
RuntimeDirectory=aegisai
StateDirectory=aegisai
LogsDirectory=aegisai
ConfigurationDirectory=aegisai
```

The package may use `tmpfiles.d` only for directories that are not already
created by `systemd` directory settings or Debian maintainer-script handling.

## Service Contract

The first service unit name is `aegisai-runtime.service`.

Default service posture:

- runs the daemon as `_aegisai`, never as `root`
- starts after local filesystems and journald are available
- writes stdout/stderr to journald
- appends daemon summaries to `/var/log/aegisai/runtime-daemon.md`
- uses `Restart=on-failure`
- uses `NoNewPrivileges=yes` for the rootless daemon path
- does not enable live `linux-command` actions by default
- does not enable helper-backed probes unless the helper privilege path is
  explicitly configured

Contracted `ExecStart` shape:

```ini
ExecStart=/usr/bin/aegisai-runtime-daemon \
  --repo-root /etc/aegisai \
  --config-profile production \
  --source linux \
  --metadata procfs \
  --actuator-backend linux-skeleton \
  --allow-partial-probes \
  --verification-log /var/log/aegisai/runtime-daemon.md
```

The default backend is `linux-skeleton` because unattended packaging must not
modify scheduler state. A live-action unit or drop-in is a separate future
artifact and must still require `--confirm-live-actuator`, a non-empty PID
allowlist, and the existing live-affinity gate for `taskset`.

## Helper Privilege Boundary

The rootless daemon owns config loading, classification, policy selection,
audit summaries, and rollback lease scheduling. It must not receive broad root,
`CAP_SYS_ADMIN`, `CAP_SYS_NICE`, `CAP_BPF`, or `CAP_PERFMON` privileges through
the package service.

The helper boundary is limited to:

- `aegisai-ebpf-helper --check`
- `aegisai-ebpf-helper compatibility [--offcpu] [--io]`
- `aegisai-ebpf-helper stream [--offcpu] [--io] [--pid <pid>] [--process-name <name>]`

The daemon must pass only selectors and fixed signal flags to the helper. The
helper must not accept arbitrary bpftrace source, arbitrary shell commands, or
general root actions.

Helper-backed eBPF mode requires administrator approval because current probes
depend on privileged BPF/perf access. The package may support one of these
validated privilege paths in a later implementation:

- a dedicated root-owned helper service or socket with a narrow request
  protocol, or
- a file-capability/group-restricted helper path that proves the fixed
  bpftrace backend receives only the capabilities it needs on the target
  kernel.

Until one of those paths is implemented and validated, the packaged default
must keep helper-backed probes optional and rely on procfs-derived signals when
`--allow-partial-probes` is set. Installing the package must not make the full
daemon root, setuid root, or a generic privileged command runner.

Expected helper capabilities when helper-backed probes are enabled:

- Prefer Linux `CAP_BPF` and `CAP_PERFMON` on kernels/toolchains that support
  them.
- Use broader `CAP_SYS_ADMIN` only as an explicitly documented fallback for
  kernels or bpftrace builds that cannot operate with the narrower capability
  set.
- Do not grant `CAP_SYS_NICE` to the helper; scheduler actions belong to the
  daemon-controlled actuator path and remain separately gated.

## Unsupported Prerequisites

Installer or service preflight must fail before enabling or starting the
service when any required default-service prerequisite is missing:

- non-Linux OS
- missing system `systemd`
- kernel below `5.15`
- missing `/etc/aegisai/configs/profiles/production/`
- invalid production config schema or cross-file safety validation
- unwritable `/var/log/aegisai` for `_aegisai`

Helper-enabled mode must additionally fail before start when:

- `aegisai-ebpf-helper --check` fails
- helper compatibility reports `helper unavailable`
- helper compatibility reports `tracepoint incompatible`
- the configured bpftrace backend cannot attach with the approved privilege
  path

The default procfs/partial-probe mode must not fail solely because bpftrace or
helper-backed signals are unavailable.

## Rollback And Uninstall

Runtime rollback:

- Scheduler rollback is owned by the daemon lease/audit path while the daemon is
  running.
- The default package service uses `linux-skeleton`, so there are no live host
  writes to roll back.
- A future live-action service/drop-in must not claim unattended stop-time
  rollback until the daemon has signal-aware graceful shutdown coverage. Until
  then, live mode remains an operator-run path with explicit PID allowlist and
  audit logs.

Package upgrade rollback:

- Debian upgrade hooks must stop/restart only the package-owned service unit.
- Failed upgrades must leave existing `/etc/aegisai` profile files and
  `/var/log/aegisai` logs intact.
- Package maintainer scripts must not rewrite Beads issue data, Git remotes, or
  project working-tree files.

Uninstall:

- `remove` stops and disables `aegisai-runtime.service`, removes package-owned
  binaries and unit files, and leaves `/etc/aegisai`, `/var/lib/aegisai`, and
  `/var/log/aegisai` for operator inspection.
- `purge` may remove package-owned config, state, and logs after the service is
  stopped, but must not remove unrelated files under parent directories.
- The locked `_aegisai` system user/group may remain after purge unless the
  platform packaging policy explicitly requires safe removal.

## References Consulted

- Debian Policy Manual, system service packages:
  `https://www.debian.org/doc/debian-policy/ch-opersys.html`
- Debian `dh_installsystemd` manual:
  `https://manpages.debian.org/bookworm-backports/debhelper/dh_installsystemd.1.en.html`
- `systemd.exec(5)` execution environment and service directory settings:
  `https://man7.org/linux/man-pages/man5/systemd.exec.5.html`
- `systemd.service(5)` service unit behavior:
  `https://man7.org/linux/man-pages/man5/systemd.service.5.html`
- `sysusers.d(5)` system user/group allocation:
  `https://www.freedesktop.org/software/systemd/man/latest/sysusers.d.html`
- `tmpfiles.d(5)` directory/file creation policy:
  `https://man7.org/linux/man-pages/man5/tmpfiles.d.5.html`
- `capabilities(7)` Linux capability boundaries:
  `https://man7.org/linux/man-pages/man7/capabilities.7.html`
