# Linux Validation

This file owns Linux host setup, preflight, helper validation, and live guarded
experiment checklists. Product strategy lives in `docs/strategy.md`; current
artifact status lives in `docs/status.md`.

## Host Split

Development can happen on Windows or any non-production host for architecture,
crate integration, mock-source development, control-loop verification,
configuration, and docs.

Linux validation is required for:

- real `/proc` metadata enrichment
- scheduler-state capture and actuator rollback safety
- real probe-backed event ingestion
- eBPF helper loading and validation
- scheduling actions such as `nice` and `taskset`
- benchmark data collection and final experiment results

Current baseline:

- helper-backed `offcpu_time` and `io_latency` have been validated on Linux host
  `gg-vm`
- live guarded Inference Tail Guard has effective host-level action evidence and
  stable MVP benefit for `live_guarded_phase4_sample_sizing_20260511T000000Z`
- live guarded Tool Call Booster has contract `PASS` and benefit `PASS` for
  `codex_fixed_work_guarded_final_20260511T141942Z`; the older
  `live_guarded_tcb_stable_executor_20260511T000000Z` run remains a historical
  non-controlled workload `FAIL`

## Base Host Checks

Run these first:

```bash
uname -a
uname -r
rustc --version
cargo --version
```

Confirm kernel `5.15+`, Rust availability, and repository readability from a
Linux filesystem.

## Cgroup And Procfs Checks

```bash
mount | grep cgroup
cat /proc/self/cgroup
cat /proc/self/cpuset
grep '^Cpus_allowed_list:' /proc/self/status
cat /sys/devices/system/cpu/online
```

Confirm cgroup v2 or a known host layout, readable `/proc/<pid>/status`
`Cpus_allowed_list`, readable `/proc/<pid>/cpuset`, and readable online CPU
topology.

## eBPF And Capability Checks

The main runtime daemon should remain rootless. Prepare the narrow privileged
helper instead:

```bash
sysctl kernel.unprivileged_bpf_disabled || true
bpftool version || true
which bpftool || true
which aegisai-ebpf-helper || true
aegisai-ebpf-helper --check || true
which bpftrace || true
which clang || true
which llc || true
```

Install missing Linux dependencies before real probe validation:

```bash
dnf install -y bpftool bpftrace clang llvm util-linux
apt-get install -y bpftool bpftrace clang llvm util-linux
```

Use the command matching the host distribution.

## Build Verification

From the repository root:

```bash
cargo fmt --all -- --check
cargo test --workspace
cargo clippy --all-targets --all-features -- -D warnings
```

This confirms the Linux host matches the accepted workspace baseline before live
probe or actuator experiments.

## Version Checkpoint

Before each live Linux experiment batch:

```bash
cargo run -p aegisai-git-control -- status --path .
cargo run -p aegisai-git-control -- checkpoint --path . --label "linux experiment baseline"
```

Record repository root, branch, HEAD, dirty state, and a normalized checkpoint
label.

## Safe Runtime Smoke Tests

```bash
cargo run -p aegisai-runtime-daemon -- --repo-root . --source mock --metadata demo --actuator-backend noop
cargo run -p aegisai-runtime-daemon -- --repo-root . --source linux --metadata procfs --actuator-backend linux-skeleton --allow-partial-probes
bash bench/scripts/linux_source_ingestion_smoke.sh
```

Confirm daemon startup, procfs metadata enrichment, mock scenario triggers, and
Linux source preflight. Zero events in the direct Linux source preflight are
acceptable because that command checks startup/configuration safety, not
ingestion.

The controlled ingestion smoke is the Linux source validation gate for procfs
event ingestion. It starts short-lived CPU worker processes, writes a temporary
runtime config scoped to those worker PIDs, runs the daemon with
`linux-skeleton` by default, and requires the summary to show
`processed_events > 0` plus `signal_observations` for at least one of
`run_queue_delay`, `cpu_migration`, or `major_page_fault`. It never uses the
live `linux-command` backend. To exercise the command-backed audit path without
host scheduler writes, use:

```bash
AEGISAI_LINUX_SOURCE_SMOKE_BACKEND=linux-command-dry-run \
  bash bench/scripts/linux_source_ingestion_smoke.sh
```

Exit states are intentionally distinct:

- `0`: PASS. The daemon processed at least one procfs-derived signal from the
  controlled PID allowlist.
- `1`: FAIL. The host looked capable of producing procfs deltas, but the daemon
  command failed or the summary did not contain the required ingestion evidence.
