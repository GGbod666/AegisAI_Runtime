# 下一阶段执行计划

## 1. 当前结论

当前仓库已经完成统一主干收敛，且通过了以下工程级验收：

- `cargo check`
- `cargo test`
- `cargo clippy --all-targets --all-features -- -D warnings`
- `cargo fmt --all -- --check`

这意味着基础模块边界、共享契约和单元测试护栏已经站稳。下一阶段不应该继续堆骨架，而应该把设计路线推进到“真实可运行闭环”。

## 2. 下一阶段的目标

下一阶段的唯一主目标是：

把当前的统一模块主干推进成一个可以在 Linux 环境中跑起来的最小闭环 Demo，先完成：

`AI Workload Awareness -> Inference Tail Guard`

而不是同时展开所有场景。

## 3. 本阶段范围

### 必做范围

1. 增加运行入口
   - 新增一个真正的 runtime daemon / runner crate
   - 负责加载配置、初始化 orchestrator、接收事件流、驱动 tick/rollback

2. 定义运行时接入边界
   - 增加统一的 event source / runtime source 抽象
   - 让 orchestrator 可以同时接 mock source 和真实 Linux source

3. 打通最小事件链路
   - `ebpf_probe -> collector -> classifier -> policy_engine -> actuator -> metrics`
   - 先只接 MVP 需要的信号：`run_queue_delay`、`off_cpu`、`major_page_fault`

4. 增加 runtime metadata enrichment
   - 为事件补全 `pid/tid/process_name/cmdline/cgroup/parent`
   - 为 classifier 提供稳定输入，而不是只靠事件瞬时字段

5. 增加 actuator backend 分层
   - `noop/mock backend` 用于本地测试
   - `linux backend` 用于真实 `nice/affinity` 控制

6. 建立最小 Demo 与实验入口
   - 固定一个目标 runtime，优先 `ollama` 或 `llama.cpp`
   - 固定一套 CPU 干扰场景
   - 跑出 baseline 与 boost 对照数据

### 明确不做

- 不在这一阶段做完整 `tool_call_booster` 实机闭环
- 不引入 GPU 协同
- 不做复杂 tracing 平台
- 不做在线自动调参
- 不做 dashboard

## 4. 模块级任务拆分

### A. `runtime_entry`

职责：

- 解析配置
- 初始化 logger / runtime context
- 创建 orchestrator
- 接收 source 事件并投递
- 周期性调用 rollback/tick

建议产物：

- `agent/runtime_daemon/`
- `src/main.rs`
- `src/source.rs`
- `src/runtime_loop.rs`

### B. `source_adapter`

职责：

- 定义 `EventSource` trait
- 提供 `MockEventSource`
- 提供 `LinuxProbeSource` 骨架

建议接口：

- `next_event()`
- `poll_batch()`
- `snapshot_process(pid)`

### C. `metadata_enricher`

职责：

- 从 `/proc` 或运行时快照补全 classifier 需要的信息
- 缓存短周期 metadata，避免每个事件都全量读取

最小字段：

- `process_name`
- `cmdline`
- `parent_pid`
- `parent_process_name`
- `cgroup`
- `tags`

### D. `linux_actuator_backend`

职责：

- 将 `ActionPlan` 映射到真实 Linux 动作
- 优先支持：
  - `nice`
  - `sched_setaffinity`
- 保持 bounded、可回退、可审计

### E. `demo_benchmark`

职责：

- 固定最小 demo 路径
- 生成 baseline / boosted 对照
- 沉淀实验结果到 metrics

## 5. 推荐推进顺序

1. 先做 `runtime_daemon` 和 `MockEventSource`
2. 用 mock source 跑通主干闭环
3. 接入 metadata enrichment
4. 接入 Linux actuator backend
5. 接入最小 `ebpf_probe` source
6. 固定 `ollama` 或 `llama.cpp` 的 demo
7. 跑首轮 `inference_tail_guard` benchmark

## 6. 阶段退出条件

满足以下条件后，下一阶段算完成：

- 可以通过单一命令启动 runtime 进程
- runtime 能持续消费事件流而不是只跑单元测试
- classifier 能识别目标 AI runtime
- `inference_tail_guard` 能在 Linux demo 中真实触发
- actuator 能执行并回退最小动作
- metrics 能输出 baseline 与 boost 对照结果

## 7. 下一阶段之后再做什么

本阶段完成后，再进入：

1. `tool_call_booster` 的真实运行链路
2. benchmark 自动化脚本
3. explain/tune 报告固化
4. AI-aware isolation 扩展

## 8. 主脑判断

当前最关键的不是继续加模块，而是把“统一主干”变成“真实可运行系统”。

所以，下一阶段应该严格围绕：

`runtime entry + source adapter + metadata enrichment + linux actuator backend + demo benchmark`

来推进。
