# Modular Execution Plan

Boundary: this is a historical staged implementation plan from the runnable mock
control-loop era. Current execution order lives in `docs/next_stage.md`; active
task detail lives in `docs/task_list.md`.

This plan defines the next implementation stages after the runnable mock
control loop. The goal is to reach a Linux VM demo for:

`AI Workload Awareness -> Inference Tail Guard`

## Stage 1: Preflight-Ready Runtime

Goal: make the current runtime safe and diagnosable before real system effects.

Modules:

- `runtime_daemon`: keep one command as the main entrypoint.
- `source_adapter`: validate Linux probe plans and startup state.
- `actuator`: make Linux command-backed actions explicit and auditable.
- `verification`: append all validation output to `docs/verification_log.md`.

Exit checks:

- `cargo check --workspace`
- `cargo test --workspace`
- mock daemon smoke test triggers `inference_tail_guard`
- Linux source preflight exits cleanly with `--allow-partial-probes`

## Stage 2: Linux Signal Ingestion

Goal: replace planning-only Linux source behavior with a real event stream.

Initial scope:

- `run_queue_delay`
- `offcpu_time`
- `cpu_migration`
- `major_page_fault`

Implementation boundary:

- The source layer owns attach, poll, stop, and event normalization.
- The orchestrator should not know whether events came from mock, replay, or live probes.
- If a real eBPF reader requires extra dependencies, add them behind a narrow module boundary.

Exit checks:

- Linux source emits at least one normalized `SourceEvent` from a controlled local workload.
- Metadata enrichment fills process name, cmdline, cgroup, and parent fields from `/proc`.
- Verification log includes probe startup, emitted event count, and shutdown summary.

## Stage 3: Bounded Linux Actions

Goal: safely execute and roll back the minimum useful Linux actions.

Initial scope:

- capture original nice value
- capture original CPU affinity
- apply bounded `renice`
- apply bounded `taskset` or syscall-backed affinity
- rollback both values using captured state

Non-goals for this stage:

- cpuset writes
- background throttling
- permanent service tuning

Exit checks:

- Command-backed preflight records apply and rollback details.
- Missing capture state prevents unsafe rollback and is visible in audit fields.
- A real Linux process can be boosted and restored in a controlled test.

## Stage 4: Inference Tail Guard Demo

Goal: prove the MVP path on a real runtime.

Default target:

- `ollama` first
- `llama.cpp` as fallback

Experiment shape:

- baseline run with no bounded boost
- boosted run with `inference_tail_guard`
- optional CPU pressure with `stress-ng`

Metrics:

- TTFT
- P95 latency
- P99 latency
- jitter
- boost hit rate
- rollback count

Exit checks:

- A/B results are recorded in the verification log or an experiment artifact.
- The observed result is explicit: improved, neutral, or regressed.
- Any regression includes enough trace data to tune thresholds.

## Stage 5: Tool Call Booster

Goal: extend the same proven loop to tool calling once inference guard is stable.

Initial scope:

- executor startup delay
- retrieval queue wait
- rerank queue wait
- lifecycle-scoped bounded action

Exit checks:

- tool executor/retrieval/rerank labels route correctly.
- tool-call benchmark has a reproducible baseline and boosted run.
