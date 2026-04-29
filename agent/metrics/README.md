# metrics

`agent/metrics` records both metric snapshots and trigger traces for the AegisAI Runtime control loop.

It now provides:
- `MetricRecord` for offline experiment analysis
- `MetricTrace` for scenario evaluation, action apply, and rollback trails

Key tracked metrics:
- `ttft`
- `p95_latency`
- `p99_latency`
- `jitter`
- `boost_hit_rate`
- `rollback_count`
- `side_effect_rate`

The recorder keeps a small rolling baseline per PID and computes `delta` plus `improvement_ratio` so later tuning and benchmark comparison have consistent evidence.
