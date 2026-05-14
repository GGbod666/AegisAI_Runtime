# Deferred Online Adaptive Policy Evidence Gate

This design records the first acceptable slice for
`AegisAI_Runtime-0ry.4`. It is a planning and evidence gate only. It does not
add online learning, live mutation, profile writes, remote policy fetches, or
new actuator behavior.

Reference framing: NIST AI RMF 1.0 describes risk management for AI systems,
and the NIST AI RMF Playbook organizes practical work around Govern, Map,
Measure, and Manage. For this repo that maps to explicit ownership and
non-goals, bounded replay inputs, measured safety/benchmark artifacts, and
managed freeze, rollback, and operator approval gates before live mutation.

## First Slice Contract

- Mode is shadow-only. Adaptive output is a suggestion with an audit id.
- Runtime behavior remains disconnected. The gate does not call the daemon,
  actuator, helper, or config profile writer.
- Operator approval is required before any future live mutation, but this
  shadow gate must not consume approval.
- Every recommendation records the static-policy comparison, rationale,
  bounded action shape, rollback plan, and freeze/operator state.
- Suggestions are not scheduler benefit proof. Host-level benefit still
  requires guarded live A/B evidence under the existing strict benefit gate.

## Safety Invariants

- Priority deltas are clamped to the configured cap/floor.
- Boost duration is clamped to the global duration cap.
- Affinity ratio is clamped to the configured ratio cap.
- Live mutation and production profile writes are hard rejections.
- A recommendation without a rollback plan is a hard rejection.
- Drift above the configured threshold latches freeze behavior and suppresses
  further recommendations.
- Retained replay state is bounded by `retention_limit`.

## Evidence Artifacts

`bench/scripts/adaptive_policy_gate.py` writes:

- `adaptive_policy_shadow_replay.json`: per-sample decisions, audit ids,
  freeze reasons, and recommendation details
- `adaptive_policy_benchmark.csv`: static baseline versus adaptive shadow
  trigger counts, false positives, false negatives, mutation count, retention
  maximum, drift freeze count, and stability verdict
- `adaptive_policy_gate_report.md`: human-readable verdict and artifact paths

The deterministic default smoke can be run with:

```bash
python3 bench/scripts/adaptive_policy_gate.py
```

## Promotion Requirements

Before this can become runtime behavior, a separate issue must provide:

- deterministic replay against recorded production-like artifacts
- safety invariant tests for all configured action classes
- drift, freeze, rollback, bounded-retention, and operator approval evidence
- benchmark report comparing the adaptive suggestion policy with the existing
  static policy baseline
- guarded live A/B proof before any live mutation path is enabled
- production profile review proving no self-modifying config writes occur
