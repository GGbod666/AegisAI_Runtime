# Linux VM Checklist

This checklist is the handoff path for the first system-level validation run on Linux.

The current assumption is:

- development continues on Windows
- system validation happens in a Linux VM
- the first live validation target is `AI Workload Awareness -> Inference Tail Guard`

## 1. Base Host Checks

Run these first after copying the repository into the Linux VM:

```bash
uname -a
uname -r
rustc --version
cargo --version
```

Confirm:

- kernel is at least `5.15`
- Rust toolchain is available
- the repository is readable from the VM filesystem

## 2. Cgroup And Procfs Checks

Confirm the host exposes the Linux surfaces the current design expects:

```bash
mount | grep cgroup
cat /proc/self/cgroup
cat /proc/self/cpuset
grep '^Cpus_allowed_list:' /proc/self/status
```

Confirm:

- cgroup v2 is available, or the host layout is at least understood
- `/proc/<pid>/status` exposes `Cpus_allowed_list`
- `/proc/<pid>/cpuset` is readable

## 3. eBPF And Capability Checks

The current runtime is still probe-reader skeleton code, but the VM should be prepared now:

```bash
sysctl kernel.unprivileged_bpf_disabled || true
bpftool version || true
which bpftool || true
which clang || true
which llc || true
```

If the VM is missing these tools, install the Linux-side dependencies before real probe wiring.

For Fedora/openEuler-style hosts, the required preflight tools map to:

```bash
dnf install -y bpftool clang llvm util-linux
```

For Debian/Ubuntu-style hosts, use:

```bash
apt-get install -y bpftool clang llvm util-linux
```

## 4. Build Verification

From the repository root:

```bash
cargo check
cargo test
cargo clippy --all-targets --all-features -- -D warnings
cargo fmt --all -- --check
```

This confirms the Linux VM matches the Windows-side verified baseline before any probe or syscall work starts.

## 4.5 Version Checkpoint

Before the first live Linux experiment, record a clean version checkpoint:

- confirm the repository root
- confirm branch and HEAD
- confirm whether the tree is dirty
- choose a normalized checkpoint label for the experiment batch

The current codebase now has a dedicated `git_control` module for this workflow.

Suggested commands:

```bash
cargo run -p aegisai-git-control -- status --path .
cargo run -p aegisai-git-control -- checkpoint --path . --label "linux vm baseline"
```

## 5. Runtime Baseline Smoke Tests

Start with the same safe daemon paths we already use on Windows:

```bash
cargo run -p aegisai-runtime-daemon -- --repo-root . --source mock --metadata demo --actuator-backend noop
cargo run -p aegisai-runtime-daemon -- --repo-root . --source mock --metadata procfs --actuator-backend noop
```

Confirm:

- the daemon starts cleanly
- `procfs` metadata enrichment works on Linux
- the control loop still produces scenario triggers in mock mode

## 6. Linux Source Skeleton Check

Before real probe wiring, validate the current Linux source path and probe planning:

```bash
cargo run -p aegisai-runtime-daemon -- --repo-root . --source linux --metadata procfs --actuator-backend linux-skeleton
```

Expected current result:

- the process either exits with a clear preflight failure, or continues in planning-only mode if partial probes are allowed
- the output reflects planned probes and runtime-only signals
- preflight should validate tracefs, tracepoint availability, and kprobe symbol visibility

This is the expected pre-driver baseline.

## 7. Actuator State Capture Check

Once on Linux, verify that the planned executor can at least capture original process state through `/proc` before real syscalls are wired:

```bash
cargo test -p aegisai-actuator
```

Focus on:

- original nice capture
- original affinity capture
- cpuset capture
- rollback audit fields

## 8. First Live Integration Targets

When the Linux VM is ready, implement and validate in this order:

1. real `LinuxProbeDriver` attach / poll / stop lifecycle
2. real `LinuxSyscallApplier` plus Linux state capture / restore
3. `runtime_daemon --source linux --metadata procfs --actuator-backend linux-skeleton`
4. first `Inference Tail Guard` live demo

## 9. Benchmark Entry Conditions

Do not start collecting benchmark numbers until all of the following are true:

- mock mode still passes
- Linux source starts without unsupported-reader failure
- actuator rollback paths are auditable
- the target runtime process can be identified by classifier rules

## 10. First Benchmark Shape

The first live benchmark should stay narrow:

- one target runtime: `ollama` or `llama.cpp`
- one interference source: CPU pressure first
- one scenario: `inference_tail_guard`
- one comparison: baseline vs bounded boost

Capture at least:

- TTFT
- P95 latency
- P99 latency
- jitter
- rollback count

## 11. Pre-Ollama Inference Tail Guard Demo Preflight

First record toolchain availability:

```bash
bash bench/scripts/toolchain_preflight.sh
```

If this fails only because required commands such as `bpftool`, `clang`, `llc`, or `taskset` are missing, install the required packages listed in that log entry and rerun the same command. Optional lint/demo tools such as `rustfmt`, `clippy`, and `stress-ng` are recorded separately and should not be confused with the required gate.

Before installing Ollama, downloading a model, or creating pressure on the VM, run the safe preflight harness:

```bash
bash bench/scripts/inference_tail_guard_preflight.sh
```

The script appends a validation-style entry to `docs/verification_log.md`.

Required gates:

- required Linux surfaces such as `/proc/self/cgroup`, `/proc/self/cpuset`, and `Cpus_allowed_list` must be readable
- the mock/noop runtime daemon smoke test must pass

Optional inventory:

- missing optional tools such as `ollama`, `llama.cpp`, `stress-ng`, or `taskset` are recorded as `SKIPPED`
- optional tool version/help command failures are recorded as non-blocking

Out of scope for this stage:

- no Ollama installation is attempted
- no model is downloaded
- no inference request is sent
- no CPU or I/O pressure is started

Use this as the handoff gate before the separate Ollama/model installation stage or before turning the harness into a real `ollama` or `llama.cpp` plus `stress-ng` demo wrapper.
