# Tool Call Booster Benchmark

用于验证工具链调用场景是否存在稳定可观察的系统优化收益。

## 第一轮建议

- 固定 tool executor 样本
- 固定 retrieval / rerank worker 样本
- 记录 end-to-end tool call latency 和子链路耗时