- `77`: SKIPPED. The host is not Linux, procfs is not mounted/readable, worker
  counters under `/proc/<pid>/schedstat`, `/proc/<pid>/sched`, or
  `/proc/<pid>/stat` are unavailable, or the controlled workers do not produce a
  positive scheduler/fault counter delta during the precheck window.

Useful bounded overrides:

```bash
AEGISAI_LINUX_SOURCE_SMOKE_WORKERS=8 \
AEGISAI_LINUX_SOURCE_SMOKE_MAX_EVENTS=8 \
AEGISAI_LINUX_SOURCE_SMOKE_POLL_TIMEOUT_MS=500 \
  bash bench/scripts/linux_source_ingestion_smoke.sh
```

## Helper Validation

For helper-backed signals:

```bash
AEGISAI_EBPF_HELPER=/path/to/aegisai-ebpf-helper \
AEGISAI_BPFTRACE=/usr/bin/bpftrace \
  cargo run -p aegisai-runtime-daemon -- \
    --repo-root . \
    --source linux \
    --metadata procfs \
    --actuator-backend linux-skeleton \
    --allow-partial-probes
```

Use controlled off-CPU and block I/O workloads. Record conclusions with these
buckets:

- `helper unavailable`
- `tracepoint incompatible`
- `no workload events`
- `validated signal`

For portability work, capture the exact tracepoint or field that failed.

## Actuator State Capture Check

Before live host actions:

```bash
cargo test -p aegisai-actuator
```

Focus on original nice capture, original affinity capture, online/allowed CPU
intersection, cpuset capture, rollback audit fields, and explicit live guards.

Live cpuset and background-isolation writes are not part of the current live
action set. Before any implementation enables them, validation must prove:

- current `live_guarded` contracts still keep cpuset disabled by default
- policy/config alone cannot enable cgroupfs writes
- the host uses a known cgroup v2 layout with an administrator-created
  AegisAI-owned subtree
- the proposed applier can read and restore original membership, `cpuset.cpus`,
  `cpuset.mems`, and any touched `cpu.max`
- dry-run plans reject unsafe roots, unknown classifications, empty CPU sets,
  missing rollback state, and overbroad process sets with explicit reasons

The first accepted artifact for this area should be dry-run only. A later live
artifact must name every cgroup file touched, every pid/tid moved, rollback
result, and manual restore instruction for any failed restore.

## Benchmark Entry Conditions

Do not collect benchmark numbers until:

- mock mode passes
- Linux source starts without unsupported-reader failure
- helper readiness is understood for the current host
- actuator rollback paths are auditable
- the target runtime process can be identified by classifier rules
- live experiment window, PID allowlist, and permissions are explicit
- cpuset/background isolation is either contractually disabled or running only
  in the reviewed dry-run planner

## Live Guarded Experiments

Inference Tail Guard:

```bash
AEGISAI_AB_MODES=baseline,noop_observation,dry_run,live_guarded \
AEGISAI_CONFIRM_LIVE_ACTUATOR=1 \
AEGISAI_LIVE_PID_ALLOWLIST=<pid,...> \
  bash bench/scripts/inference_tail_guard_phase4_report.sh
```

Enable affinity only when intended:

```bash
AEGISAI_ENABLE_LIVE_AFFINITY=1
```

The CPU affinity planner intersects `/proc/<pid>/status` `Cpus_allowed_list`
with `/sys/devices/system/cpu/online`; empty intersections should remain a
visible action-effectiveness risk rather than falling back to disallowed CPUs.

Tool Call Booster:

```bash
AEGISAI_TCB_MODES=baseline,noop_observation,dry_run,live_guarded \
AEGISAI_CONFIRM_LIVE_ACTUATOR=1 \
  bash bench/scripts/tool_call_booster_real_executor_harness.sh
```

Reports should include latency deltas, trigger counts, rollback counts, action
errors, stage-attributed effective scheduler action counts, explicit
contract/benefit verdicts, and the `WarmupExecutor` boundary.

## Pre-Ollama Preflight

```bash
bash bench/scripts/toolchain_preflight.sh
bash bench/scripts/inference_tail_guard_preflight.sh
```

The preflight script appends a validation-style entry to
`docs/verification_log.md` unless `AEGISAI_VERIFY_LOG` is redirected. It checks
safe host readiness before a separate Ollama/model installation stage or before
running real `ollama`/`llama.cpp` plus `stress-ng` experiments.
