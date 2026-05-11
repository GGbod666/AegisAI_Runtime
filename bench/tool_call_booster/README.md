# Tool Call Booster Benchmark

用于验证工具链调用场景是否存在稳定可观察的系统优化收益。

## 第一轮建议

- 固定 tool executor 样本
- 固定 retrieval / rerank worker 样本
- 记录 end-to-end tool call latency 和子链路耗时

## Phase 5 repeated A/B benefit harness

Phase 2R 通过后，Tool Call Booster 小阶段改为真实本地 tool executor
进程树，不再只依赖 mock lifecycle 回放。当前 harness 会重复运行
baseline / noop / dry_run 对照轮次，并生成收益判定报告：

```bash
bash bench/scripts/tool_call_booster_real_executor_harness.sh
```

该 harness 会启动 `real_tool_executor.py`，创建真实的
tool-executor / retrieval-worker / rerank-worker / background-worker 进程树，
再用 runtime daemon 的 `linux` + `procfs` source 观察同一个 `tool_call_id`。
默认跑 3 轮 `baseline,noop,dry_run`，输出
`tool_call_booster_detail.csv`、`tool_call_booster_summary.csv`、
`tool_call_booster_stage_effectiveness.csv`、
`tool_call_booster_benefit_report.md`、daemon stdout/stderr、executor stdout/stderr
到 `.cache/aegisai/tool_call_booster/<run_id>/`。

当前验收口径：

- baseline 记录未观测 executor / retrieval / rerank critical chain latency，
  作为同轮对照；background worker 耗时只作为干扰观测，不计入收益 latency
- executor stdout 至少出现 4 个真实角色
- executor / retrieval / rerank 三段都必须产出 latency，否则该轮 contract FAIL
- noop / dry_run / guarded 档 daemon summary 捕获 `tool_call_lifecycles`
- lifecycle stages 覆盖 executor / retrieval / rerank
- `tool_call_booster` 至少触发一次 action，并完成可回滚链路
- summary 报告每档 latency delta、trigger count、rollback count 和明确
  `benefit_verdict`
- stage effectiveness 报告按 executor / retrieval / rerank 关联同轮 baseline
  latency delta、stage trigger count 和可追溯到 `tool_call_stage` 的有效 scheduler
  action count，用于判断哪段真正被隔离、哪段 latency 改善
- runtime daemon 的 apply detail audit 会内联 `tool_call_stage`、`tool_call_id`、
  `action_kind` 和 `effective`；summarizer 优先使用这些字段归因 stage
  effectiveness，旧 artifact 缺少内联字段时只保留能从 PID/stage audit 推断出的归因

当前 benefit 证明边界只覆盖 scheduler 侧动作：`nice`，以及显式打开 live affinity
时的 `taskset`。`WarmupExecutor` 默认仍是 deferred/no-side-effect audit；只有
daemon 使用 `--warmup-executor-command`、`--warmup-executor-arg` 和正数
`--warmup-executor-timeout-ms` 显式配置时，command backend 才会运行一个受超时约束
的 executor/cache warmup 命令。rollback 始终记录 no-op，不尝试反向清理 cache 或
短命预热进程。

默认 `noop` 与 `dry_run` 只证明识别、触发、审计和 rollback 闭环；它们不会
单独被判定为主机级收益证明。只有显式加入 guarded/live 档，至少一个 guarded
stage 的 `stage_effectiveness` 为 `PASS`，并在至少三分之二可比较轮次中相对
baseline 改善达到 `AEGISAI_TCB_MIN_BENEFIT_PCT`，报告才会给出 benefit `PASS`。

最新稳定 executor-control artifact：

- run id: `live_guarded_tcb_stable_executor_20260511T000000Z`
- modes: `baseline,noop_observation,dry_run,live_guarded`
- verdict: contract `PASS`, benefit `FAIL`
- reason: `live_guarded` 只有 `0/3` 可比较轮次达到 `5.0%` latency improvement

常用缩短 smoke：

```bash
AEGISAI_TCB_ROUNDS=2 \
AEGISAI_TCB_EXECUTOR_CPU_MS=900 \
AEGISAI_TCB_WORKER_CPU_MS=1400 \
AEGISAI_TCB_WORKER_IO_KB=64 \
AEGISAI_TCB_DAEMON_MAX_EVENTS=40 \
  bash bench/scripts/tool_call_booster_real_executor_harness.sh
```

常用覆盖：

```bash
AEGISAI_TCB_ROUNDS=3 \
AEGISAI_TCB_MODES=baseline,noop,dry_run \
AEGISAI_TCB_MIN_BENEFIT_PCT=5 \
  bash bench/scripts/tool_call_booster_real_executor_harness.sh
```

需要把 benefit verdict 作为 shell hard gate 时，设置
`AEGISAI_TCB_REQUIRE_BENEFIT=1`。这通常只适合显式受控的 guarded/live 实验窗口。
`live_guarded` 还必须设置 `AEGISAI_CONFIRM_LIVE_ACTUATOR=1`；可以用
`AEGISAI_LIVE_PID_ALLOWLIST=<pid,...>` 固定 allowlist，未设置时 harness 会从
当前轮次的 executor / retrieval / rerank / background 进程树派生 allowlist。
当 modes 包含 live guarded 档且未显式设置 `AEGISAI_ENABLE_LIVE_AFFINITY` 时，
该 harness 默认启用 live affinity，让受控收益证明包含实际可生效的 `taskset`
隔离动作；设置 `AEGISAI_ENABLE_LIVE_AFFINITY=0` 可退回 nice-only 验证。
需要显式测试 warmup side effect 时，设置
`AEGISAI_TCB_WARMUP_EXECUTOR_COMMAND`、可选
`AEGISAI_TCB_WARMUP_EXECUTOR_ARGS` 和
`AEGISAI_TCB_WARMUP_EXECUTOR_TIMEOUT_MS`；报告会把 warmup side effect 计数与
scheduler benefit verdict 分开。
