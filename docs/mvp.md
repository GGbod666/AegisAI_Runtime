# MVP 说明

_Updated: 2026-05-10_

## 当前审查结论

当前仓库已经具备最小可运行 AI-aware 控制闭环，并通过 2026-05-10 系统验收。
但收益型 MVP 尚未被证明：最新 `docs/mvp_benefit_report.md` 记录了 effective
live host-level `taskset` action，稳定收益趋势仍没有达到门槛，因此报告正确给出
`FAIL`。

后续 MVP 判断必须继续保持这条硬规则：

- `noop_observation` 和 `dry_run` 只能证明识别、触发、审计和 rollback 闭环。
- 只有 effective live host-level actuator change 加重复 A/B 收益趋势，才能证明
  MVP 性能收益。

当前状态见 `docs/current_status.md`；19 项验收台账和后续缺口索引见
`docs/task_list.md`。

## 1. MVP 重新定义

当前 MVP 不再只是“做一个尾延迟插件”，而是“建立最小可运行的 AI-aware 优化闭环”。

这个闭环包含两部分：

- 基础能力 MVP：AI workload awareness
- 收益验证 MVP：Inference Tail Guard

## 2. MVP 要证明什么

MVP 要证明两件事：

1. 系统能够稳定识别目标 AI workload，并为其打上可用标签
2. 在 AI 推理场景下，系统级动态保护可以降低尾延迟和抖动

第一件事当前已由 classifier、runtime orchestrator、mock/Linux preflight、Tool Call
lifecycle 和相关测试支撑。第二件事仍未证明。

## 3. MVP 范围

### 必做项

- 识别目标 AI 推理进程
- 输出基础 workload label
- 采集 run queue delay
- 采集 CPU migration
- 采集 major page fault
- 通过窄权限 helper 采集 off-CPU time
- 通过窄权限 helper 采集 I/O latency
- 根据阈值触发短时 boost
- 记录优化前后 TTFT、P95/P99 latency、jitter
- 严格区分 baseline、noop、dry-run 和 live guarded 证据

### 可选项

- cpuset 隔离
- 轻度 background throttle
- Tool Call Booster guarded scheduler benefit

### 明确不做

- 自动多模型分类
- GPU 协同调度
- AI 自动调参闭环
- 可视化 dashboard
- 生产 service/installer

## 4. MVP 目标架构切片

本阶段只打通最小闭环：

1. procfs fallback 输出 `run_queue_delay`、`cpu_migration`、`major_page_fault`
   可解释指标；`offcpu_time` / `io_latency` 由窄权限
   `aegisai-ebpf-helper` 补齐，主 daemon 不以 root 运行
2. collector 聚合窗口内指标
3. classifier 用规则识别 AI inference 目标
4. `inference_tail_guard` 根据阈值决定是否 boost
5. actuator 执行轻量动作
6. CPU affinity planning 通过独立 planner 处理 online/allowed CPU 交集和 rollback
   target
7. metrics 记录优化效果

## 5. 推荐 demo 场景

### 场景 A：AI 推理 + CPU 干扰

- `ollama` 或 `llama.cpp`
- 配合 `stress-ng` 制造 CPU 压力

验证目标：

- 无优化 vs 开启 guarded boost 模式
- 观察 P99、TTFT 或 jitter 是否稳定改善

### 场景 B：AI 推理 + I/O 扰动

- 推理期间运行磁盘读写干扰
- 观察 page fault / I/O 抖动与延迟波动关系

验证目标：

- 观察 tail latency 和 jitter 是否下降

## 6. 成功标准

收益型 MVP 至少要在 repeated A/B 中看到以下趋势之一稳定成立：

- P95/P99 latency 明显下降
- TTFT 更稳定
- jitter 或 latency variance 明显下降
- 轻中度干扰下交互响应更流畅

平均吞吐不是第一指标。只要尾延迟和稳定性显著提升，MVP 就成立。

同时必须满足：

- `live_guarded` 发生至少一次 effective host-level actuator change
- apply/rollback 没有 action audit error
- benefit report 明确区分 baseline、noop、dry-run 和 live guarded 证据
- repeated benefit rule 通过：至少三轮可比较样本、至少三分之二轮次改善、平均改善
  达到报告配置的阈值

## 7. DoD

基础闭环 DoD 已满足：

- AI workload awareness 有独立目录与配置入口
- inference tail guard 有独立策略与 benchmark 入口
- 模块边界清晰
- benchmark 方案明确
- 配置入口明确
- helper-backed `offcpu_time` / `io_latency` 已完成受控验证
- live CPU affinity planning 已模块化并有目标选择测试
- Tool Call Booster 有 real executor lifecycle harness、audit continuity 和 guarded
  benefit report
- hot-path 测试覆盖 actuator rollback、Linux source/procfs 边界和 benefit report 解释逻辑

剩余收益 DoD：

- `AegisAI_Runtime-2kz` 关闭：有效 live Inference Tail Guard actuator benefit 被证明，
  或报告明确给出可复现的失败原因。
- `AegisAI_Runtime-79d` 关闭：Tool Call Booster guarded scheduler benefit 被证明，
  或报告明确给出可复现的失败原因。

明确后续但不阻塞基础闭环：

- `AegisAI_Runtime-cqv`：生产配置 profile 和 schema validation。
- `AegisAI_Runtime-51c`：helper 跨 kernel 可移植性验证。
- `AegisAI_Runtime-14r`：决定是否需要真实 `WarmupExecutor` side effect。
- `AegisAI_Runtime-otk`：live cpuset/background isolation 安全边界。
- `AegisAI_Runtime-ufp`：生产 packaging/installer。
- `AegisAI_Runtime-0ry`：dashboard、GPU、adaptive policy 等延期扩展。

## 8. 建议先锁的技术选择

建议优先选一个稳定、现成、可重复的推理 runtime：

- `ollama`
- `llama.cpp`

建议优先做的系统动作：

- affinity
- nice 调整

暂缓：

- live cpuset cgroup writes
- background throttling
- real executor/cache warmup side effect
