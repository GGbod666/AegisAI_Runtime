# Linux Host Checklist

This checklist is the handoff path for Linux validation, live guarded
experiments, and portability checks.

Current baseline:

- helper-backed `offcpu_time` and `io_latency` have been validated on Linux host
  `gg-vm`
- live guarded Inference Tail Guard has effective host-level action evidence but
  no stable MVP benefit yet
- live guarded Tool Call Booster has contract `PASS` but benefit `FAIL`
- future Linux work should continue from `docs/current_status.md` and `bd ready`

## 1. Base Host Checks

Run these first on a Linux validation host:

```bash
uname -a
uname -r
rustc --version
cargo --version
```

Confirm:

- kernel is at least `5.15`
- Rust toolchain is available
- the repository is readable from the Linux filesystem

## 2. Cgroup And Procfs Checks

Confirm the host exposes the Linux surfaces the current design expects:

```bash
mount | grep cgroup
cat /proc/self/cgroup
cat /proc/self/cpuset
grep '^Cpus_allowed_list:' /proc/self/status
cat /sys/devices/system/cpu/online
```

Confirm:

- cgroup v2 is available, or the host layout is at least understood
- `/proc/<pid>/status` exposes `Cpus_allowed_list`
- `/proc/<pid>/cpuset` is readable
- `/sys/devices/system/cpu/online` is readable for affinity planning

## 3. eBPF And Capability Checks

The main runtime daemon should remain rootless. Prepare the host for the narrow
privileged helper instead:

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

If the host is missing these tools, install the Linux-side dependencies before
real probe validation.

For Fedora/openEuler-style hosts:

```bash
dnf install -y bpftool bpftrace clang llvm util-linux
```

For Debian/Ubuntu-style hosts:

```bash
apt-get install -y bpftool bpftrace clang llvm util-linux
```

## 4. Build Verification

From the repository root:

```bash
cargo fmt --all -- --check
cargo test --workspace
cargo clippy --all-targets --all-features -- -D warnings
```

This confirms the Linux host matches the accepted workspace baseline before any
live probe or actuator experiment starts.

## 5. Version Checkpoint

Before each live Linux experiment batch, record a clean version checkpoint:

- confirm the repository root
- confirm branch and HEAD
- confirm whether the tree is dirty
- choose a normalized checkpoint label for the experiment batch

Suggested commands:

```bash
cargo run -p aegisai-git-control -- status --path .
cargo run -p aegisai-git-control -- checkpoint --path . --label "linux experiment baseline"
```

## 6. Safe Runtime Smoke Tests

Start with the safe daemon paths:

```bash
cargo run -p aegisai-runtime-daemon -- --repo-root . --source mock --metadata demo --actuator-backend noop
cargo run -p aegisai-runtime-daemon -- --repo-root . --source linux --metadata procfs --actuator-backend linux-skeleton --allow-partial-probes
```

Confirm:

- the daemon starts cleanly
- `procfs` metadata enrichment works on Linux
- mock mode still produces scenario triggers
- Linux source preflight exits cleanly even if it processes zero live events

Zero events in the Linux source smoke are acceptable; this checks
startup/configuration safety, not benefit.

## 7. Helper Validation

For helper-backed signal validation:

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

Use controlled off-CPU and block I/O workloads. Record conclusions using the
standard buckets:

- `helper unavailable`
- `tracepoint incompatible`
- `no workload events`
- `validated signal`

For portability work, capture the exact tracepoint or field that failed.

## 8. Actuator State Capture Check

Before live host actions:

```bash
cargo test -p aegisai-actuator
```

Focus on:

- original nice capture
- original affinity capture
- online/allowed CPU intersection
- cpuset capture
- rollback audit fields

## 9. Benchmark Entry Conditions

Do not start collecting benchmark numbers until all of the following are true:

- mock mode still passes
- Linux source starts without unsupported-reader failure
- helper readiness is understood for the current host
- actuator rollback paths are auditable
- the target runtime process can be identified by classifier rules
- live experiment window, PID allowlist, and permissions are explicit

## 10. Inference Tail Guard Live Experiment

Use the strict Phase 4 report path:

```bash
AEGISAI_AB_MODES=baseline,noop_observation,dry_run,live_guarded \
AEGISAI_CONFIRM_LIVE_ACTUATOR=1 \
AEGISAI_LIVE_PID_ALLOWLIST=<pid,...> \
  bash bench/scripts/inference_tail_guard_phase4_report.sh
```

Capture at least:

- TTFT
- P95 latency
- P99 latency
- jitter
- trigger count
- rollback count
- live effective action count

Enable affinity only when intended:

```bash
AEGISAI_ENABLE_LIVE_AFFINITY=1
```

The CPU affinity planner intersects `/proc/<pid>/status` `Cpus_allowed_list`
with `/sys/devices/system/cpu/online`; empty intersections should remain a
visible action-effectiveness risk rather than falling back to disallowed CPUs.

## 11. Tool Call Booster Live Experiment

Use the repeated A/B harness:

```bash
AEGISAI_TCB_MODES=baseline,noop_observation,dry_run,live_guarded \
AEGISAI_CONFIRM_LIVE_ACTUATOR=1 \
  bash bench/scripts/tool_call_booster_real_executor_harness.sh
```

Confirm the report includes:

- latency deltas
- trigger counts
- rollback counts
- action error counts
- explicit contract and benefit verdicts
- the `WarmupExecutor` boundary

## 12. Pre-Ollama Preflight

First record toolchain availability:

```bash
bash bench/scripts/toolchain_preflight.sh
```

Before installing Ollama, downloading a model, or creating pressure on a fresh
host, run:

```bash
bash bench/scripts/inference_tail_guard_preflight.sh
```

The script appends a validation-style entry to `docs/verification_log.md` unless
`AEGISAI_VERIFY_LOG` is redirected.

Required gates:

- required Linux surfaces such as `/proc/self/cgroup`, `/proc/self/cpuset`, and
  `Cpus_allowed_list` must be readable
- the mock/noop runtime daemon smoke test must pass

Optional inventory:

- missing optional tools such as `ollama`, `llama.cpp`, `stress-ng`, or `taskset`
  are recorded as `SKIPPED`
- optional tool version/help command failures are recorded as non-blocking

Use this as a safe host-readiness gate before a separate Ollama/model
installation stage or before running a real `ollama`/`llama.cpp` plus
`stress-ng` experiment.
