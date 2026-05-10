# Roadmap

_Updated: 2026-05-10_

## Current Position

The project has moved beyond the original framework, runnable-daemon setup, and
19-task evidence-hardening pass. The active phase is product evidence:

- prove or reproducibly falsify effective live `inference_tail_guard` benefit
  under the strict Phase 4 gate
- prove or reproducibly falsify Tool Call Booster guarded latency benefit
- keep helper portability, production config, packaging, cpuset/background
  isolation, WarmupExecutor side effects, dashboard, GPU, and adaptive policy
  work behind explicit follow-up issues

See `docs/current_status.md` for the current state and open issue IDs.
See `docs/task_list.md` for the accepted 19-task ledger.

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

状态：完成。

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

状态：基础闭环完成；awareness 作为 classifier/orchestrator 基础能力服务场景策略。

## Phase 2：Inference Tail Guard MVP

目标：

- 证明系统级干预可以改善 AI 推理尾延迟

范围：

- `sched/offcpu/page fault/io` 观测
- collector 聚合
- bounded boost
- 延迟与稳定性评估

退出条件：

- 有 / 无 boost 对照实验可复现
- live guarded 动作确实改变了主机层目标状态
- P95/P99、TTFT 或 jitter 在重复轮次中满足严格收益门槛

当前状态：

- 控制闭环、dry-run 审计、live guarded 动作、CPU affinity planner 和 Phase 4 报告路径
  已具备。
- 最新 `docs/mvp_benefit_report.md` 正确给出 `FAIL`：已经记录 effective live
  `taskset` action，但稳定收益趋势没有达到门槛，因此不能声明 MVP 收益成立。
- 下一步由 `AegisAI_Runtime-2kz` 追踪。

## Phase 3：Tool Calling Booster

目标：

- 降低工具调用链的 end-to-end latency

范围：

- tool call 生命周期识别
- executor / retrieval / rerank 子链路标签
- 生命周期内 boost 与自动退出
- guarded scheduler benefit report

退出条件：

- 工具调用链存在稳定、可观察的优化收益，或者报告给出可复现的失败原因

当前状态：

- policy path、真实 executor lifecycle harness、audit continuity、summary/report 和
  live_guarded proof run 已具备。
- 最新 artifact contract `PASS`，benefit `FAIL`：`live_guarded` 没有达到重复 latency
  improvement 门槛。
- `WarmupExecutor` 仍是 plan/audit-only，不代表真实 executor/cache warmup。
- 下一步由 `AegisAI_Runtime-79d` 和 `AegisAI_Runtime-14r` 追踪。

## Phase 4：AI-aware Isolation

目标：

- 提升多任务并发场景下 AI 响应稳定性

范围：

- 区分 interactive AI task 与 background tasks
- 动态限速背景任务
- AI workload 资源隔离

退出条件：

- 并发场景下 tail latency 与 jitter 有明显改善

当前状态：

- policy/audit surface 存在，但 live cpuset/background throttling 尚未启用。
- 下一步先由 `AegisAI_Runtime-otk` 定义安全边界。

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

当前状态：

- 离线 explain/tune 基础能力存在。
- 在线 adaptive policy learning 暂缓，由 `AegisAI_Runtime-0ry` 后续拆分。

## Phase 6：Productionization

目标：

- 让 runtime daemon 和 helper 能被可靠部署、配置和运维

范围：

- production config profiles
- schema validation
- daemon/helper packaging
- service management
- helper privilege installation and checks

当前状态：

- 配置 profile 边界已记录在 `docs/engineering_debt_boundaries.md`。
- 实现由 `AegisAI_Runtime-cqv` 和 `AegisAI_Runtime-ufp` 追踪。

## Phase 7：Advanced Extensions

可能扩展：

- `rag_pipeline_booster`
- `multi_agent_concurrency_isolation`
- `gpu_host_coordination`
- `cold_start_optimizer`
- `adaptive_policy_learning`
- dashboard

当前状态：延期，由 `AegisAI_Runtime-0ry` 后续规划。

## 推荐推进顺序

1. `AegisAI_Runtime-2kz`：继续 Inference Tail Guard live guarded benefit proof，
   保持 strict gate。
2. `AegisAI_Runtime-79d`：继续 Tool Call Booster guarded latency benefit proof。
3. `AegisAI_Runtime-51c`：扩展 helper 跨 kernel 可移植性验证。
4. `AegisAI_Runtime-cqv`：补 production config profiles 和 schema validation。
5. `AegisAI_Runtime-14r` / `AegisAI_Runtime-otk`：分别决定 WarmupExecutor side
   effect 和 live cpuset/background isolation 边界。
6. `AegisAI_Runtime-ufp`：生产 service packaging 和 installer。
7. `AegisAI_Runtime-0ry`：dashboard、GPU、adaptive policy 等高级扩展规划。
