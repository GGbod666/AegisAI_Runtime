# Tool Call Booster

目标：在工具调用生命周期内对相关进程组做轻量级保护，降低 end-to-end tool call
latency，同时把每个动作能否真正改变 scheduler 状态记录清楚。

## 依赖输入

- `TOOL_CALL`
- `RETRIEVAL_STAGE`
- `RERANK_STAGE`
- subprocess start delay
- queue wait
- optional I/O latency
- `tool_call_id` from tags, cmdline, cgroup, or parent cmdline

## Stage Mapping

- `TOOL_CALL` only -> `executor`
- `TOOL_CALL + RETRIEVAL_STAGE` -> `retrieval`
- `TOOL_CALL + RERANK_STAGE` -> `rerank`
- background/interference processes remain audit context, not critical-chain
  benefit latency

## Actions And Audit

- policy may emit bounded `RaiseNice`, optional `SetAffinity`, optional
  `WarmupExecutor`, and audit-only cpuset intent depending on config and safety
  limits
- live scheduler benefit proof covers `nice` and explicitly enabled `affinity`
  only
- `WarmupExecutor` is reported separately; default apply is deferred/no-side
  effect unless a command backend is explicitly configured with a positive
  timeout, and rollback is always an audited no-op
- daemon apply detail audit records `tool_call_stage`, `tool_call_id`,
  `action_kind`, and `effective` inline so report code can attribute effective
  scheduler actions to executor / retrieval / rerank stages

## Verification

Primary entry point:

```bash
bash bench/scripts/tool_call_booster_real_executor_harness.sh
```

Default modes are `baseline,noop,dry_run`. A live guarded proof attempt must be
explicit:

```bash
AEGISAI_TCB_MODES=baseline,noop_observation,dry_run,live_guarded \
AEGISAI_CONFIRM_LIVE_ACTUATOR=1 \
  bash bench/scripts/tool_call_booster_real_executor_harness.sh
```

The harness writes `tool_call_booster_detail.csv`,
`tool_call_booster_summary.csv`, `tool_call_booster_stage_effectiveness.csv`,
and `tool_call_booster_benefit_report.md` under
`.cache/aegisai/tool_call_booster/<run_id>/`.

## Current Evidence

Latest stable executor-control run:
`live_guarded_tcb_stable_executor_20260511T000000Z`.

- contract verdict: `PASS`
- benefit verdict: `FAIL`
- `live_guarded` improved `0/3` comparable rounds by at least `5.0%`
- average delta versus same-round baseline: `1.077%`
- median delta versus same-round baseline: `0.200%`

This is a reproducible falsification for this host/run shape, not a product
benefit proof. `noop` and `dry_run` remain control evidence only.
