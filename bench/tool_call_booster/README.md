# Tool Call Booster Benchmark

用于验证工具链调用场景是否存在稳定可观察的系统优化收益。

## 第一轮建议

- 固定 tool executor 样本
- 固定 retrieval / rerank worker 样本
- 记录 end-to-end tool call latency 和子链路耗时

## Phase 5 real executor harness

Phase 2R 通过后，Tool Call Booster 小阶段改为真实本地 tool executor
进程树，不再只依赖 mock lifecycle 回放：

```bash
bash bench/scripts/tool_call_booster_real_executor_harness.sh
```

该 harness 会启动 `real_tool_executor.py`，创建真实的
tool-executor / retrieval-worker / rerank-worker / background-worker 进程树，
再用 runtime daemon 的 `linux` + `procfs` source 观察同一个
`tool_call_id`。默认先跑 `noop`，并追加一轮 `linux-command-dry-run`，输出
`tool_call_booster_summary.csv`、daemon stdout/stderr、executor stdout/stderr 到
`.cache/aegisai/tool_call_booster/<run_id>/`。

当前验收口径：

- executor stdout 至少出现 4 个真实角色
- daemon summary 捕获 `tool_call_lifecycles`
- lifecycle stages 覆盖 executor / retrieval / rerank
- `tool_call_booster` 至少触发一次 action，并完成可回滚链路

常用缩短 smoke：

```bash
AEGISAI_TCB_EXECUTOR_CPU_MS=900 \
AEGISAI_TCB_WORKER_CPU_MS=1400 \
AEGISAI_TCB_WORKER_IO_KB=64 \
AEGISAI_TCB_DAEMON_MAX_EVENTS=40 \
  bash bench/scripts/tool_call_booster_real_executor_harness.sh
```

说明：本阶段只把 mock lifecycle 升级为真实 tool executor harness，并固化
真实进程生命周期识别与 noop/dry-run 执行审计。background isolation 和
explain/tune 的正式固化仍留在下一小阶段。
