# 项目表述

## 中文版简历描述

AegisAI Runtime：基于 eBPF 与 Rust 构建面向 AI 推理与 Tool Calling 的系统级性能优化框架。通过低开销观测调度延迟、off-CPU、页错误与 I/O 抖动等关键指标，识别 AI workload 关键路径并动态执行 CPU 亲和性、优先级提升和资源隔离等优化策略，显著降低推理尾延迟与响应抖动。

## 英文版简历描述

Built AegisAI Runtime, a system-level optimization framework for AI inference and tool-calling workloads using eBPF and Rust. It monitors low-overhead scheduling, off-CPU, page-fault, and I/O interference signals, classifies AI runtime phases, and dynamically applies affinity, priority boosting, and isolation strategies to reduce tail latency and response jitter.

## 面试时的一句话

我做的是一个面向 AI runtime 的系统级性能护航框架，它不只是解释“为什么慢”，而是在关键路径上主动降低尾延迟和系统干扰。

## 创新点表达

### 1. AI workload-aware

不是做通用系统优化，而是专门面向 AI 推理和工具调用场景。

### 2. 关注尾延迟而不是均值

核心指标不是平均吞吐，而是：

- TTFT
- P95/P99 latency
- jitter

### 3. eBPF 观测 + Rust 控制闭环

不是只做观测，也不是只做调优，而是：

观测 -> 分类 -> 决策 -> 干预 -> 反馈 的闭环优化系统。

### 4. 插件化 runtime framework

不是单点脚本，而是可扩展到：

- inference
- tool calling
- RAG
- multi-agent
- GPU-host runtime

## 项目定位提醒

后面不管写文档还是讲项目，都要始终强调一件事：

这是一个动态优化引擎，不是一个观测平台。

