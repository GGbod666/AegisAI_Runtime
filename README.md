# AegisAI Runtime

AegisAI Runtime is a Rust research prototype for AI-aware Linux runtime
control. It observes process-level scheduler and I/O pressure, classifies AI
workloads and tool-call stages, evaluates bounded policies, applies short-lived
reversible actions, and records whether the action helped.

The project is intentionally narrow: make AI inference and tool-call workloads
visible enough for the host runtime to protect latency-sensitive paths. It is
not a scheduler replacement, not a production daemon, and not a general
observability platform.

## Current Status

| Area | Status | What is actually implemented |
| --- | --- | --- |
| Mock control loop | Working | `collector -> classifier -> policy_engine -> actuator -> metrics` runs end-to-end with `noop` actions. |
| Linux source path | Partial | procfs-derived `run_queue_delay`, `cpu_migration`, and `major_page_fault`; helper-backed `offcpu_time` and `io_latency` where host support exists. |
| Actuator | Guarded prototype | `noop`, `linux-skeleton`, `linux-command-dry-run`, and gated `linux-command` with leases and rollback records. |
| Inference Tail Guard | Local controlled proof only | A CPU-interference Ollama run showed a modest live-guarded jitter improvement with effective host actions. |
| Tool Call Booster | Local controlled proof only | A fixed-work tool-call benchmark showed live-guarded latency improvement; an older less-controlled shape failed. |
| Packaging | Prototype | Debian/systemd files exist, but this is not a production release. |
| GPU, dashboard, adaptive policy | Not product features | Current work is observe/plan/read-only or shadow-only, disconnected from the live runtime path. |

## Measured Evidence

These are local benchmark artifacts, not broad production claims. Positive
numbers in the Inference Tail Guard table mean improvement. Negative numbers in
the Tool Call Booster row mean latency went down.

| Scenario | Run shape | Rounds / samples | Live action evidence | Result |
| --- | --- | ---: | --- | --- |
| Inference Tail Guard | `live_guarded_phase4_sample_sizing_20260511T000000Z`; Ollama `qwen2.5:0.5b`, CPU interference, live nice+affinity guarded by PID allowlist | 3 rounds, 24 samples per mode | 3 effective host-level actions | TTFT P95 `-0.51%`, latency P95 `+2.90%`, jitter `+6.42%`; jitter improved in 2/3 comparable rounds. |
| Tool Call Booster | `codex_fixed_work_guarded_final_20260511T141942Z`; fixed-work executor/retrieval/rerank chain, controlled CPU affinity, live guarded scheduler action | 3 comparable rounds | stage attribution for executor, retrieval, and rerank | end-to-end latency average delta `-26.832%`, median delta `-26.367%`; all three stages reported effectiveness `PASS`. |
| Tool Call Booster historical control | `live_guarded_tcb_stable_executor_20260511T000000Z`; older stable executor-control run | 3 comparable rounds | contract passed | benefit `FAIL`; live guarded improved 0/3 rounds by the 5% threshold. |

Important limits on the evidence:

- The Inference Tail Guard improvement is small and workload-specific. It does
  not prove a general P95/P99 tail-latency fix.
- Tail attribution currently classifies a `>=15%` P95/P99 scheduler-causality
  claim as `NOT_PROVEN_HELPER_GAP`; duration-backed scheduler signals peaked at
  `1.57%` of latency P95 in the available artifact.
- `noop` and `dry_run` modes validate recognition, policy evaluation, audit, and
  rollback shape only. They are not host-level performance proof.
- Helper-backed `offcpu_time` and `io_latency` depend on kernel, bpftrace, and
  privilege support. A current-host helper portability smoke has failed when the
  bpftrace eBPF backend was unavailable.
- All live results are from controlled local experiments with explicit PID
  allowlists. They should be reproduced on your own host before being trusted.

See [`docs/mvp_benefit_report.md`](docs/mvp_benefit_report.md),
[`docs/tail_guard_attribution_report.md`](docs/tail_guard_attribution_report.md),
and [`bench/tool_call_booster/README.md`](bench/tool_call_booster/README.md) for
the detailed evidence rules and benchmark context.

## Architecture

```text
Linux/procfs/helper events or mock events
        |
        v
collector -> classifier -> policy_engine -> actuator
        |          |              |             |
        |          |              |             v
        |          |              |        leases + rollback
        |          |              v
        |          |        bounded action plans
        |          v
        |    workload profile
        v
metrics + traces + reports
```

Core crates:

- `agent/runtime_daemon`: CLI, event sources, metadata enrichment, runtime loop
- `agent/runtime_orchestrator`: connects collector, classifier, policy,
  actuator, and metrics
- `agent/collector`: aggregates low-level events into feature windows
- `agent/classifier`: maps processes to AI workload labels and stages
- `agent/policy_engine`: evaluates scenario policies and resolves action
  conflicts
