# AegisAI Runtime 项目方案 v2

基于 eBPF + Rust 的 AI 推理与 Tool Calling 系统级性能优化框架。

## 1. 项目定义

### 一句话定义

把 AI workload 当作系统层的一等公民，通过低开销观测、任务识别、策略决策和受限干预，让 AI 推理与工具调用在真实系统环境里跑得更快、更稳、更低干扰。

### 项目定位

- 这是一个面向 AI runtime 的系统级优化引擎。
- 它的目标不是解释“为什么慢”而已，而是在关键路径上主动降低尾延迟和系统抖动。
- 它关注的不是平均值优化，而是 TTFT、P95/P99、jitter 这类更贴近真实体验的指标。

### 项目不是什么

- 不是通用监控平台。
- 不是 Linux scheduler 替代品。
- 不是把 AI 模型放进实时微秒级调度路径。
- 不是第一版就做 RAG、多智能体、GPU 协同和 dashboard 全家桶。

## 2. 项目要解决的三个核心问题

### 问题 A：推理尾延迟

AI 推理线程会受到 run queue delay、off-CPU、迁核、page fault 等系统级干扰，导致 TTFT 和 P95/P99 明显恶化。

### 问题 B：工具调用链路不稳定

Tool call 生命周期中常出现冷启动、子链路抖动、短时竞争和调度不稳定，导致 end-to-end latency 波动较大。

### 问题 C：系统对 AI 任务感知不足

系统往往无法分辨谁是 AI inference、谁是 tool executor、谁是 background job，最终只能一视同仁调度，无法做针对性保护。

## 3. 设计总路线

### 3.1 总体判断

这次路线不再沿用“通用平台 + 插件列表”的组织方式，而是改成“问题主线驱动 + 能力闭环复用”的结构。

核心判断如下：

- “系统感知 AI 任务进程”不是附属能力，而是全局基础能力。
- “尾延迟治理”是最适合做 MVP 的主战场。
- “工具调用链优化”应建立在任务识别和基础闭环已经成立之后。

### 3.2 三条主线

#### 主线 1：AI Workload Awareness

目标是让系统知道：

- 哪些进程或线程属于 AI workload。
- 它们处在 inference、tool calling、retrieval、rerank 等哪个阶段。
- 它们是否属于交互敏感路径。

第一轮只做规则识别，不做复杂智能分类：

- process name
- cmdline pattern
- cgroup/tag
- PID allowlist
- parent-child relation

#### 主线 2：Inference Tail Guard

目标是在 AI 推理关键路径上，用短时、可回退、可审计的 boost 动作对抗系统干扰，改善：

- TTFT
- P95/P99 latency
- jitter

这是 MVP 主场景，因为最容易形成闭环，也最容易做出可量化收益。

#### 主线 3：Tool Call Booster

目标是识别工具调用生命周期，并在 tool executor、retrieval、rerank、sandbox worker 等子链路上做轻量保护，降低 tool call end-to-end latency。

第一轮不追求复杂 tracing，只做：

- 生命周期识别
- 轻量 boost
- 自动退出

### 3.3 路线依赖关系

路线依赖关系是：

`AI Workload Awareness -> Inference Tail Guard -> Tool Call Booster -> AI-aware Isolation / Explain-Tune`

也就是说，任务识别先立住，尾延迟治理先闭环，再推进工具链优化，最后扩展到更复杂的多任务保护和解释调优。

## 4. 架构路线

### 4.1 双轴骨架

从问题视角看，本项目有三条主线。

从实现视角看，本项目采用“双轴骨架”：

#### 轴一：核心闭环能力轴

- `ebpf/`：低开销观测
- `agent/collector`：事件聚合与窗口统计
- `agent/classifier`：AI workload 识别与阶段标签
- `agent/policy_engine`：规则匹配、动作选择、冲突处理
- `agent/actuator`：系统动作施加、回退和审计
- `agent/metrics`：收益与副作用记录

#### 轴二：问题场景轴

- `scenarios/ai_workload_awareness`
- `scenarios/inference_tail_guard`
- `scenarios/tool_call_booster`

能力轴定义“靠哪些组件形成闭环”，场景轴定义“具体解决什么问题”。

### 4.2 核心数据流

系统的核心数据流为：

`kernel event -> event aggregation -> workload label -> scenario policy -> bounded action -> effect measurement`

其中：

- `ebpf/` 负责产生低开销事件。
- `collector` 将离散事件变成稳定的窗口特征。
- `classifier` 给进程、线程、cgroup 打上 AI runtime 语义标签。
- `policy_engine` 根据场景和安全边界生成动作。
- `actuator` 施加动作并负责自动回退。
- `metrics` 记录收益、副作用和触发行为。

## 5. 当前采用的工程结构

