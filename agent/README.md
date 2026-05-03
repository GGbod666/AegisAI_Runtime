# Agent Layer

The `agent/` layer is the user-space control loop for AegisAI Runtime.

It is responsible for turning observed signals into:

- workload labels
- scenario decisions
- bounded actions
- metrics and rollback records

## Submodules

- `collector`: aggregate low-level events into stable feature windows
- `ebpf_helper`: narrow privileged helper for real Linux eBPF off-CPU and I/O
  latency event streaming
- `classifier`: map processes and threads to AI-runtime semantics
- `policy_engine`: evaluate scenario rules and resolve action conflicts
- `actuator`: apply bounded actions and manage rollback leases
- `metrics`: record outcomes, side effects, and trace records
- `explain_tune`: generate offline reports and tuning suggestions
- `git_control`: discover repository state and plan version checkpoints for experiments
- `runtime_orchestrator`: compose the shared control-loop modules
- `runtime_daemon`: runnable entrypoint, source adapter, metadata enrichment, and loop driver

## Design Rules

- `classifier` is foundational capability, not a plugin
- `policy_engine` owns reusable decision mechanics
- scenario-specific logic lives under `scenarios/`
- configuration is preferred over hard-coded branching
- every action must be bounded and revertible
- the main runtime daemon is rootless; privileged kernel/eBPF work belongs in
  narrow helpers, not the whole control loop
