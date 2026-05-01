# MVP Benefit Report

## Verdict

- Result: `FAIL`
- Conclusion: MVP benefit not proven: no live guarded mode met the stable improvement threshold.
- Run ID: `phase4_mvp_benefit_multiround`

## Controls

- Model: `qwen2.5:0.5b`
- Rounds per scenario: `2`
- Samples per mode: `4`
- Concurrency: `2`
- Modes: `baseline,live_guarded`

## Aggregate Comparison

| scenario | mode | rounds | samples | TTFT P95 mean | TTFT P99 mean | lat P95 mean | lat P99 mean | jitter mean | TTFT P95 delta % | TTFT P99 delta % | lat P95 delta % | lat P99 delta % | jitter delta % |
| --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- |
| CPU interference | baseline | 2/2 | 8/8 | 4142.160 | 4142.160 | 6987.407 | 6987.407 | 2093.825 | 0.00 | 0.00 | 0.00 | 0.00 | 0.00 |
| CPU interference | live_guarded | 0/2 | 8/8 | 9398.140 | 9398.140 | 17449.308 | 17449.308 | 5513.315 | -126.89 | -126.89 | -149.73 | -149.73 | -163.31 |
| CPU + optional I/O interference | baseline | 2/2 | 8/8 | 10568.951 | 10568.951 | 19279.541 | 19279.541 | 5916.395 | 0.00 | 0.00 | 0.00 | 0.00 | 0.00 |
| CPU + optional I/O interference | live_guarded | 0/2 | 8/8 | 11808.246 | 11808.246 | 19616.565 | 19616.565 | 5061.732 | -11.73 | -11.73 | -1.75 | -1.75 | 14.45 |

## Per-Round Comparison

| scenario | round | status | mode | ok/total | TTFT P95 | TTFT P99 | lat P95 | lat P99 | jitter | triggers | rollbacks | action errors |
| --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- |
| CPU interference | 1 | 0 | baseline | 4/4 | 3984.873 | 3984.873 | 6494.342 | 6494.342 | 2025.855 | 0 | 0 | 0 |
| CPU interference | 1 | 1 | live_guarded | 4/4 | 12830.614 | 12830.614 | 17180.116 | 17180.116 | 4934.194 | 0 | 0 | 0 |
| CPU interference | 2 | 0 | baseline | 4/4 | 4299.447 | 4299.447 | 7480.473 | 7480.473 | 2161.795 | 0 | 0 | 0 |
| CPU interference | 2 | 1 | live_guarded | 4/4 | 5965.667 | 5965.667 | 17718.500 | 17718.500 | 6092.436 | 20 | 20 | 4 |
| CPU + optional I/O interference | 1 | 0 | baseline | 4/4 | 10583.054 | 10583.054 | 18795.864 | 18795.864 | 5518.367 | 0 | 0 | 0 |
| CPU + optional I/O interference | 1 | 1 | live_guarded | 4/4 | 11486.957 | 11486.957 | 18644.215 | 18644.215 | 4431.643 | 0 | 0 | 0 |
| CPU + optional I/O interference | 2 | 0 | baseline | 4/4 | 10554.848 | 10554.848 | 19763.219 | 19763.219 | 6314.422 | 0 | 0 | 0 |
| CPU + optional I/O interference | 2 | 1 | live_guarded | 4/4 | 12129.535 | 12129.535 | 20588.914 | 20588.914 | 5691.821 | 0 | 0 | 0 |

## Stable Trend Check

- No metric crossed the stable trend rule: at least two thirds of comparable rounds improved and mean improvement was at least 5%.

## Live Guarded Contract

- CPU interference round 1: no inference_tail_guard trigger, no rollback.
- CPU interference round 2: 4 action audit error(s).
- CPU + optional I/O interference round 1: no inference_tail_guard trigger, no rollback.
- CPU + optional I/O interference round 2: no inference_tail_guard trigger, no rollback.

## Interpretation

- `dry_run` and `noop_observation` validate recognition, trigger, audit, and rollback paths but do not by themselves prove host-level performance benefit.
- Host-level MVP benefit requires a real guarded actuator run to show a stable downward trend in tail latency, TTFT, or jitter.
- If live `renice` is denied by host permissions, the report remains a closed-loop validation artifact, not a benefit proof.

## Artifacts

- Detail CSV: `/home/gg/AegisAI_Runtime/.cache/aegisai/inference_tail_guard_phase4/phase4_mvp_benefit_multiround/phase4_runs.csv`
- Aggregate CSV: `/home/gg/AegisAI_Runtime/.cache/aegisai/inference_tail_guard_phase4/phase4_mvp_benefit_multiround/phase4_aggregate.csv`
