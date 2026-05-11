# Architecture And Engineering Boundaries

This file owns durable architecture and process boundaries. Current status lives
in `docs/status.md`; product strategy and roadmap live in `docs/strategy.md`.

## Architecture Shape

AegisAI Runtime uses a dual-axis design:

- capability axis: observe, collect, classify, decide, act, measure
- scenario axis: AI workload awareness, inference tail protection, tool-call
  optimization

Design goals:

- low-overhead observability
- AI workload identification
- bounded and reversible intervention
- scenario-extensible policy
- benchmark-backed effect measurement

Non-goals:

- generic monitoring platform
- Linux scheduler replacement
- complex AI decision-making in realtime scheduler paths
- one-shot implementation of RAG, multi-agent, GPU, dashboard, and adaptive
  policy extensions

## Capability Layers

Observe:

- use a narrow privileged eBPF helper for key interference signals
- emit a normalized event stream
- keep the main daemon rootless
- fall back to procfs/PSI-style signals when helper support is unavailable

Collector:

- aggregate events over policy windows
- form feature views by process, thread, and cgroup

Classifier:

- identify AI workload and stage labels
- provide routing labels for scenario policies
- treat AI workload awareness as foundational capability, not a normal plugin

Policy:

- convert labels and feature views into bounded decisions
- enforce cooldowns, priorities, duration limits, and safety constraints

Actuator:

- execute reversible actions
- record action lifecycle and rollback state
- delegate live CPU affinity planning to `agent/actuator/src/cpu_affinity.rs`

Metrics / Explain:

- record before/after metrics and side effects
- support offline reports and threshold suggestions

## Scenario Lines

`ai_workload_awareness`:

- runtime recognition
- stage labels
- interactive-sensitive markers
- background job distinction

`inference_tail_guard`:

- tail-latency risk detection
- bounded boost decisions
- TTFT, P95/P99, and jitter evaluation

`tool_call_booster`:

- tool call lifecycle recognition
- executor/retrieval/rerank subpath tracking
- lifecycle-scoped scheduler protection

## Data Flow

```mermaid
flowchart TD
    A["Kernel Events"] --> B["eBPF Probes"]
    B --> C["Privileged eBPF Helper"]
    C --> D["Rootless Runtime Daemon"]
    D --> E["Collector"]
    E --> F["AI Workload Awareness"]
    F --> G["Scenario Policy"]
    G --> H["Actuator"]
    H --> I["Metrics Recorder"]
    I --> J["Explain / Tune"]
```

Closed-loop sequence:

1. privileged helper captures a fixed probe set
2. rootless daemon consumes normalized events
3. collector aggregates window features
4. classifier emits workload labels
5. scenario policy consumes labels and features
6. actuator applies bounded, rollback-capable actions
7. metrics evaluate benefit and side effects

## Repository Map

- `ebpf/`: probe contracts, descriptors, and helper-adjacent observability
- `agent/`: runtime daemon, collector, classifier, policy, actuator, metrics,
  explain/tune, git control
- `agent/ebpf_helper`: the only component intended to carry root or eBPF
  capability
- `scenarios/`: scenario packages for awareness, tail guard, and tool call
  booster
- `bench/`: scenario benchmarks, reports, and host preflights
- `configs/`: runtime, classifier, scenario, and safety config examples

The main daemon should not run as root and should not pass arbitrary
eBPF/bpftrace programs to the helper.

## Deployment Boundary

Target runtime environment:

- Linux kernel `5.15+`
- cgroup v2 or an explicitly understood host layout
- eBPF-capable environment for helper-backed signals

Default split:

- `aegisai-runtime-daemon`: ordinary user process
- `aegisai-ebpf-helper`: administrator-installed helper with minimal root or
  eBPF capability
- degraded path: procfs/PSI-style ordinary-permission observation when helper
  support is unavailable

Windows or macOS are acceptable for docs, control-plane preparation, and mock
verification. Probe validation and benchmark evidence require Linux.

## Live Action Boundaries

Global live-action rules:

- `linux-command` live actions require explicit operator confirmation and a
  non-empty PID allowlist. Scenario policy and config may request actions, but
  they cannot enable a new class of live host write by themselves.
- The rootless runtime daemon owns policy decisions, leases, audits, and
  rollback scheduling. It must not gain broad cgroupfs write authority.
- Each live write class needs an explicit guard, original-state capture,
  bounded affected process set, rollback path, and tests that show denied paths
  stay denied.

Current live affinity boundary:

- `agent/actuator/src/cpu_affinity.rs` parses `Cpus_allowed_list`, reads online
  CPU topology, intersects allowed/online CPUs, selects reserved/low-contention
  targets, and formats rollback targets.
