# Tail Guard Attribution Report

- Generated: `2026-05-14T06:55:14Z`
- Source Phase 4 detail CSV: `.cache/aegisai/inference_tail_guard_phase4/live_guarded_phase4_sample_sizing_20260511T000000Z/phase4_runs.csv`
- Run ID: `live_guarded_phase4_sample_sizing_20260511T000000Z`
- Baseline rounds: `3`
- Baseline latency P95 mean: `34309.526 ms`
- Baseline latency P99 mean: `34309.526 ms`
- Live latency P95 delta: `2.90%`
- Live latency P99 delta: `2.90%`
- Duration-backed scheduler-attributable tail max: `1.57%`
- Duration-backed scheduler-attributable tail mean: `0.88%`
- P95/P99 >=15% plausibility: `NOT_PROVEN_HELPER_GAP`
- Plausibility reason: Captured duration-backed scheduler signals peak at 1.57% of P95, below 15.0%, and helper-backed offcpu/io signals were excluded or zero in these artifacts.

## Signal Rollup

- Run-queue delay max: `567.592 ms`
- Helper off-CPU max: `0.000 ms`
- Helper I/O latency max: `0.000 ms`
- CPU migration total: `1658`
- Major page fault total: `1`
- Helper off-CPU events: `0`
- Helper I/O events: `0`
- Helper-backed signal status for Phase 5 planning: `excluded_or_zero`

## Trigger Apply Rollback Capture

- Live triggers: `125`
- Live rollbacks: `125`
- Live apply audit detail count: `9`
- Live rollback audit detail count: `6`
- Live effective host actions: `3`
- Timing capture status: `audit_counts_only`

## Per-Round Attribution

| scenario | round | mode | lat P95 | model/runtime P95 | runq max ms | offcpu max ms | I/O max ms | sched tail % | cpu mig | maj faults | triggers | apply audits | rollbacks | rollback audits | timing |
| --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- |
| CPU interference | 1 | baseline | 31859.407 | 15260.202 | 0.000 | 0.000 | 0.000 | 0.00 | 0 | 0 | 0 | 0 | 0 | 0 | not_applicable_baseline |
| CPU interference | 1 | noop_observation | 34219.734 | 15378.716 | 525.770 | 0.000 | 0.000 | 1.54 | 56 | 0 | 3 | 0 | 3 | 0 | not_captured |
| CPU interference | 1 | dry_run | 34245.467 | 15632.393 | 492.959 | 0.000 | 0.000 | 1.44 | 96 | 0 | 5 | 6 | 5 | 4 | audit_counts_only |
| CPU interference | 1 | live_guarded | 32488.064 | 13964.152 | 48.329 | 0.000 | 0.000 | 0.15 | 396 | 0 | 41 | 3 | 41 | 2 | audit_counts_only |
| CPU interference | 2 | baseline | 38708.091 | 18941.450 | 0.000 | 0.000 | 0.000 | 0.00 | 0 | 0 | 0 | 0 | 0 | 0 | not_applicable_baseline |
| CPU interference | 2 | noop_observation | 37407.963 | 16667.608 | 420.384 | 0.000 | 0.000 | 1.12 | 60 | 0 | 3 | 0 | 3 | 0 | not_captured |
| CPU interference | 2 | dry_run | 36198.610 | 16293.335 | 567.592 | 0.000 | 0.000 | 1.57 | 55 | 0 | 2 | 6 | 2 | 4 | audit_counts_only |
| CPU interference | 2 | live_guarded | 32550.273 | 14485.080 | 47.662 | 0.000 | 0.000 | 0.15 | 408 | 1 | 41 | 3 | 41 | 2 | audit_counts_only |
| CPU interference | 3 | baseline | 32361.079 | 15079.694 | 0.000 | 0.000 | 0.000 | 0.00 | 0 | 0 | 0 | 0 | 0 | 0 | not_applicable_baseline |
| CPU interference | 3 | noop_observation | 34017.449 | 14905.207 | 177.377 | 0.000 | 0.000 | 0.52 | 48 | 0 | 2 | 0 | 2 | 0 | not_captured |
| CPU interference | 3 | dry_run | 34281.959 | 16355.755 | 434.707 | 0.000 | 0.000 | 1.27 | 118 | 0 | 6 | 6 | 6 | 4 | audit_counts_only |
| CPU interference | 3 | live_guarded | 34905.406 | 17571.006 | 61.179 | 0.000 | 0.000 | 0.18 | 421 | 0 | 43 | 3 | 43 | 2 | audit_counts_only |

## Interpretation

- `model/runtime P95` is `latency_p95_ms - ttft_p95_ms`; it is the request body/runtime portion not explained by TTFT.
- `sched tail %` is duration-backed `run_queue_delay + offcpu_time + io_latency` max divided by latency P95, capped at 100%.
- CPU migration and major page fault columns are event-pressure attribution, not duration attribution; they should guide stronger isolation tests but are not added to duration-backed plausibility.
- Trigger/apply/rollback timing is currently audit-count based in Phase 4 artifacts; exact apply and rollback timestamps are a remaining instrumentation gap.
- Kernel cgroup v2 CPU controls support proportional `cpu.weight`, bandwidth `cpu.max`, and CPU pressure accounting, which is why the next live isolation work must stay scoped to an owned cgroup subtree.
- Kernel scheduler stats expose per-process runqueue wait time through `/proc/<pid>/schedstat`; this report uses the daemon's procfs-derived `run_queue_delay` observations as the runqueue-wait attribution source.

## References

- Linux kernel cgroup v2 CPU interface files: https://docs.kernel.org/admin-guide/cgroup-v2.html
- Linux kernel scheduler statistics and `/proc/<pid>/schedstat`: https://docs.kernel.org/scheduler/sched-stats.html

## Artifacts

- Attribution CSV: `.cache/aegisai/inference_tail_guard_tail_attribution/live_guarded_phase4_sample_sizing_20260511T000000Z/tail_attribution.csv`
- Summary JSON: `.cache/aegisai/inference_tail_guard_tail_attribution/live_guarded_phase4_sample_sizing_20260511T000000Z/tail_attribution_summary.json`
