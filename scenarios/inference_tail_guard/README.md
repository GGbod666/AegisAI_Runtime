# Inference Tail Guard

目标：在 AI 推理关键路径上识别尾延迟风险信号，并触发短时、可回退的 boost 保护。

## 依赖输入

- `AI_INFERENCE`
- `INTERACTIVE_LATENCY_SENSITIVE`
- run queue delay
- off-CPU time
- CPU migration
- major page fault

## 首批动作

- 提高优先级
- CPU affinity 调整
- 可选 cpuset 隔离

## 首批评估指标

- TTFT
- P95/P99 latency
- jitter
