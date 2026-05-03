# MVP Benefit Report

## Verdict

- Result: `FAIL`
- Conclusion: MVP benefit not proven: live_guarded trend was observed, but live actuator changes were priority-limited or no-op.
- Run ID: `phase2r4_short16_20260502T070201Z`

## Controls

- Model: `qwen2.5:0.5b`
- Rounds per scenario: `3`
- Samples per mode: `8`
- Concurrency: `2`
- Modes: `baseline,noop_observation,dry_run,live_guarded`

## Aggregate Comparison

| scenario | mode | rounds | samples | TTFT P95 mean | TTFT P99 mean | lat P95 mean | lat P99 mean | jitter mean | cpu mig total | maj fault total | TTFT P95 delta % | TTFT P99 delta % | lat P95 delta % | lat P99 delta % | jitter delta % | live effective actions | live priority-limited |
| --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- |
| CPU interference | baseline | 3/3 | 24/24 | 34964.672 | 34964.672 | 61837.992 | 61837.992 | 14935.755 | 0 | 0 | 0.00 | 0.00 | 0.00 | 0.00 | 0.00 | 0 | 0 |
| CPU interference | dry_run | 3/3 | 24/24 | 33816.229 | 33816.229 | 61537.913 | 61537.913 | 14779.090 | 118 | 0 | 3.28 | 3.28 | 0.49 | 0.49 | 1.05 | 0 | 0 |
| CPU interference | live_guarded | 3/3 | 24/24 | 32351.160 | 32351.160 | 64134.460 | 64134.460 | 17063.519 | 159 | 0 | 7.47 | 7.47 | -3.71 | -3.71 | -14.25 | 0 | 3 |
| CPU interference | noop_observation | 3/3 | 24/24 | 35064.320 | 35064.320 | 65673.914 | 65673.914 | 16919.595 | 160 | 0 | -0.28 | -0.28 | -6.20 | -6.20 | -13.28 | 0 | 0 |
| CPU + optional I/O interference | baseline | 3/3 | 24/24 | 42068.740 | 42068.740 | 77714.493 | 77714.493 | 19008.715 | 0 | 0 | 0.00 | 0.00 | 0.00 | 0.00 | 0.00 | 0 | 0 |
| CPU + optional I/O interference | dry_run | 3/3 | 24/24 | 40115.180 | 40115.180 | 74804.310 | 74804.310 | 18727.965 | 442 | 0 | 4.64 | 4.64 | 3.74 | 3.74 | 1.48 | 0 | 0 |
| CPU + optional I/O interference | live_guarded | 3/3 | 24/24 | 39849.342 | 39849.342 | 73734.691 | 73734.691 | 18113.998 | 78 | 0 | 5.28 | 5.28 | 5.12 | 5.12 | 4.71 | 0 | 3 |
| CPU + optional I/O interference | noop_observation | 3/3 | 24/24 | 40885.602 | 40885.602 | 73335.330 | 73335.330 | 18406.046 | 347 | 0 | 2.81 | 2.81 | 5.63 | 5.63 | 3.17 | 0 | 0 |

## Per-Round Comparison

