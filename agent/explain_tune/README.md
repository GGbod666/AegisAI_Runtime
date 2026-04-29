# explain_tune

`agent/explain_tune` is the offline analysis layer for AegisAI Runtime.

It consumes:
- `MetricRecord` snapshots from `agent/metrics`
- `MetricTrace` trails from scenario evaluation and action lifecycle
- optional `ScenarioPolicy` inputs from `agent/policy_engine`

It produces:
- experiment summaries per scenario
- per-trigger explanations with rationale and trace evidence
- conservative tuning suggestions for thresholds, cooldowns, and action intensity

The crate intentionally stays out of the real-time control path. Its job is to
help benchmark review, parameter tuning, and operator confidence after an
experiment or replay run.
