# Deferred GPU Coordination Evidence Gate

This design records the first acceptable slice for
`AegisAI_Runtime-0ry.3`. It is a planning and evidence gate only. It does not
add a GPU scheduler, CUDA or driver control, MIG reconfiguration, helper
control, daemon integration, or production GPU mutation behavior.

Reference framing: NVIDIA DCGM provides datacenter GPU telemetry,
diagnostics, health, and profiling surfaces, while the NVIDIA MIG guide
documents GPU partitioning, supported hardware prerequisites, privileged setup
flows, and device/capability boundaries. For this repo that maps to
read-only discovery first, explicit NVIDIA-only first-slice scope, unsupported
host no-op behavior, and deny-by-default mutation gates until isolation,
privilege, target allowlist, rollback, and benchmark evidence are proven.

Reference links:

- NVIDIA DCGM feature overview:
  https://docs.nvidia.com/datacenter/dcgm/latest/user-guide/feature-overview.html
- NVIDIA MIG user guide:
  https://docs.nvidia.com/datacenter/tesla/mig-user-guide/index.html

## First Slice Contract

- Mode is observe/plan-only. Plans are audit evidence, not daemon inputs or
  actuator commands.
- Runtime behavior remains disconnected. The gate does not call the daemon,
  actuator, helper, `nvidia-smi`, NVML, DCGM hostengine, CUDA, sysfs, cgroups,
  or container runtime APIs.
- The first supported host class is NVIDIA datacenter GPU telemetry with DCGM
  inventory available in recorded artifacts.
- Non-NVIDIA GPUs, hosts without compatible GPUs, missing telemetry, detached
  devices, and hosts without isolation evidence produce no-op plans.
- MIG and container boundaries are treated as prerequisites for future work.
  This gate records them but does not create, destroy, or rebalance instances.
- Live GPU actions require a future issue with an explicit target allowlist,
  privilege review, isolation review, rollback/no-op proof, and guarded
  benchmark evidence. This gate hard-rejects them.

## Safety Invariants

- Live GPU mutation is denied by default.
- Mutation-shaped candidates without an explicit target allowlist are rejected.
- Unsupported vendors and unsupported hosts remain no-op.
- Missing MIG/container/device isolation evidence is a hard rejection.
- Missing privilege review is a hard rejection.
- Missing rollback or no-op behavior is a hard rejection.
- Benchmark evidence cannot be interpreted as scheduler benefit. It only proves
  parser, dry-run planner, and overhead boundaries for recorded inputs.

## Evidence Artifacts

`bench/scripts/gpu_coordination_gate.py` writes:

- `gpu_coordination_plan.json`: per-host parser output, unsupported-host
  no-op behavior, dry-run plan decisions, and safety matrix summary
- `gpu_coordination_benchmark.csv`: representative baseline versus dry-run
  telemetry overhead, host CPU impact, latency/throughput deltas, and memory
  pressure deltas
- `gpu_coordination_safety_matrix.csv`: denied future mutation candidates and
  rejection reasons
- `gpu_coordination_gate_report.md`: human-readable verdict and artifact paths

The deterministic default smoke can be run with:

```bash
python3 bench/scripts/gpu_coordination_gate.py
```

## Promotion Requirements

Before this can become runtime behavior, a separate issue must provide:

- real GPU-host parser evidence from representative NVIDIA hardware
- explicit decision for any non-NVIDIA support beyond no-op
- unsupported-host smoke on CPU-only and non-NVIDIA hosts
- dry-run planner proof on a GPU host with artifact paths
- safety rejection matrix for live action, target allowlist, isolation,
  privilege, rollback, detached-device, and unsupported-vendor failures
- benchmark report comparing no-op and dry-run modes for latency, throughput,
  host CPU, and GPU memory pressure
- operator approval and rollback evidence before any live mutation path is
  enabled
