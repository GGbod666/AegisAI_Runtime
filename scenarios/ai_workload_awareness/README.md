# AI Workload Awareness

目标：在系统层稳定识别 AI workload，并给后续策略提供统一语义标签。

## 第一版规则

- 进程名匹配
- 命令行匹配
- cgroup 标记
- PID allowlist
- 父子进程关系识别

## 输出标签

- `AI_INFERENCE`
- `TOOL_CALL`
- `RETRIEVAL_STAGE`
- `RERANK_STAGE`
- `BACKGROUND_JOB`
- `INTERACTIVE_LATENCY_SENSITIVE`
