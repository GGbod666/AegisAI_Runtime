# MVP Benefit Report

## Verdict

- Result: `PASS`
- Conclusion: MVP benefit observed: live_guarded shows a stable improvement trend with effective host-level actuator changes.
- Run ID: `live_guarded_phase4_sample_sizing_20260511T000000Z`
- Changed variable: `sample_sizing`

## Controls

- Tuned variable: `sample_sizing`
- Tuned variable detail: `Changed samples per mode from 4 to 8 versus live_guarded_phase4_calibrated_20260510T043859Z; stress worker count, concurrency, prompt/model, and affinity/nice pairing remain matched to the latest run; live PID allowlist is explicit for the current ollama serve process.`
- Model: `qwen2.5:0.5b`
- Num predict: `32`
- Rounds per scenario: `3`
- Samples per mode: `8`
- Concurrency: `2`
- Modes: `baseline,noop_observation,dry_run,live_guarded`
- Scenarios: `cpu`
- Interference shape: `cpu_workers=1; io_workers=0; hdd_workers=0; hdd_bytes=128M`
- Live actuator confirmation: `1`
- Live PID allowlist: `2130`
- Live affinity enabled: `1`

## Aggregate Comparison

| scenario | changed variable | mode | rounds | samples | TTFT P95 mean | TTFT P99 mean | lat P95 mean | lat P99 mean | jitter mean | cpu mig total | maj fault total | TTFT P95 delta % | TTFT P99 delta % | lat P95 delta % | lat P99 delta % | jitter delta % | live effective actions | live priority-limited |
| --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- |
| CPU interference | sample_sizing | baseline | 3/3 | 24/24 | 17882.410 | 17882.410 | 34309.526 | 34309.526 | 8945.077 | 0 | 0 | 0.00 | 0.00 | 0.00 | 0.00 | 0.00 | 0 | 0 |
| CPU interference | sample_sizing | dry_run | 3/3 | 24/24 | 18814.851 | 18814.851 | 34908.679 | 34908.679 | 8828.700 | 269 | 0 | -5.21 | -5.21 | -1.75 | -1.75 | 1.30 | 0 | 0 |
| CPU interference | sample_sizing | live_guarded | 3/3 | 24/24 | 17974.502 | 17974.502 | 33314.581 | 33314.581 | 8371.052 | 1225 | 1 | -0.51 | -0.51 | 2.90 | 2.90 | 6.42 | 3 | 3 |
| CPU interference | sample_sizing | noop_observation | 3/3 | 24/24 | 19564.538 | 19564.538 | 35215.049 | 35215.049 | 8821.581 | 164 | 0 | -9.41 | -9.41 | -2.64 | -2.64 | 1.38 | 0 | 0 |

## Per-Round Comparison

| scenario | round | changed variable | status | mode | ok/total | TTFT P95 | TTFT P99 | lat P95 | lat P99 | jitter | triggers | rollbacks | action errors | cpu mig total | maj fault total | live effective actions | live priority-limited |
| --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- |
| CPU interference | 1 | sample_sizing | 0 | baseline | 8/8 | 16599.205 | 16599.205 | 31859.407 | 31859.407 | 8377.712 | 0 | 0 | 0 | 0 | 0 | 0 | 0 |
| CPU interference | 1 | sample_sizing | 0 | noop_observation | 8/8 | 18841.018 | 18841.018 | 34219.734 | 34219.734 | 8866.630 | 3 | 3 | 0 | 56 | 0 | 0 | 0 |
| CPU interference | 1 | sample_sizing | 0 | dry_run | 8/8 | 18613.074 | 18613.074 | 34245.467 | 34245.467 | 8552.931 | 5 | 5 | 0 | 96 | 0 | 0 | 0 |
| CPU interference | 1 | sample_sizing | 0 | live_guarded | 8/8 | 18523.912 | 18523.912 | 32488.064 | 32488.064 | 7669.485 | 41 | 41 | 0 | 396 | 0 | 1 | 1 |
| CPU interference | 2 | sample_sizing | 0 | baseline | 8/8 | 19766.641 | 19766.641 | 38708.091 | 38708.091 | 9940.574 | 0 | 0 | 0 | 0 | 0 | 0 | 0 |
| CPU interference | 2 | sample_sizing | 0 | noop_observation | 8/8 | 20740.355 | 20740.355 | 37407.963 | 37407.963 | 9118.086 | 3 | 3 | 0 | 60 | 0 | 0 | 0 |
| CPU interference | 2 | sample_sizing | 0 | dry_run | 8/8 | 19905.275 | 19905.275 | 36198.610 | 36198.610 | 9377.881 | 2 | 2 | 0 | 55 | 0 | 0 | 0 |
| CPU interference | 2 | sample_sizing | 0 | live_guarded | 8/8 | 18065.193 | 18065.193 | 32550.273 | 32550.273 | 8335.873 | 41 | 41 | 0 | 408 | 1 | 1 | 1 |
| CPU interference | 3 | sample_sizing | 0 | baseline | 8/8 | 17281.385 | 17281.385 | 32361.079 | 32361.079 | 8516.945 | 0 | 0 | 0 | 0 | 0 | 0 | 0 |
| CPU interference | 3 | sample_sizing | 0 | noop_observation | 8/8 | 19112.242 | 19112.242 | 34017.449 | 34017.449 | 8480.026 | 2 | 2 | 0 | 48 | 0 | 0 | 0 |
| CPU interference | 3 | sample_sizing | 0 | dry_run | 8/8 | 17926.204 | 17926.204 | 34281.959 | 34281.959 | 8555.287 | 6 | 6 | 0 | 118 | 0 | 0 | 0 |
| CPU interference | 3 | sample_sizing | 0 | live_guarded | 8/8 | 17334.400 | 17334.400 | 34905.406 | 34905.406 | 9107.799 | 43 | 43 | 0 | 421 | 0 | 1 | 1 |

## Stable Trend Check

- CPU interference / live_guarded / Jitter: 2/3 rounds improved, mean delta 5.89%.

## Failure Diagnosis

- Failure cause: `none`.
- Evidence: Live guarded met the stable trend rule with effective host-level actuator changes.
- Comparable live guarded rounds per metric: `3`.
- Minimum observed live guarded samples per successful round: `8`.
- Configured minimum for benefit proof: `rounds>=3; samples_per_mode>=3`; observed live guarded samples must also be at least 3.

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

- Detail CSV: `/home/gg/AegisAI_Runtime/.cache/aegisai/inference_tail_guard_phase4/live_guarded_phase4_sample_sizing_20260511T000000Z/phase4_runs.csv`
- Aggregate CSV: `/home/gg/AegisAI_Runtime/.cache/aegisai/inference_tail_guard_phase4/live_guarded_phase4_sample_sizing_20260511T000000Z/phase4_aggregate.csv`