| scenario | round | status | mode | ok/total | TTFT P95 | TTFT P99 | lat P95 | lat P99 | jitter | triggers | rollbacks | action errors | cpu mig total | maj fault total | live effective actions | live priority-limited |
| --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- |
| CPU interference | 1 | 0 | baseline | 8/8 | 37635.933 | 37635.933 | 62138.153 | 62138.153 | 14483.087 | 0 | 0 | 0 | 0 | 0 | 0 | 0 |
| CPU interference | 1 | 0 | noop_observation | 8/8 | 34385.339 | 34385.339 | 68107.012 | 68107.012 | 16997.153 | 6 | 6 | 0 | 78 | 0 | 0 | 0 |
| CPU interference | 1 | 0 | dry_run | 8/8 | 32779.215 | 32779.215 | 55105.092 | 55105.092 | 13148.439 | 3 | 3 | 0 | 47 | 0 | 0 | 0 |
| CPU interference | 1 | 0 | live_guarded | 8/8 | 28436.734 | 28436.734 | 61942.239 | 61942.239 | 16733.230 | 10 | 10 | 0 | 68 | 0 | 0 | 1 |
| CPU interference | 2 | 0 | baseline | 8/8 | 31713.140 | 31713.140 | 57304.242 | 57304.242 | 14324.609 | 0 | 0 | 0 | 0 | 0 | 0 | 0 |
| CPU interference | 2 | 0 | noop_observation | 8/8 | 33933.983 | 33933.983 | 62145.047 | 62145.047 | 15982.035 | 3 | 3 | 0 | 40 | 0 | 0 | 0 |
| CPU interference | 2 | 0 | dry_run | 8/8 | 32998.211 | 32998.211 | 58887.638 | 58887.638 | 13961.883 | 3 | 3 | 0 | 27 | 0 | 0 | 0 |
| CPU interference | 2 | 0 | live_guarded | 8/8 | 33163.905 | 33163.905 | 62062.717 | 62062.717 | 17572.765 | 12 | 12 | 0 | 74 | 0 | 0 | 1 |
| CPU interference | 3 | 0 | baseline | 8/8 | 35544.942 | 35544.942 | 66071.581 | 66071.581 | 15999.570 | 0 | 0 | 0 | 0 | 0 | 0 | 0 |
| CPU interference | 3 | 0 | noop_observation | 8/8 | 36873.638 | 36873.638 | 66769.684 | 66769.684 | 17779.596 | 5 | 5 | 0 | 42 | 0 | 0 | 0 |
| CPU interference | 3 | 0 | dry_run | 8/8 | 35671.261 | 35671.261 | 70621.009 | 70621.009 | 17226.949 | 5 | 5 | 0 | 44 | 0 | 0 | 0 |
| CPU interference | 3 | 0 | live_guarded | 8/8 | 35452.840 | 35452.840 | 68398.424 | 68398.424 | 16884.561 | 4 | 4 | 0 | 17 | 0 | 0 | 1 |
| CPU + optional I/O interference | 1 | 0 | baseline | 8/8 | 41065.216 | 41065.216 | 79308.064 | 79308.064 | 19842.236 | 0 | 0 | 0 | 0 | 0 | 0 | 0 |
| CPU + optional I/O interference | 1 | 0 | noop_observation | 8/8 | 39715.218 | 39715.218 | 69197.362 | 69197.362 | 17359.675 | 4 | 4 | 0 | 76 | 0 | 0 | 0 |
| CPU + optional I/O interference | 1 | 0 | dry_run | 8/8 | 39371.210 | 39371.210 | 70476.578 | 70476.578 | 18500.190 | 8 | 8 | 0 | 129 | 0 | 0 | 0 |
| CPU + optional I/O interference | 1 | 0 | live_guarded | 8/8 | 37739.947 | 37739.947 | 69048.968 | 69048.968 | 17366.770 | 5 | 5 | 0 | 30 | 0 | 0 | 1 |
| CPU + optional I/O interference | 2 | 0 | baseline | 8/8 | 41310.475 | 41310.475 | 75104.656 | 75104.656 | 17981.743 | 0 | 0 | 0 | 0 | 0 | 0 | 0 |
| CPU + optional I/O interference | 2 | 0 | noop_observation | 8/8 | 41362.987 | 41362.987 | 74836.009 | 74836.009 | 19545.136 | 7 | 7 | 0 | 122 | 0 | 0 | 0 |
| CPU + optional I/O interference | 2 | 0 | dry_run | 8/8 | 40272.744 | 40272.744 | 77794.469 | 77794.469 | 19621.189 | 9 | 9 | 0 | 148 | 0 | 0 | 0 |
| CPU + optional I/O interference | 2 | 0 | live_guarded | 8/8 | 39989.080 | 39989.080 | 69899.482 | 69899.482 | 17672.474 | 3 | 3 | 0 | 24 | 0 | 0 | 1 |
| CPU + optional I/O interference | 3 | 0 | baseline | 8/8 | 43830.530 | 43830.530 | 78730.758 | 78730.758 | 19202.167 | 0 | 0 | 0 | 0 | 0 | 0 | 0 |
| CPU + optional I/O interference | 3 | 0 | noop_observation | 8/8 | 41578.600 | 41578.600 | 75972.618 | 75972.618 | 18313.327 | 9 | 9 | 0 | 149 | 0 | 0 | 0 |
| CPU + optional I/O interference | 3 | 0 | dry_run | 8/8 | 40701.587 | 40701.587 | 76141.882 | 76141.882 | 18062.517 | 9 | 9 | 0 | 165 | 0 | 0 | 0 |
| CPU + optional I/O interference | 3 | 0 | live_guarded | 8/8 | 41818.998 | 41818.998 | 82255.623 | 82255.623 | 19302.749 | 4 | 4 | 0 | 24 | 0 | 0 | 1 |

## Stable Trend Check

- CPU interference / live_guarded / TTFT P95: 2/3 rounds improved, mean delta 6.71%.
- CPU interference / live_guarded / TTFT P99: 2/3 rounds improved, mean delta 6.71%.
- CPU + optional I/O interference / live_guarded / TTFT P95: 3/3 rounds improved, mean delta 5.30%.
- CPU + optional I/O interference / live_guarded / TTFT P99: 3/3 rounds improved, mean delta 5.30%.
- CPU + optional I/O interference / live_guarded / Latency P95: 2/3 rounds improved, mean delta 5.13%.
- CPU + optional I/O interference / live_guarded / Latency P99: 2/3 rounds improved, mean delta 5.13%.
- CPU + optional I/O interference / noop_observation / Latency P95: 3/3 rounds improved, mean delta 5.54%.
- CPU + optional I/O interference / noop_observation / Latency P99: 3/3 rounds improved, mean delta 5.54%.
- Live guarded trend is treated as non-proof because no effective live actuator changes were observed.

## Live Guarded Contract

- No live guarded mode contract failures were recorded.
- Live guarded recorded no effective host-level actuator changes; priority-limited no-op nice applications: 6.

## Interpretation

- `dry_run` and `noop_observation` validate recognition, trigger, audit, and rollback paths but do not by themselves prove host-level performance benefit.
- `cpu_migration` and `major_page_fault` columns are procfs-backed explainability signals for the run shape; they do not replace the live guarded latency benefit rule.
- `offcpu_time` remains an eBPF/future enhancement and is not a blocking benefit gate in this report.
- Host-level MVP benefit requires a real guarded actuator run to show a stable downward trend in tail latency, TTFT, or jitter.
- If live `renice` is denied by host permissions, the report remains a closed-loop validation artifact, not a benefit proof.

## Artifacts

- Detail CSV: `/home/gg/AegisAI_Runtime/.cache/aegisai/inference_tail_guard_phase4/phase2r4_short16_20260502T070201Z/phase4_runs.csv`
- Aggregate CSV: `/home/gg/AegisAI_Runtime/.cache/aegisai/inference_tail_guard_phase4/phase2r4_short16_20260502T070201Z/phase4_aggregate.csv`
