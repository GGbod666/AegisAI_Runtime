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
- `linux`: minimal Linux ingestion path using procfs schedstat for run queue delay,
  with preflight support still available in the source layer

Current Linux source behavior:

- samples `/proc/<pid>/schedstat` for target runtimes to produce minimal
  `run_queue_delay` events before the full eBPF reader is wired
- plans the required probe set from `focus_signals`
- separates kernel probe signals from runtime-only signals
- exposes a `ProbeEventReader` hook for the later real probe reader
- provides `LinuxProbeDriver` plus `DriverBackedProbeEventReader` as the attach / poll / stop seam for real Linux probe wiring
- now includes `PreflightLinuxProbeDriver`, which validates tracefs / tracepoint / kprobe prerequisites before real probe loading
- carries a `ProbeReaderConfig` with startup policy and ring-buffer sizing hints
- records reader startup and shutdown state so Linux integration can validate attach/drain behavior
- records whether a driver is expected to stream events or is an explicit no-event preflight/audit path
- can be run with `--allow-partial-probes` while only `run_queue_delay` is backed by procfs schedstat
- preflight may attach successfully and still return no events by design because it does not load eBPF programs or read ring buffers

Current Linux reader CLI knobs:

- `--allow-partial-probes`
- `--probe-buffer-events <n>`
- `--probe-poll-timeout-ms <n>`
- `--verification-log <path>` to append daemon summaries to the validation audit log

Current actuator backend modes:

- `noop`: safe default for Windows-side development
- `linux-skeleton`: integration placeholder for the later Linux validation phase
- `linux-command`: Linux-only command-backed actuator path for preflight `renice` / `taskset` validation

Current mock source behavior:

- `MockEventSource::demo_sequence()` is self-describing enough to run with `noop` metadata
- `StaticMetadataProvider` remains useful when we want to test enrichment and tag-marker merge paths

Example:

```powershell
cargo run -p aegisai-runtime-daemon -- --repo-root . --source mock --metadata demo --actuator-backend noop
```
