# Deferred Observability Dashboard Evidence Gate

This design records the first acceptable slice for
`AegisAI_Runtime-0ry.2`. It is a planning and evidence gate only. It does not
add a dashboard service, web UI, profile editor, helper controller, scheduler
action path, remote config channel, or new runtime behavior.

Reference framing: OpenTelemetry treats metrics as runtime measurements with
metadata and provides a stable metrics signal. Its metrics specification keeps
instrumentation, SDK processing, and exporters separated. For this repo that
maps to a dashboard that consumes stable exported telemetry and existing
artifacts, while keeping benefit truth in benchmark reports and actuator truth
in daemon audit output.

Reference links:

- OpenTelemetry metrics signal:
  https://opentelemetry.io/docs/concepts/signals/metrics/
- OpenTelemetry metrics specification:
  https://opentelemetry.io/docs/specs/otel/metrics/

## First Slice Contract

- Mode is read-only. The dashboard model is a parsed snapshot of existing
  audit output, verification artifacts, and stable telemetry exports.
- Runtime behavior remains disconnected. The gate does not call the daemon,
  actuator, helper, scheduler, profile writer, or policy engine.
- Dashboard status cannot be the source of benefit truth. Benefit verdicts are
  copied from recorded verification artifacts only.
- The dashboard-facing fields are named and versioned before any UI work:
  `runtime_audit.v1`, `verification_artifact.v1`, and
  `otel_metric_export.v1`.
- Local bind/auth scope remains a future deployment decision. This gate records
  `local_operator` scope only and does not open a socket.

## Safety Invariants

- Read-only mode is mandatory.
- Live policy/profile editing is rejected.
- Helper control is rejected.
- Scheduler or actuator commands are rejected.
- Remote config distribution is rejected.
- Raw command lines, environment values, secrets, and unredacted user text are
  rejected from dashboard input artifacts.
- A dashboard-sourced benefit override is rejected.
- Parser/render overhead evidence cannot prove scheduler benefit.

## Evidence Artifacts

`bench/scripts/observability_dashboard_gate.py` writes:

- `observability_dashboard_snapshot.json`: parsed read-only dashboard model
  with source counts, metric names, artifact verdicts, redaction status, and
  no-control-path summary
- `observability_dashboard_export.csv`: stable exported rows for audit,
  verification, and telemetry inputs
- `observability_dashboard_gate_report.md`: human-readable verdict and
  artifact paths

The deterministic default smoke can be run with:

```bash
python3 bench/scripts/observability_dashboard_gate.py
```

## Promotion Requirements

Before this can become runtime behavior, a separate issue must provide:

- parser/export tests for all dashboard-facing schema versions
- local smoke against recorded audit, verification, and telemetry artifacts
- bind/auth decision and no-new-privilege review
- PII/log redaction review using representative artifacts
- failure isolation proof showing dashboard failures do not affect daemon or
  helper services
- overhead report showing scrape/render cost and daemon loop latency remain
  bounded
- proof that benefit verdicts remain artifact-derived and cannot be overridden
  by UI state
