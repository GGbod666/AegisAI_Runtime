# Strategy And Evidence Rules

_Updated: 2026-05-11_

This file owns product strategy, MVP definition, experiment rules, and long-run
roadmap. It does not track active task state; use `bd` for that.

## Current Stage

The project is past the runnable control-loop setup and 19-task
evidence-hardening pass. The active stage is product evidence and production
readiness:

- preserve the effective live `inference_tail_guard` benefit proof under the
  strict Phase 4 gate
- prove or reproducibly falsify Tool Call Booster guarded latency benefit
- keep helper portability, production config, packaging, cpuset/background
  isolation, WarmupExecutor side effects, dashboard, GPU, and adaptive policy
  work behind explicit follow-up issues

The accepted mainline remains:

`collector -> classifier -> policy_engine -> actuator -> metrics`

## MVP Definition

The MVP is the smallest runnable AI-aware optimization loop:

- identify target AI inference workload and attach usable labels
- collect interference signals: `run_queue_delay`, `cpu_migration`,
  `major_page_fault`, plus helper-backed `offcpu_time` and `io_latency`
- trigger bounded system actions through guarded policy and actuator paths
- record before/after TTFT, P95/P99 latency, jitter, trigger count, rollback
  count, and side effects

The basic closed-loop MVP is accepted. Inference Tail Guard performance benefit
is proven for the latest controlled CPU-interference run shape; Tool Call
Booster performance benefit is not proven yet.

## Strict Benefit Gate

Do not weaken this rule:

- `noop_observation` and `dry_run` prove recognition, trigger, audit, command
  planning, and rollback shape only.
- Host-level MVP benefit requires both effective live host-level actuator
  change and stable repeated A/B benefit.

Repeated benefit means:

- at least three comparable rounds
- at least three samples per mode in successful comparable rounds
- at least two thirds of comparable rounds improve
- mean improvement reaches the configured report threshold, currently `5.0%`,
  for TTFT P95/P99, latency P95/P99, or jitter
- no live guarded action audit errors

The current `docs/mvp_benefit_report.md` reports `PASS`: a controlled
sample-sizing follow-up kept model, prompt, stress shape, concurrency, and live
affinity/nice pairing fixed, increased samples per mode from `4` to `8`, and
showed stable live-guarded jitter benefit with effective host-level actions.

## Current Required Work

### Inference Tail Guard Benefit

Latest proof run: `live_guarded_phase4_sample_sizing_20260511T000000Z`

Disposition:

- product benefit proof observed for the current CPU interference run shape
- strict rules remain mandatory: PID allowlist, explicit live confirmation,
  effective live action, clean mode contracts, and stable repeated benefit
- future changes to this path should preserve the generated report contract and
  rerun the same exit checks before changing the benefit claim

Exit checks:

- `live_guarded` records effective host-level action, or the report classifies
  `action_effectiveness` explicitly
- baseline, noop observation, dry-run, and live guarded modes remain comparable
- report gives `PASS` only when effective live action and stable repeated
  benefit are both present

### Tool Call Booster Benefit

Issue: `AegisAI_Runtime-79d`

Goal:

- continue from the real executor lifecycle harness and current live guarded
  artifact
- determine whether guarded scheduler actions can produce repeated tool-call
  latency benefit on this host
- keep `WarmupExecutor` benefit accounting separate from scheduler benefit; the
  default remains deferred audit unless an explicit bounded warmup command is
  configured

Exit checks:

- report includes latency deltas, trigger counts, rollback counts, action
  errors, and explicit PASS/FAIL verdict
- noop/dry-run remain control evidence, not host benefit proof
- guarded benefit `PASS` requires clean contracts plus repeated latency
  improvement versus baseline

## Experiment Method

Use controlled A/B runs with fixed machine, runtime, model, concurrency, prompt
shape, and interference level.

Current mode taxonomy:

- `baseline`: no daemon intervention or unobserved same-round reference
- `noop_observation`: recognition and lifecycle observation only
- `dry_run`: command planning, audit, and rollback shape only
- `live_guarded`: real bounded action under explicit confirmation and PID
  allowlist

Core metrics:

- workload: TTFT, P95/P99 latency, jitter, tool call end-to-end latency,
  executor/retrieval/rerank latency
- system: run queue delay, off-CPU time, CPU migration, major page fault, block
  I/O latency
- policy: trigger count, boost duration, rollback count, action errors, observed
  side effects

Recommended scenarios:

- AI inference plus CPU interference with `ollama` or `llama.cpp` and
  `stress-ng`
- AI inference plus I/O disturbance
- classifier replay against fixed target-runtime and background-worker samples
- real executor lifecycle harness for Tool Call Booster

## Command Entry Points

Safe reconfirmation:

```bash
AEGISAI_VERIFY_LOG=/tmp/aegisai_verify_workspace.md bash bench/scripts/verify_workspace.sh
AEGISAI_VERIFY_LOG=/tmp/aegisai_toolchain_preflight.md bash bench/scripts/toolchain_preflight.sh
AEGISAI_VERIFY_LOG=/tmp/aegisai_inference_preflight.md bash bench/scripts/inference_tail_guard_preflight.sh
```

Inference Tail Guard live benefit proof, only inside an approved experiment
window:

```bash
AEGISAI_AB_MODES=baseline,noop_observation,dry_run,live_guarded \
AEGISAI_CONFIRM_LIVE_ACTUATOR=1 \
AEGISAI_LIVE_PID_ALLOWLIST=<pid,...> \
  bash bench/scripts/inference_tail_guard_phase4_report.sh
```

Tool Call Booster guarded proof:

```bash
AEGISAI_TCB_MODES=baseline,noop_observation,dry_run,live_guarded \
AEGISAI_CONFIRM_LIVE_ACTUATOR=1 \
  bash bench/scripts/tool_call_booster_real_executor_harness.sh
```

## Roadmap

| phase | objective | current disposition |
| --- | --- | --- |
| 0. Framework reset | project definition, dual-axis skeleton, config and safety boundaries | complete |
| 1. Awareness foundation | stable AI workload labels for scenario policies | basic loop complete |
| 2. Inference Tail Guard MVP | prove or falsify live guarded tail-latency benefit | latest controlled run `PASS` |
| 3. Tool Calling Booster | prove or falsify guarded tool-call latency benefit | active: `AegisAI_Runtime-79d`; `WarmupExecutor` has explicit command-backed side-effect boundaries and separate reporting |
| 4. AI-aware isolation | define live cpuset/background throttling boundary | deferred: `AegisAI_Runtime-otk` |
| 5. Explain/Tune | useful offline reports and threshold suggestions | offline basics exist; online learning deferred |
| 6. Productionization | config profiles, schema validation, daemon/helper packaging | deferred: `AegisAI_Runtime-cqv`, `AegisAI_Runtime-ufp` |
| 7. Advanced extensions | RAG, multi-agent isolation, GPU host coordination, cold start, adaptive policy, dashboard | deferred: `AegisAI_Runtime-0ry` |

## Non-Goals For This Stage

- weakening the benefit gates
- treating noop or dry-run deltas as host-level benefit
- broad module decomposition without an active behavior issue
- enabling live cpuset writes by configuration alone
- treating warmup side-effect counts as scheduler benefit proof

This stage exits when artifacts support one honest statement: benefit proven, or
benefit not proven with a reproducible failure reason.
