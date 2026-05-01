# Tool Call Booster Benchmark

用于验证工具链调用场景是否存在稳定可观察的系统优化收益。

## 第一轮建议

- 固定 tool executor 样本
- 固定 retrieval / rerank worker 样本
- 记录 end-to-end tool call latency 和子链路耗时

## Phase 5 mock lifecycle harness

当前可先用 runtime daemon 内置 harness 固定 lifecycle 证据：

```bash
cargo run -p aegisai-runtime-daemon -- \
  --repo-root . \
  --source mock \
  --mock-profile tool-call-lifecycle \
  --metadata noop \
  --actuator-backend noop
```

输出中的 `tool_call_lifecycles` 会汇总同一个 `tool_call_id` 下的
executor / retrieval / rerank / background 事件、boosted action 数和
isolation 审计事件数。这个 harness 用于固化 tool call 子链路识别与
explain/tune 报告输入；真实 benchmark 再接入外部 tool executor 样本。
