# AegisAI Runtime

AegisAI Runtime is an experimental Rust control loop for AI-aware Linux workload
management. It observes scheduler and I/O interference signals, classifies AI
processes and tool-call stages, evaluates bounded scenario policies, applies
short-lived reversible actions, and records whether the intervention actually
helped.

The project is built around a narrow claim: AI runtimes should be visible to the
operating system as workloads with different latency, isolation, and lifecycle
needs. AegisAI Runtime explores that idea without replacing the Linux scheduler
or putting opaque AI decisions in hot kernel paths.

> Status: usable research prototype. The mock path, procfs path, helper-backed
> probe path, policy engine, guarded actuator, metrics, and benchmark reports are
> implemented. Production hardening, cross-kernel validation, and broader
> workload evidence are still active work.

## Highlights

- **AI workload awareness**: classifies inference servers, tool executors,
  retrieval/rerank workers, background jobs, and latency-sensitive interactive
  paths from process metadata and rules.
- **Inference Tail Guard**: detects tail-latency risk from signals such as run
  queue delay, off-CPU time, CPU migration, major faults, and optional I/O
  latency.
- **Tool Call Booster**: tracks executor, retrieval, and rerank stages so
  scheduler actions can be attributed to a specific tool-call lifecycle.
- **Bounded actuator model**: supports noop, Linux planning, Linux command
  dry-run, and guarded live command backends with leases and rollback records.
- **Narrow privileged boundary**: keeps the main daemon rootless and isolates
  eBPF/bpftrace access behind `aegisai-ebpf-helper`.
- **Evidence-first workflow**: separates observation, dry-run validation, live
  host action, and measured benefit in benchmark reports.

## Architecture

```text
Linux signals / mock events
        |
        v
collector -> classifier -> policy_engine -> actuator
        |          |              |             |
        |          |              |             v
        |          |              |        leases + rollback
        |          |              v
        |          |        scenario decisions
        |          v
        |    workload profile
        v
metrics + traces + reports
```

Core layers:

- `agent/runtime_daemon`: CLI entrypoint, source selection, metadata enrichment,
  and runtime loop.
- `agent/runtime_orchestrator`: connects collection, classification, policy,
  actuation, and metrics.
- `agent/collector`: aggregates low-level events into feature windows.
- `agent/classifier`: maps Linux processes to AI runtime semantics.
- `agent/policy_engine`: evaluates scenario policies and resolves conflicts.
- `agent/actuator`: manages bounded action application and rollback.
- `agent/metrics` and `agent/explain_tune`: record traces and produce reports.
- `agent/ebpf_helper` and `ebpf/ebpf_probe`: define and run the narrow probe
  boundary.

## Repository Layout

```text
agent/                 Rust control-loop crates
ebpf/                  Probe contracts and eBPF-oriented helpers
scenarios/             Scenario policies for awareness, tail guard, tool calls
configs/               Example runtime, classifier, scenario, and safety config
bench/                 Benchmark harnesses, report scripts, and smoke checks
docs/                  Architecture notes, validation logs, and evidence reports
packaging/             Debian/systemd packaging prototype
plugins/               Placeholder for future extension points
```

## Quick Start

Prerequisites:

- Rust toolchain with Cargo, rustfmt, and clippy
- Python 3 for benchmark/report tests
- Linux for real procfs/eBPF paths
- Optional Linux tools for deeper validation: `bpftrace`, `bpftool`, `clang`,
  `llc`, `taskset`, and `stress-ng`

Clone and run the safe mock path:

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

Print the project validation checklist:

```bash
bash bench/scripts/project_preflight.sh
```

Run the main workspace verification pass without appending to the tracked log:

```bash
AEGISAI_VERIFY_LOG=/tmp/aegisai_verify_workspace.md \
  bash bench/scripts/verify_workspace.sh
```

## Linux Modes

Linux source with procfs metadata and planning-only actuator:

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

Live host actions are intentionally gated:

```bash
cargo run -p aegisai-runtime-daemon -- \
  --repo-root . \
  --source linux \
  --metadata procfs \
  --actuator-backend linux-command \
  --confirm-live-actuator \
  --live-pid-allowlist <pid,...>
```

The live backend is designed for controlled experiments. By default it only
allows nice-related actions; `--enable-live-affinity` is required before it may
apply `taskset` affinity changes. Always use a tight PID allowlist.

## Evidence

Current local evidence includes:

- Inference Tail Guard live guarded run: `PASS` in
  [`docs/mvp_benefit_report.md`](docs/mvp_benefit_report.md).
- Tool Call Booster fixed-work guarded run: `PASS` in the latest project status
  and benchmark artifacts referenced from [`docs/status.md`](docs/status.md).
- Workspace gates for Rust, Python report tests, shell syntax, daemon smoke
  tests, and Linux preflight are listed in
  [`bench/scripts/project_preflight.sh`](bench/scripts/project_preflight.sh).

Interpretation is deliberately conservative. Noop and dry-run modes prove
recognition, policy evaluation, audit, and rollback paths; they do not prove
host-level performance benefit. Live guarded reports are controlled local
experiments, not a general production guarantee.

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

Production profile names are identifier-only, and profile loading performs
stricter schema and cross-file safety validation than the local demo path.

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

Some preflight paths exercise host capabilities and may skip or fail on systems
without the expected Linux tooling. For lightweight contribution checks, start
with `cargo test --workspace` and the Python unit tests.

## Contributing

High-value contribution areas:

- Cross-kernel validation for procfs, bpftrace, and helper-backed probes
- More reproducible benchmark workloads and public artifact hygiene
- Safer live isolation backends for cgroup/cpuset experiments
- Production profile examples and packaging hardening
- Scenario policies with narrow evidence gates
- Documentation that helps operators reproduce experiments without overstating
  results

Public issues and pull requests are welcome. Please keep changes small, include
the smallest relevant verification command, and avoid mixing benchmark evidence,
status updates, and unrelated refactors in the same pull request.

## Boundaries

AegisAI Runtime is not:

- a generic observability dashboard
- a Linux scheduler replacement
- a production-ready daemon for arbitrary hosts
- a GPU scheduler
- an online adaptive policy system with live profile mutation

Those directions are intentionally kept behind evidence gates until the safety
and measurement story is strong enough.

## License

Several crate manifests currently declare `MIT`, but this repository does not
yet include a root `LICENSE` file. Add a repository-level license before relying
on redistribution terms.
