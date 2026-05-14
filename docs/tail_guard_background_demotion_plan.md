# Tail Guard Background Demotion Dry-Run Plan

- Generated: `2026-05-14T06:50:19Z`
- Run ID: `codex_tail_background_demotion_20260514T000000Z`
- Mode: `dry_run_only`
- Runtime behavior: `not_connected`
- Live mutation: `false`
- Verdict: `PASS`
- Protected inference PIDs: `1`
- Candidate background PIDs: `0`
- Affected set bound: `0/8`
- Unknown processes rejected: `241`
- Interactive-sensitive processes rejected: `6`
- Limit rejections: `0`

## Proposed Controls

- `nice`: increase background candidates by `+5`, capped at nice `19`.
- `cpu.weight`: plan `50` only inside a future administrator-created AegisAI-owned cgroup v2 subtree.
- `cpu.max`: keep `max 100000` in this dry run; hard quota remains for the guarded owned-cgroup applier issue.
- Rollback capture requirements: `current_nice,cgroup_path,cpus_allowed_list,cpu.weight,cpu.max,cgroup.procs membership`.

## Process Decisions

- Full process decision inventory is in `/home/gg/AegisAI_Runtime/.cache/aegisai/inference_tail_guard_background_demotion/codex_tail_background_demotion_20260514T000000Z/background_demotion_candidates.csv`.
- Markdown preview rows: `20/248`.

| pid | name | class | decision | reject | nice | proposed nice | cpu.weight |
| --- | --- | --- | --- | --- | --- | --- | --- |
| 1768 | ollama | protected_inference | reject | inference_name_or_cmdline | 0 | n/a | n/a |
| 1 | systemd | unknown | reject | unknown_classification | 0 | n/a | n/a |
| 2 | kthreadd | unknown | reject | unknown_classification | 0 | n/a | n/a |
| 3 | pool_workqueue_release | unknown | reject | unknown_classification | 0 | n/a | n/a |
| 4 | kworker/R-rcu_g | unknown | reject | unknown_classification | -20 | n/a | n/a |
| 5 | kworker/R-rcu_p | unknown | reject | unknown_classification | -20 | n/a | n/a |
| 6 | kworker/R-slub_ | unknown | reject | unknown_classification | -20 | n/a | n/a |
| 7 | kworker/R-netns | unknown | reject | unknown_classification | -20 | n/a | n/a |
| 8 | kworker/0:0-events | unknown | reject | unknown_classification | 0 | n/a | n/a |
| 10 | kworker/0:0H-events_highpri | unknown | reject | unknown_classification | -20 | n/a | n/a |
| 12 | kworker/R-mm_pe | unknown | reject | unknown_classification | -20 | n/a | n/a |
| 13 | rcu_tasks_kthread | unknown | reject | unknown_classification | 0 | n/a | n/a |
| 14 | rcu_tasks_rude_kthread | unknown | reject | unknown_classification | 0 | n/a | n/a |
| 15 | rcu_tasks_trace_kthread | unknown | reject | unknown_classification | 0 | n/a | n/a |
| 16 | ksoftirqd/0 | unknown | reject | unknown_classification | 0 | n/a | n/a |
| 17 | rcu_preempt | unknown | reject | unknown_classification | 0 | n/a | n/a |
| 18 | migration/0 | unknown | reject | unknown_classification | 0 | n/a | n/a |
| 19 | idle_inject/0 | unknown | reject | unknown_classification | 0 | n/a | n/a |
| 20 | cpuhp/0 | unknown | reject | unknown_classification | 0 | n/a | n/a |
| 21 | cpuhp/1 | unknown | reject | unknown_classification | 0 | n/a | n/a |

## Safety Boundary

- Unknown processes are rejected instead of inferred as safe background work.
- Interactive-latency-sensitive and protected inference processes are rejected even when they match an allowlist by name.
- This artifact does not call `renice`, `taskset`, or write `cpu.weight`, `cpu.max`, `cgroup.procs`, or any cgroup filesystem path.
- Future live cgroup writes remain blocked until the guarded owned-cgroup isolation applier supplies explicit confirmation and rollback evidence.

## Artifacts

- Plan JSON: `/home/gg/AegisAI_Runtime/.cache/aegisai/inference_tail_guard_background_demotion/codex_tail_background_demotion_20260514T000000Z/background_demotion_plan.json`
- Candidate CSV: `/home/gg/AegisAI_Runtime/.cache/aegisai/inference_tail_guard_background_demotion/codex_tail_background_demotion_20260514T000000Z/background_demotion_candidates.csv`
