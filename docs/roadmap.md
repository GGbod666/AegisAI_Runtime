# Roadmap

_Updated: 2026-05-10_

## Current Position

The project has moved beyond the original framework and runnable-daemon setup
work. The active phase is evidence hardening:

- prove effective live `inference_tail_guard` benefit under the strict Phase 4
  gate
- complete real eBPF-backed off-CPU and I/O latency ingestion
- turn `tool_call_booster` from trigger proof into repeated A/B benefit proof
- harden tests around the actuator/runtime hot paths

See `docs/current_status.md` for the current state and beads issue IDs.
See `docs/task_list.md` for detailed active tasks.

## Phase 0：Framework Reset

目标：

- 固定项目定义
- 固定双轴骨架
- 固定配置层次
- 固定安全边界

交付：

- 更新后的仓库骨架
- 场景目录
- classifier / safety / scenario 配置入口

## Phase 1：Awareness Foundation

目标：

- 把 AI workload awareness 做成全局基础能力

范围：

- process / cmdline / cgroup / PID 规则识别
- workload label 模型
- 目标 runtime 接入规则

退出条件：

- 能稳定识别目标 AI runtime
- 标签可供后续策略直接消费

## Phase 2：Inference Tail Guard MVP

目标：

- 证明系统级干预可以改善 AI 推理尾延迟

范围：

- `sched/offcpu/page fault` 观测
- collector 聚合
- bounded boost
- 延迟与稳定性评估

退出条件：

- 有 / 无 boost 对照实验可复现
- live guarded 动作确实改变了主机层目标状态
- P95/P99、TTFT 或 jitter 在重复轮次中满足严格收益门槛

当前状态：

- 控制闭环、dry-run 审计和 Phase 4 报告路径已经具备。
- 最新 `docs/mvp_benefit_report.md` 正确给出 `FAIL`：已经记录 effective live
  `taskset` action，但稳定收益趋势没有达到门槛，因此不能声明 MVP 收益成立。

## Phase 3：Tool Calling Booster

目标：

- 降低工具调用链的 end-to-end latency

范围：

- tool call 生命周期识别
- executor / retrieval / rerank 子链路标签
- 生命周期内 boost 与自动退出

退出条件：

- 工具调用链存在稳定、可观察的优化收益

当前状态：

- 已有 policy path 和真实 executor lifecycle harness。
- 下一步需要 repeated A/B benefit proof，而不是只证明识别和触发。

## Phase 4：AI-aware Isolation

目标：

- 提升多任务并发场景下 AI 响应稳定性

范围：

- 区分 interactive AI task 与 background tasks
- 动态限速背景任务
- AI workload 资源隔离

退出条件：

- 并发场景下 tail latency 与 jitter 有明显改善

## Phase 5：Explain / Tune Layer

目标：

- 让系统具备可解释、可调优能力

范围：

- 自动生成优化报告
- 分析 trigger 原因
- 阈值建议
- 历史表现对比

退出条件：

- 可以基于实验数据自动生成有用结论

## Phase 6：Advanced Extensions

可能扩展：

- `rag_pipeline_booster`
- `multi_agent_concurrency_isolation`
- `gpu_host_coordination`
- `cold_start_optimizer`
- `adaptive_policy_learning`

## 推荐推进顺序

1. 完成 `AegisAI_Runtime-jtt`：用 controlled workload 验证 helper-backed
   off-CPU / I/O latency 真实信号。
2. 完成 `AegisAI_Runtime-v2y`：把 live CPU affinity planning 从 actuator 大文件中
   模块化，降低后续 live 调参风险。
3. 完成 `AegisAI_Runtime-lql`：继续调优 Inference Tail Guard live affinity
   benefit，保持严格 Phase 4 gate。
4. 完成 `AegisAI_Runtime-94s`：Tool Call Booster live guarded benefit proof。
5. 穿插完成热路径测试加固：actuator rollback、Linux source/procfs 边界和 benefit
   report 解释逻辑。
6. 最后再考虑 AI-aware isolation、explain/tune 自动化和高级扩展。