```text
.
├── agent/
│   ├── actuator/
│   ├── classifier/
│   ├── collector/
│   ├── metrics/
│   └── policy_engine/
├── bench/
│   ├── ai_workload_awareness/
│   ├── inference_tail_guard/
│   ├── interference/
│   ├── scripts/
│   └── tool_call_booster/
├── configs/
│   ├── classifier/
│   ├── runtime/
│   ├── safety/
│   └── scenarios/
├── docs/
├── ebpf/
│   ├── fault_probe/
│   ├── io_probe/
│   ├── offcpu_probe/
│   └── sched_probe/
├── scenarios/
│   ├── ai_workload_awareness/
│   ├── inference_tail_guard/
│   └── tool_call_booster/
├── project.md
├── specification.md
└── README.md
```

这个结构不再围绕旧的 `plugins/` 和 `llm_benchmark/` 组织，而是围绕三条主线和统一闭环组织。

## 6. 模块职责

### `ebpf/`

负责低开销采集关键系统事件，第一轮保留高价值 probe：

- `sched_probe`
- `offcpu_probe`
- `fault_probe`
- `io_probe`

### `agent/collector`

负责：

- 事件聚合
- 时间窗口统计
- 去噪与采样
- thread/process/cgroup 维度归并
- 对 policy 暴露统一 feature view

### `agent/classifier`

负责将 process/thread/cgroup 转换成 AI runtime 语义标签，例如：

- `AI_INFERENCE`
- `TOOL_CALL`
- `RETRIEVAL_STAGE`
- `RERANK_STAGE`
- `BACKGROUND_JOB`
- `INTERACTIVE_LATENCY_SENSITIVE`

### `agent/policy_engine`

负责根据“指标 + 标签 + 安全约束”做出动作决策，至少需要支持：

- 触发条件
- 冷却窗口
- 动作强度等级
- 多策略冲突处理
- 最大干预时长
- 安全限制

### `agent/actuator`

负责执行系统动作，并保证动作有限时、可回退、可审计。第一轮保守动作包括：

- CPU affinity
- nice / priority 调整
- cpuset 预留接口
- background throttle 预留接口
- service/cache warmup 预留接口

### `agent/metrics`

负责记录收益和副作用，重点指标包括：

- TTFT
- P95/P99 latency
- jitter / variance
- boost hit rate
- rollback count

### `scenarios/ai_workload_awareness`

负责沉淀识别规则、阶段标签和 runtime 适配方案。

### `scenarios/inference_tail_guard`

负责把推理尾延迟治理做成可独立推进、可独立验证的场景包。

### `scenarios/tool_call_booster`

负责把工具链优化做成独立场景包，避免与推理场景耦合。

## 7. MVP 范围

### MVP 目标

只证明一件事：

在 AI 推理场景下，系统级动态保护可以稳定改善尾延迟和抖动。

### MVP 包含

- 锁定一个目标 runtime，优先 `ollama` 或 `llama.cpp`
- 实现最小 `AI workload awareness` 规则链路
- 打通 `sched/offcpu/fault` 的观测到聚合链路
- 实现一个最小 `inference_tail_guard` 策略
- 在 Linux 环境完成无优化与有优化的 benchmark 对照

### MVP 暂不包含

- 自动多模型智能分类
- 完整 tool lifecycle tracing
- 完整 GPU 协同调度
- AI 自动调参闭环
- 大型 dashboard

## 8. 路线图

### Phase 0：Framework Reset

完成项目定义、骨架刷新、配置边界和文档统一。

### Phase 1：Awareness Foundation

把 `AI workload awareness` 做成全局基础能力，交付：

- 统一 label 模型
- classifier config
- runtime 识别规则

### Phase 2：Inference Tail Guard MVP

打通最小闭环，交付：

- `sched/offcpu/fault` probe
- collector 窗口聚合
- classifier 标签
- `inference_tail_guard` 决策
- bounded boost 动作
- benchmark 对照实验

### Phase 3：Tool Call Booster

在工具链路上建立场景包，交付：

- tool call 生命周期识别
- executor / retrieval / rerank 子链路标签
- 生命周期内 boost 与自动退出

### Phase 4：AI-aware Isolation

在多任务并发场景中保护交互型 AI workload。

### Phase 5：Explain / Tune

补充实验报告、触发解释和参数建议能力。

## 9. benchmark 与评估路线

### `bench/inference_tail_guard`

验证：

- 无优化 vs 开启 boost
- CPU 干扰场景
- I/O 干扰场景
- TTFT / P95 / P99 / jitter

### `bench/tool_call_booster`

验证：

- tool executor 启动延迟
- retrieval / rerank 子链路时延
- end-to-end tool call latency

### `bench/ai_workload_awareness`

验证：

- 规则识别准确性
- 标签覆盖率
- runtime 适配完整度

### `bench/interference`

统一管理干扰源：

- `stress-ng`
- `fio`
- background batch workers

## 10. 当前明确不做

- 把项目做成通用 observability 平台
- 让 AI 直接参与实时策略执行
- 追求不可回退的强系统控制
- 提前进入复杂 RAG、多智能体、GPU 协同大场景

## 11. 当前设计结论

这版方案的最终路线可以概括为一句话：

先把 AI workload awareness 做成基础能力，再围绕 inference tail guard 做出第一个可验证闭环，随后把同一套能力扩展到 tool call booster，而不是一开始做一个大而泛的 runtime 平台。

这就是后续真正动工时的总路线。
