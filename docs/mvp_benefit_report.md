# MVP Benefit Report

## Verdict

- Result: `FAIL`
- Conclusion: MVP benefit not proven: no live guarded mode met the stable improvement threshold.
- Run ID: `live_guarded_phase4_calibrated_20260510T043859Z`

## Controls

- Model: `qwen2.5:0.5b`
- Num predict: `32`
- Rounds per scenario: `3`
- Samples per mode: `4`
- Concurrency: `2`
- Modes: `baseline,noop_observation,dry_run,live_guarded`
- Scenarios: `cpu`
- Interference shape: `cpu_workers=1; io_workers=0; hdd_workers=0; hdd_bytes=128M`
- Live actuator confirmation: `1`
- Live PID allowlist: `2029`
- Live affinity enabled: `1`

## Aggregate Comparison

| scenario | mode | rounds | samples | TTFT P95 mean | TTFT P99 mean | lat P95 mean | lat P99 mean | jitter mean | cpu mig total | maj fault total | TTFT P95 delta % | TTFT P99 delta % | lat P95 delta % | lat P99 delta % | jitter delta % | live effective actions | live priority-limited |
| --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- |
| CPU interference | baseline | 3/3 | 12/12 | 15931.045 | 15931.045 | 30791.519 | 30791.519 | 8769.607 | 0 | 0 | 0.00 | 0.00 | 0.00 | 0.00 | 0.00 | 0 | 0 |
| CPU interference | dry_run | 3/3 | 12/12 | 17952.846 | 17952.846 | 33673.275 | 33673.275 | 9256.934 | 155 | 0 | -12.69 | -12.69 | -9.36 | -9.36 | -5.56 | 0 | 0 |
| CPU interference | live_guarded | 3/3 | 12/12 | 16937.616 | 16937.616 | 32296.528 | 32296.528 | 8946.017 | 551 | 0 | -6.32 | -6.32 | -4.89 | -4.89 | -2.01 | 3 | 3 |
| CPU interference | noop_observation | 3/3 | 12/12 | 17218.930 | 17218.930 | 33031.618 | 33031.618 | 9273.479 | 169 | 0 | -8.08 | -8.08 | -7.28 | -7.28 | -5.75 | 0 | 0 |

## Per-Round Comparison

| scenario | round | status | mode | ok/total | TTFT P95 | TTFT P99 | lat P95 | lat P99 | jitter | triggers | rollbacks | action errors | cpu mig total | maj fault total | live effective actions | live priority-limited |
| --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- |
| CPU interference | 1 | 0 | baseline | 4/4 | 14804.951 | 14804.951 | 31377.233 | 31377.233 | 9337.818 | 0 | 0 | 0 | 0 | 0 | 0 | 0 |
| CPU interference | 1 | 0 | noop_observation | 4/4 | 17304.248 | 17304.248 | 32386.890 | 32386.890 | 9132.110 | 2 | 2 | 0 | 57 | 0 | 0 | 0 |
| CPU interference | 1 | 0 | dry_run | 4/4 | 17812.327 | 17812.327 | 33315.716 | 33315.716 | 9207.516 | 2 | 2 | 0 | 55 | 0 | 0 | 0 |
| CPU interference | 1 | 0 | live_guarded | 4/4 | 17993.195 | 17993.195 | 32085.153 | 32085.153 | 8922.126 | 21 | 21 | 0 | 169 | 0 | 1 | 1 |
| CPU interference | 2 | 0 | baseline | 4/4 | 17176.310 | 17176.310 | 30038.282 | 30038.282 | 8277.314 | 0 | 0 | 0 | 0 | 0 | 0 | 0 |
| CPU interference | 2 | 0 | noop_observation | 4/4 | 16637.461 | 16637.461 | 32585.830 | 32585.830 | 9600.796 | 3 | 3 | 0 | 63 | 0 | 0 | 0 |
| CPU interference | 2 | 0 | dry_run | 4/4 | 19204.631 | 19204.631 | 34422.905 | 34422.905 | 9166.452 | 2 | 2 | 0 | 37 | 0 | 0 | 0 |
| CPU interference | 2 | 0 | live_guarded | 4/4 | 15263.055 | 15263.055 | 30226.151 | 30226.151 | 8361.363 | 19 | 19 | 0 | 185 | 0 | 1 | 1 |
| CPU interference | 3 | 0 | baseline | 4/4 | 15811.875 | 15811.875 | 30959.043 | 30959.043 | 8693.690 | 0 | 0 | 0 | 0 | 0 | 0 | 0 |
| CPU interference | 3 | 0 | noop_observation | 4/4 | 17715.080 | 17715.080 | 34122.135 | 34122.135 | 9087.531 | 3 | 3 | 0 | 49 | 0 | 0 | 0 |
| CPU interference | 3 | 0 | dry_run | 4/4 | 16841.579 | 16841.579 | 33281.204 | 33281.204 | 9396.834 | 2 | 2 | 0 | 63 | 0 | 0 | 0 |
| CPU interference | 3 | 0 | live_guarded | 4/4 | 17556.598 | 17556.598 | 34578.280 | 34578.280 | 9554.562 | 23 | 23 | 0 | 197 | 0 | 1 | 1 |

## Stable Trend Check

- No metric crossed the stable trend rule: at least two thirds of comparable rounds improved and mean improvement was at least 5%.

## Live Guarded Contract

- No live guarded mode contract failures were recorded.
- Selected mode contracts: `PASS`.
- Live effective host-level actuator changes: `3`.
- Live priority-limited/no-op nice applications: `3`.

## Interpretation

- `dry_run` and `noop_observation` validate recognition, trigger, audit, and rollback paths but do not by themselves prove host-level performance benefit.
- `cpu_migration` and `major_page_fault` columns are procfs-backed explainability signals for the run shape; they do not replace the live guarded latency benefit rule.
- `offcpu_time` can be sourced from the real eBPF helper when available, but it is not a blocking benefit gate in this report.
- Host-level MVP benefit requires a real guarded actuator run to show a stable downward trend in tail latency, TTFT, or jitter.
- If live `renice` is denied by host permissions, the report remains a closed-loop validation artifact, not a benefit proof.

## Artifacts

- Detail CSV: `/home/gg/AegisAI_Runtime/.cache/aegisai/inference_tail_guard_phase4/live_guarded_phase4_calibrated_20260510T043859Z/phase4_runs.csv`
- Aggregate CSV: `/home/gg/AegisAI_Runtime/.cache/aegisai/inference_tail_guard_phase4/live_guarded_phase4_calibrated_20260510T043859Z/phase4_aggregate.csv`
