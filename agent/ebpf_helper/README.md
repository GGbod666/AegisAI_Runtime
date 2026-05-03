# ebpf_helper

`aegisai-ebpf-helper` is the narrow privileged boundary for Linux eBPF signal
collection.

The main runtime daemon is expected to run as a normal user. This helper is the
only component that should be installed with root or equivalent eBPF privileges.
It accepts a fixed set of probe selectors and streams normalized
`aegisai_probe ...` records on stdout for the daemon to consume.

It intentionally does not accept arbitrary bpftrace programs from the daemon.
That keeps the privileged surface tied to AegisAI's expected off-CPU and I/O
latency probes instead of exposing a generic root command runner.

Basic readiness check:

```bash
aegisai-ebpf-helper --check
```

Stream both first-wave eBPF signals for a target runtime:

```bash
aegisai-ebpf-helper stream --offcpu --io --process-name ollama
```

The packaged accelerator should install this helper as a controlled system
component, while `aegisai-runtime-daemon` remains rootless.

## Packaging Rule

Do:

- install or run this helper with the minimum host privileges required by the
  chosen eBPF backend
- keep the main daemon, UI, policy engine, reports, and configs in normal-user
  space
- pass only selectors (`--pid`, `--process-name`) and fixed signal flags
  (`--offcpu`, `--io`) across the boundary
- log helper readiness and attach failures clearly so the daemon can fall back to
  procfs-backed observation when requested

Do not:

- run the full accelerator application as root for convenience
- let the rootless daemon send arbitrary bpftrace source text to this helper
- expand this helper into a general command executor

This preserves the intended product shape: rootless by default, enhanced by an
auditable privileged component when administrators approve kernel-level
observability.
