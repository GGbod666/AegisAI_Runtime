# Benchmark 规划

这一层改为按问题场景组织 benchmark，而不是只按工具或单一 demo 组织。

## 子目录

- `inference_tail_guard`：推理尾延迟验证
- `tool_call_booster`：工具链调用验证
- `ai_workload_awareness`：规则识别与标签覆盖验证
- `interference`：CPU / I/O / background 干扰定义
- `scripts`：实验辅助脚本

## 原则

- 优先 A/B 对照
- 优先稳定复现
- 优先 tail latency 与 jitter
- 把识别准确性也纳入验证项