- `linux-command` consumes planner output and executes protected `taskset`
  apply/rollback.
- Empty intersections must remain visible action-effectiveness risks, not fall
  back to offline or disallowed CPUs.

Current Tool Call Booster action boundary:

- benefit proof covers guarded scheduler actions: `nice`, plus explicitly
  enabled `affinity`
- `WarmupExecutor` defaults to deferred/no-side-effect audit; command backends
  can run an explicitly configured warmup command with a positive timeout, and
  rollback remains an audited no-op because cache/process priming is not
  deterministically reversible

Cpuset and background-isolation boundary:

- Current state remains disabled for live writes. `use_cpuset = true` is a
  policy/audit surface only; live guarded backends must keep reporting cpuset as
  disabled until a separate cgroup/cpuset applier is implemented and reviewed.
- A future live applier may touch only cgroup v2 files under an
  administrator-created AegisAI-owned subtree, for example
  `/sys/fs/cgroup/aegisai.runtime/`. It must not write the root cgroup,
  system/user manager cgroups, container-orchestrator-owned cgroups, or arbitrary
  paths learned from process metadata.
- The allowed live file surface is limited to creating/removing AegisAI-owned
  child cgroups and writing `cgroup.procs`, `cgroup.threads`, `cpuset.cpus`,
  `cpuset.mems`, and, only if background throttling is explicitly approved,
  `cpu.max` for those child cgroups. No other cgroup controller files are in
  scope without a new safety issue.
- The first implementation step is a dry-run planner. It should emit target
  pids/cgroups, proposed CPU set, original membership/cpuset/cpu.max capture
  plan, rollback plan, and a rejection reason without writing cgroupfs.
- Live writes require a dedicated cgroup/cpuset applier or helper with narrower
  authority than general root. Do not fold this into the eBPF helper unless a
  separate privilege review proves that combined authority is still minimal.
- Classification must show both sides of the isolation decision before an action
  is eligible: protected work must be interactive AI inference or an active tool
  call, and throttled work must be classified as `BACKGROUND_JOB`/batch with no
  interactive-latency-sensitive tag. Unknown, mixed, or parent-inferred-only
  classifications are dry-run or reject-only.
- The affected set must be bounded before apply: every pid/tid to move must be
  enumerated, still alive, in the operator allowlist or an AegisAI-owned cgroup,
  and below a documented maximum process count. Empty CPU sets, offline CPUs,
  missing `cpuset.mems`, or migrations that would move the daemon/helper itself
  are hard rejects.
- Rollback capture must happen before apply and include original cgroup
  membership, original cpuset/cpu.max values for any AegisAI-owned cgroups that
  will be changed, and whether temporary cgroups were created. Rollback errors
  must be audited with the failed file/path and manual restore instruction;
  repeated rollback failure must disable further live cpuset/background actions
  for the process set.

## Production Config Boundaries

Current state:

- `RuntimeOrchestratorConfig::load_from_repo_root` reads fixed files under
  `configs/*/*.example.toml` plus `configs/safety/default.toml`.
- Example files are suitable for tests, demos, and benchmark harnesses; they are
  not a production profile contract.

Profile selection rules for future production work:

- select one named profile before reading component config files
- precedence should be CLI flag, then environment variable, then a documented
  non-production local default
- profile names are identifiers, not paths; accept lowercase letters, digits,
  `_`, and `-`; reject path separators, `.` segments, empty names, and absolute
  paths
- production mode must not silently load `*.example.toml`

Schema validation should check TOML syntax, keys/types, required fields, enum
values, numeric ranges, cross-file safety, and host/environment readiness.
Errors should name profile, file, section, key, and violated constraint.

Deferred config work:

- hot reload and dynamic profile switching
- remote config distribution
- secret storage/interpolation
- schema migrations
- dashboard/UI editing
- profile inheritance
- adaptive policy writes back into profile files
- enabling live cpuset writes by profile alone

## Hotspot Refactor Boundaries

Known hotspots:

- `agent/runtime_daemon/src/source.rs`
- `agent/actuator/src/backend.rs`
- `agent/explain_tune/src/engine.rs`
- `agent/runtime_orchestrator/src/runtime_orchestrator.rs`
- `agent/policy_engine/src/engine.rs`
- `bench/scripts/inference_tail_guard_ollama_smoke.sh`

Do not split these as standalone cleanup. A split is acceptable only when it is
attached to active behavior work and covered by targeted verification.

Required boundaries:

- preserve public behavior and CLI/script outputs unless the active issue
  requires a behavior change
- extract one cohesive concern at a time
- avoid combining splitting with broad renames, style cleanup, or unrelated
  moves
- preserve or add targeted tests before claiming behavior preservation
- record the active `bd` issue that justified the split
