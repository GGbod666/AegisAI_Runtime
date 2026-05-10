# MVP Benefit Report

## Verdict

- Result: `FAIL`
- Conclusion: live action is effective, but stable benefit is below threshold; MVP benefit is not proven.
- Run ID: `live_affinity_online_fix_phase4_20260503T043809Z`

## Controls

- Model: `qwen2.5:0.5b`
- Rounds per scenario: `3`
- Samples per mode: `4`
- Concurrency: `2`
- Modes: `baseline,noop_observation,dry_run,live_guarded`

## Aggregate Comparison

| scenario | mode | rounds | samples | TTFT P95 mean | TTFT P99 mean | lat P95 mean | lat P99 mean | jitter mean | cpu mig total | maj fault total | TTFT P95 delta % | TTFT P99 delta % | lat P95 delta % | lat P99 delta % | jitter delta % | live effective actions | live priority-limited |
| --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- |
| CPU interference | baseline | 3/3 | 12/12 | 36364.664 | 36364.664 | 64581.843 | 64581.843 | 18019.105 | 0 | 0 | 0.00 | 0.00 | 0.00 | 0.00 | 0.00 | 0 | 0 |
| CPU interference | dry_run | 3/3 | 12/12 | 27150.718 | 27150.718 | 58309.693 | 58309.693 | 18160.665 | 124 | 0 | 25.34 | 25.34 | 9.71 | 9.71 | -0.79 | 0 | 0 |
| CPU interference | live_guarded | 3/3 | 12/12 | 34950.017 | 34950.017 | 63317.628 | 63317.628 | 19525.786 | 76 | 0 | 3.89 | 3.89 | 1.96 | 1.96 | -8.36 | 3 | 3 |
| CPU interference | noop_observation | 3/3 | 12/12 | 34444.411 | 34444.411 | 66511.966 | 66511.966 | 19563.392 | 125 | 0 | 5.28 | 5.28 | -2.99 | -2.99 | -8.57 | 0 | 0 |

## Per-Round Comparison

| scenario | round | status | mode | ok/total | TTFT P95 | TTFT P99 | lat P95 | lat P99 | jitter | triggers | rollbacks | action errors | cpu mig total | maj fault total | live effective actions | live priority-limited |
| --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- |
| CPU interference | 1 | 0 | baseline | 4/4 | 40546.589 | 40546.589 | 62905.635 | 62905.635 | 17038.862 | 0 | 0 | 0 | 0 | 0 | 0 | 0 |
| CPU interference | 1 | 0 | noop_observation | 4/4 | 32963.896 | 32963.896 | 62305.455 | 62305.455 | 18111.777 | 7 | 7 | 0 | 38 | 0 | 0 | 0 |
| CPU interference | 1 | 0 | dry_run | 4/4 | 26374.187 | 26374.187 | 55898.739 | 55898.739 | 17393.654 | 3 | 3 | 0 | 44 | 0 | 0 | 0 |
| CPU interference | 1 | 0 | live_guarded | 4/4 | 36575.216 | 36575.216 | 69688.999 | 69688.999 | 22389.770 | 4 | 4 | 0 | 21 | 0 | 1 | 1 |
| CPU interference | 2 | 0 | baseline | 4/4 | 36691.510 | 36691.510 | 68119.801 | 68119.801 | 18998.677 | 0 | 0 | 0 | 0 | 0 | 0 | 0 |
| CPU interference | 2 | 0 | noop_observation | 4/4 | 31956.518 | 31956.518 | 63568.463 | 63568.463 | 19581.171 | 7 | 7 | 0 | 49 | 0 | 0 | 0 |
| CPU interference | 2 | 0 | dry_run | 4/4 | 22312.608 | 22312.608 | 52927.078 | 52927.078 | 17198.661 | 4 | 4 | 0 | 51 | 0 | 0 | 0 |
| CPU interference | 2 | 0 | live_guarded | 4/4 | 32387.006 | 32387.006 | 56165.264 | 56165.264 | 17737.476 | 3 | 3 | 0 | 20 | 0 | 1 | 1 |
| CPU interference | 3 | 0 | baseline | 4/4 | 31855.894 | 31855.894 | 62720.093 | 62720.093 | 18019.775 | 0 | 0 | 0 | 0 | 0 | 0 | 0 |
| CPU interference | 3 | 0 | noop_observation | 4/4 | 38412.819 | 38412.819 | 73661.980 | 73661.980 | 20997.227 | 3 | 3 | 0 | 38 | 0 | 0 | 0 |
| CPU interference | 3 | 0 | dry_run | 4/4 | 32765.360 | 32765.360 | 66103.262 | 66103.262 | 19889.680 | 2 | 2 | 0 | 29 | 0 | 0 | 0 |
| CPU interference | 3 | 0 | live_guarded | 4/4 | 35887.829 | 35887.829 | 64098.622 | 64098.622 | 18450.112 | 4 | 4 | 0 | 35 | 0 | 1 | 1 |

## Stable Trend Check

- CPU interference / dry_run / TTFT P95: 2/3 rounds improved, mean delta 23.76%.
- CPU interference / dry_run / TTFT P99: 2/3 rounds improved, mean delta 23.76%.
- CPU interference / dry_run / Latency P95: 2/3 rounds improved, mean delta 9.35%.
- CPU interference / dry_run / Latency P99: 2/3 rounds improved, mean delta 9.35%.
- Apparent improvements were limited to observation or dry-run modes, so they are treated as non-proof for MVP benefit.

## Live Guarded Contract

- No live guarded mode contract failures were recorded.
- Live guarded recorded `3` effective host-level `taskset` actions.

## Interpretation

- `dry_run` and `noop_observation` validate recognition, trigger, audit, and rollback paths but do not by themselves prove host-level performance benefit.
- `cpu_migration` and `major_page_fault` columns are procfs-backed explainability signals for the run shape; they do not replace the live guarded latency benefit rule.
- `offcpu_time` can be sourced from the real eBPF helper when available, but it is not a blocking benefit gate in this report.
- Host-level MVP benefit requires a real guarded actuator run to show a stable downward trend in tail latency, TTFT, or jitter.
- This run clears the effective live action gate through host-level `taskset`, but it remains a `FAIL` because stable repeated benefit did not cross the threshold.

## Artifacts

| run id | CSV | live effective action count | FAIL reason |
| --- | --- | --- | --- |
| `live_affinity_online_fix_phase4_20260503T043809Z` | `.cache/aegisai/inference_tail_guard_phase4/live_affinity_online_fix_phase4_20260503T043809Z/phase4_runs.csv` | `3` | live action is effective, but stable benefit is below threshold |
| `live_affinity_online_fix_phase4_20260503T043809Z` | `.cache/aegisai/inference_tail_guard_phase4/live_affinity_online_fix_phase4_20260503T043809Z/phase4_aggregate.csv` | `3` | live action is effective, but stable benefit is below threshold |
