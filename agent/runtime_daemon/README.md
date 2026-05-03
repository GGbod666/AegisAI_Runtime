# runtime_daemon

`runtime_daemon` is the first runnable entrypoint for the unified AegisAI Runtime control loop.

It is responsible for:

- loading the repo configuration set
- creating the shared `RuntimeOrchestrator`
- polling an `EventSource`
- enriching partial source events through a `MetadataProvider`
- driving periodic rollback ticks
- printing a compact execution summary

Current host strategy:

- Windows: use `MockEventSource` and `StaticMetadataProvider` for development, integration, and control-loop verification
- Linux: switch to `ProcfsMetadataProvider`, then wire the real probe-backed source during the validation phase

Current source modes:

- `mock`: runnable today and intended for Windows-side development
- `linux`: minimal Linux ingestion path using procfs for run queue delay,
  CPU migration, and major page fault deltas, with preflight support still
  available in the source layer

Current Linux source behavior:

- samples `/proc/<pid>/schedstat`, `/proc/<pid>/sched`, and `/proc/<pid>/stat`
  for target runtimes to produce minimal `run_queue_delay`, `cpu_migration`,
  and `major_page_fault` events
- streams real eBPF-backed `offcpu_time` and `io_latency` events through
  `bpftrace`, normalized into the existing `SourceEvent` model
- prints signal observation summaries and feature-window maxima so
  `cpu_migration` and `major_page_fault` can be interpreted in real-machine
  experiments
- plans the required probe set from `focus_signals`
- separates kernel probe signals from runtime-only signals
- exposes a `ProbeEventReader` hook for the later real probe reader
- provides `LinuxProbeDriver` plus `DriverBackedProbeEventReader` as the attach / poll / stop seam for real Linux probe wiring
- now includes `PreflightLinuxProbeDriver`, which validates tracefs / tracepoint / kprobe prerequisites before real probe loading
- carries a `ProbeReaderConfig` with startup policy and ring-buffer sizing hints
- records reader startup and shutdown state so Linux integration can validate attach/drain behavior
- records whether a driver is expected to stream events or is an explicit no-event preflight/audit path
- can be run with `--allow-partial-probes` to keep procfs-backed signals flowing
  when the host cannot attach `bpftrace` eBPF probes
- preflight may attach successfully and still return no events by design because it does not load eBPF programs or read ring buffers

Current Linux reader CLI knobs:

- `--allow-partial-probes`
- `--probe-buffer-events <n>`
- `--probe-poll-timeout-ms <n>`
- `--verification-log <path>` to append daemon summaries to the validation audit log

Linux eBPF requirements:

- `bpftrace` must be installed and runnable as root on the validation host
- set `AEGISAI_BPFTRACE=/path/to/bpftrace` if it is not available as `bpftrace`
  in `PATH`
- current bpftrace probes attach `sched:sched_switch`,
  `block:block_rq_issue`, and `block:block_rq_complete`; hosts with different
  block tracepoint fields should use `--allow-partial-probes` until the script is
  adjusted for that kernel

Current actuator backend modes:

- `noop`: safe default for Windows-side development
- `linux-skeleton`: integration placeholder for the later Linux validation phase
- `linux-command-dry-run`: command-backed audit path that records planned `renice` / `taskset` apply and rollback without changing host state
- `linux-command`: guarded Linux-only live path. It requires `--confirm-live-actuator` plus a non-empty PID allowlist from `--live-pid-allowlist <pids>` or `[selection].pid_allowlist`. The live scope starts with `nice`; add `--enable-live-affinity` only after nice-only validation is stable. `cpuset` remains disabled.

Current mock source behavior:

- `MockEventSource::demo_sequence()` is self-describing enough to run with `noop` metadata
- `--mock-profile tool-call-lifecycle` replays a self-contained tool call chain
  with executor startup, retrieval, rerank, and background noise events carrying a
  shared `tool_call_id`
- `StaticMetadataProvider` remains useful when we want to test enrichment and tag-marker merge paths

Example:

```powershell
cargo run -p aegisai-runtime-daemon -- --repo-root . --source mock --metadata demo --actuator-backend noop
```

Tool call lifecycle harness:

```powershell
cargo run -p aegisai-runtime-daemon -- --repo-root . --source mock --mock-profile tool-call-lifecycle --metadata noop --actuator-backend noop
```
