# Tool Call Booster

目标：在工具调用生命周期内对相关进程组做轻量级保护，降低 end-to-end tool call latency。

## 依赖输入

- `TOOL_CALL`
- `RETRIEVAL_STAGE`
- `RERANK_STAGE`
- subprocess start delay
- queue wait
- optional I/O latency

## 第一版建议

- 识别 tool call 开始与结束
- 对 tool executor / retrieval worker 进行轻量 boost
- 生命周期结束后自动退出

## 关注指标

- tool call latency
- subprocess start delay
- retrieval / rerank 子链路耗时