- `agent/actuator`: applies bounded actions and records rollback leases
- `agent/metrics` and `agent/explain_tune`: records traces and produces reports
- `agent/ebpf_helper` and `ebpf/ebpf_probe`: narrow helper/probe boundary

## Repository Layout

```text
agent/                 Rust control-loop crates
ebpf/                  Probe contracts and helper-facing probe definitions
scenarios/             Scenario policies for awareness, tail guard, tool calls
configs/               Example runtime, classifier, scenario, and safety config
bench/                 Benchmark harnesses, report scripts, and smoke checks
docs/                  Durable architecture notes and selected evidence reports
packaging/             Debian/systemd packaging prototype
plugins/               Placeholder for future extension points
```

Local agent state, task tracking, and generated session logs are intentionally
ignored and are not part of the public source tree.

## Quick Start

Prerequisites:

- Rust toolchain with Cargo, rustfmt, and clippy
- Python 3 for benchmark/report tests
- Linux for real procfs/eBPF paths
- Optional Linux tools for deeper validation: `bpftrace`, `bpftool`, `clang`,
  `llc`, `taskset`, and `stress-ng`

Run the safe mock path:

```bash
git clone https://github.com/GGbod666/AegisAI_Runtime.git
cd AegisAI_Runtime

cargo test --workspace

cargo run -p aegisai-runtime-daemon -- \
  --repo-root . \
  --source mock \
  --metadata demo \
  --actuator-backend noop \
  --max-events 3
```

Print the validation checklist:

```bash
bash bench/scripts/project_preflight.sh
```

Run the main workspace verification pass without writing a repository-local log:

```bash
AEGISAI_VERIFY_LOG=/tmp/aegisai_verify_workspace.md \
  bash bench/scripts/verify_workspace.sh
```

## Linux Modes

Planning-only Linux source path:

```bash
cargo run -p aegisai-runtime-daemon -- \
  --repo-root . \
  --source linux \
  --metadata procfs \
  --actuator-backend linux-skeleton \
  --allow-partial-probes
```

Linux command dry-run:

```bash
cargo run -p aegisai-runtime-daemon -- \
  --repo-root . \
  --source linux \
  --metadata procfs \
  --actuator-backend linux-command-dry-run \
  --allow-partial-probes
```

Live host actions are gated and should only be used in a controlled experiment:

```bash
cargo run -p aegisai-runtime-daemon -- \
  --repo-root . \
  --source linux \
  --metadata procfs \
  --actuator-backend linux-command \
  --confirm-live-actuator \
  --live-pid-allowlist <pid,...>
```

By default the live backend is nice-only. `--enable-live-affinity` is required
before `taskset` affinity changes are allowed. Keep the PID allowlist tight.

## Configuration

The default local demo profile reads example files from:

```text
configs/runtime/runtime.example.toml
configs/classifier/process_rules.example.toml
configs/scenarios/*.example.toml
configs/safety/default.toml
```

Named production profiles are selected with `--config-profile <name>` or
`AEGISAI_CONFIG_PROFILE=<name>` and are loaded from:

```text
configs/profiles/<name>/
```

Named profiles use stricter schema and cross-file safety validation than the
local demo path. There is no dynamic reload, remote config distribution, or
full TOML parser yet.

## Development

Useful checks:

```bash
cargo fmt --all -- --check
cargo test --workspace
cargo clippy --all-targets --all-features -- -D warnings
python3 -m unittest discover -s bench/tool_call_booster -p 'test_*.py'
python3 -m unittest discover -s bench/scripts -p 'test_*.py'
```

Full project preflight:

```bash
bash bench/scripts/project_preflight.sh --check
```

Some preflight paths require Linux host capabilities and may fail on systems
without the expected tools or privileges. For lightweight contribution checks,
start with the Rust and Python unit tests.

## Known Gaps

- Not production-ready; no stability or safety guarantee for arbitrary hosts.
- Live cgroup/cpuset mutation is not enabled as a production path.
- eBPF helper portability is not solved across kernels and privilege models.
- Benefit evidence covers a small number of controlled local workload shapes.
- No GPU scheduler, live dashboard control plane, or online adaptive policy
  loop is shipped.
- Root `LICENSE` is still missing even though several crate manifests declare
  `MIT`; add a repository-level license before relying on redistribution terms.

## Contributing

Useful contribution areas:

- Cross-kernel helper/probe validation
- Reproducible benchmark workloads and public artifact hygiene
- Safer isolation backends with narrow live-action gates
- Production profile examples and packaging hardening
- Scenario policies with strict evidence gates
- Documentation that helps reproduce results without overstating them

Keep pull requests small and include the smallest relevant verification command.
