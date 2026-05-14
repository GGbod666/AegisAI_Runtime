# AegisAI Runtime

AegisAI Runtime 是一个用 Rust 编写的 AI-aware Linux 控制闭环。它把 AI 推理进程和
Tool Calling 子链路当成系统层的一等 workload：观测调度和 I/O 干扰信号，识别当前
进程属于哪类 AI 工作负载，按场景策略生成有边界的系统动作，执行或演练动作，在租约
到期后回滚，并用指标与 trace 记录这次干预是否真的有收益。

当前仓库已经不只是脚手架。它包含可运行的 mock 主路径、Linux procfs/eBPF-helper
观测路径、受保护的 actuator 后端、两个已接入策略的场景、指标与解释模块，以及
benchmark/report 脚本。最新 `docs/mvp_benefit_report.md` 已经记录
Inference Tail Guard 的 live guarded `PASS`：在只把每档样本数从 `4` 提到 `8`
的受控复跑中，live guarded 产生有效主机状态变化，并在 jitter 上达到稳定收益门槛。
Tool Call Booster 也已有 fixed-work guarded scheduler benefit `PASS`；项目仍未宣称
生产级完备性，非受控工作量边界、配置、打包和跨 kernel 验证仍是开放工作。

## 问题导向

项目围绕三类具体问题组织，而不是先做一个泛化平台再寻找使用场景。

### AI Workload Awareness

Linux 内核看到的是进程、线程、cgroup 和调度事件；它不知道某个 `python` 是
retrieval worker，某个 `ollama` 是交互式推理链路，某个 `stress-ng` 是背景干扰源。
AegisAI 通过规则化的 process name、cmdline、cgroup、tag marker、父子进程关系和 PID
allowlist，把这些系统实体映射成 `AI_INFERENCE`、`TOOL_CALL`、
`INTERACTIVE_LATENCY_SENSITIVE`、`BACKGROUND_JOB` 等标签。

这不是附属功能，而是后续 Tail Guard 和 Tool Call Booster 能避免硬编码特判的基础层。

### Inference Tail Guard

推理体验常常被 TTFT、P95/P99 和 jitter 支配。仓库当前把
`run_queue_delay`、`offcpu_time`、`cpu_migration`、`major_page_fault` 和可选
`io_latency` 作为尾延迟风险信号；当交互式推理进程在窗口内触发阈值时，策略会生成
短时、可回退的优先级或 CPU 亲和性动作。

这是 MVP 的主战场，因为它最容易被 A/B 实验验证，也最容易被错误地夸大。因此报告门
槛很严格：noop 和 dry-run 只能证明识别、触发、审计和回滚闭环，不能证明主机级收益。

### Tool Call Booster

工具调用链路中有 executor startup、retrieval、rerank 和 background interference 等
阶段。仓库当前用标签和 `tool_call_id` 审计字段识别生命周期，并按 executor、
retrieval、rerank 阶段缩放动作时长和审计字段。daemon apply detail 会内联记录
`tool_call_stage`、`tool_call_id`、`action_kind` 和动作是否有效，report 因此能把
有效 scheduler action 归因到具体阶段。当前已有重复 A/B harness/report；最新受控
fixed-work guarded 运行 contract `PASS`、benefit `PASS`，executor/retrieval/rerank
stage effectiveness 全部 `PASS`。历史 stable executor-control 运行仍保留为非受控工作量
边界：该 run contract `PASS`、benefit `FAIL`。

## 设计理念

- 问题先行：核心模块服务三个明确问题，而不是追求一个通用监控平台。
- rootless 控制面：`aegisai-runtime-daemon` 默认作为普通用户进程运行。
- 窄权限边界：需要 eBPF/root 能力的部分收敛到 `aegisai-ebpf-helper`，daemon 只传
  selector 和固定信号开关，不发送任意 bpftrace 程序。
- 有边界的动作：所有 live 动作都必须有安全夹取、审计字段、租约和回滚路径。
- 策略可解释：每次触发都携带 scenario、breach、matched rule、stage、isolation mode
  等可审计字段。
- 证据诚实：有效主机动作和稳定 A/B 改善缺一不可；观察模式、dry-run 和偶发改善都不
  被当作 MVP 收益证明。
- 可在非 Linux 开发：mock source、noop backend 和 dry-run backend 让控制面可以在普
  通开发环境验证；真正的 probe 和 live actuator 仍以 Linux 为准。

明确非目标：

- 不是通用 dashboard 或监控平台。
- 不是 Linux scheduler 替代品。
- 不是把在线 AI 决策放进微秒级热路径。
- 当前没有 GPU 调度、生产服务安装器、在线自适应策略学习或完整后台隔离系统。

## 当前状态

已实现并在本地验证过的能力：

- Rust workspace 与共享 runtime contracts。
- `aegisai-runtime-daemon` CLI，支持 `mock` 和 `linux` source。
- Linux source 组合 procfs 派生的 `run_queue_delay`、`cpu_migration`、
  `major_page_fault`，以及 helper-backed 的 `offcpu_time`、`io_latency`。
- `aegisai-ebpf-helper` 作为窄权限 bpftrace/eBPF helper。
- 规则化 classifier 和 awareness defaults。
- collector 窗口聚合，并把 source event 投影为 policy feature window。
- policy engine 支持 cooldown、安全夹取、scenario 优先级和 action-slot 冲突消解。
- actuator backend：`noop`、`linux-skeleton`、`linux-command-dry-run`、
  guarded `linux-command`。
- metrics 和 explain/tune crate，用于 trace、scenario stats、实验报告、触发解释和调参
  建议。
- Inference Tail Guard 与 Tool Call Booster 的 benchmark/report 脚本。

仍未完成的缺口见本文末尾的“已知问题和差距”，并以 `bd` issue 为准。

## 目录结构

```text
.
├── agent/
│   ├── runtime_contracts/      # 共享 Event/Signal/FeatureWindow/Profile/Policy/Action 类型
│   ├── runtime_daemon/         # 可运行 CLI、EventSource、MetadataProvider、RuntimeLoop
│   ├── runtime_orchestrator/   # collector -> classifier -> policy -> actuator -> metrics 组合点
│   ├── collector/              # 低层事件窗口聚合、recent event retention、feature projection
│   ├── classifier/             # 进程规则、标签、workload/stage/latency sensitivity 推断
│   ├── policy_engine/          # 场景策略评估、cooldown、action 冲突消解
│   ├── actuator/               # 动作租约、apply/rollback 后端、Linux command guard
│   ├── metrics/                # MetricRecord、MetricTrace、ScenarioStats、趋势计算
│   ├── explain_tune/           # 离线实验报告、触发解释、调参建议
│   ├── ebpf_helper/            # 窄权限 helper binary，负责固定 eBPF/bpftrace attachment
│   └── git_control/            # 独立 git 状态/checkpoint helper，不在 runtime 热路径中
├── ebpf/
│   ├── ebpf_probe/             # probe contract、descriptor、event、filter、registry
│   ├── sched_probe/            # 调度 probe 设计说明 stub
│   ├── offcpu_probe/           # off-CPU probe 设计说明 stub
│   ├── fault_probe/            # page fault probe 设计说明 stub
│   └── io_probe/               # I/O probe 设计说明 stub
├── scenarios/
│   ├── ai_workload_awareness/  # awareness 问题说明和配置入口
│   ├── inference_tail_guard/   # 推理尾延迟策略实现
│   └── tool_call_booster/      # tool-call 生命周期策略实现
├── configs/
│   ├── runtime/                # runtime 目标、选择方式、focus signals、tracked metrics
│   ├── classifier/             # process/cmdline/cgroup/tag/parent 规则
│   ├── scenarios/              # scenario 阈值和动作配置
│   └── safety/                 # 全局安全上限
├── bench/
│   ├── scripts/                # workspace 验证和实验编排脚本
│   ├── inference_tail_guard/   # Tail Guard benchmark 文档和 artifact 约定
│   ├── tool_call_booster/      # real executor harness、A/B summarizer、单测
│   ├── ai_workload_awareness/  # awareness 验证说明
│   └── interference/           # CPU/I/O/background 干扰定义
├── docs/                       # 架构、状态、MVP、roadmap、实验和 verification log
├── plugins/                    # 插件占位说明
├── project.md                  # 早期项目方向说明
└── specification.md            # 规格草案
```

注意：`ebpf/sched_probe`、`ebpf/offcpu_probe`、`ebpf/fault_probe`、`ebpf/io_probe` 当前
主要是设计说明 stub；可编译的 probe contract 在 `ebpf/ebpf_probe`，实际 off-CPU/I/O
事件流当前通过 `aegisai-ebpf-helper` 的 bpftrace 路径进入 daemon。

## Crate 依赖关系

以 `Cargo.toml` 为准，当前 workspace 成员是：

```text
agent/actuator
agent/ebpf_helper
agent/git_control
agent/runtime_orchestrator
agent/runtime_daemon
agent/runtime_contracts
agent/collector
agent/classifier
agent/explain_tune
agent/metrics
agent/policy_engine
ebpf/ebpf_probe
```

实际依赖边如下，箭头表示“左侧 crate 依赖右侧 crate”：

```text
agent/collector
  -> ebpf_probe

agent/runtime_contracts
  -> agent/classifier

agent/policy_engine
  -> agent/runtime_contracts

agent/actuator
  -> agent/runtime_contracts

agent/runtime_orchestrator
  -> agent/actuator
  -> agent/classifier
  -> agent/collector
  -> agent/metrics
  -> agent/policy_engine
  -> agent/runtime_contracts

agent/explain_tune
  -> agent/metrics
  -> agent/policy_engine

agent/runtime_daemon
  -> agent/runtime_orchestrator
  -> agent/actuator
  -> ebpf_probe

agent/ebpf_helper
  -> agent/runtime_daemon

agent/classifier, agent/metrics, agent/git_control, ebpf_probe
  -> no workspace-internal dependencies
```

更准确地说：

- `classifier`、`metrics`、`git_control` 和 `ebpf_probe` 目前没有 workspace 内部依赖。
- `runtime_contracts` 依赖 `classifier`，并复用/导出 workload tag、stage、latency
  sensitivity 等概念。
- `policy_engine` 依赖 `runtime_contracts`，并通过 `agent/policy_engine/src/scenarios.rs`
  用 `#[path]` 引入 `scenarios/inference_tail_guard/src/policy.rs` 和
  `scenarios/tool_call_booster/src/policy.rs`。
- `runtime_orchestrator` 是用户态组合点，依赖 actuator、classifier、collector、metrics、
  policy_engine 和 runtime_contracts。
- `runtime_daemon` 负责 CLI、source、metadata 和运行循环，因此依赖 orchestrator、
  actuator 和 ebpf_probe。
- `explain_tune` 是离线分析层，依赖 metrics 和 policy_engine。
- `ebpf_helper` 复用 runtime_daemon 中的 bpftrace pipe/selector/source error 基础设施。
- `git_control` 是独立辅助 crate，不参与 AegisAI runtime 控制闭环。

## 功能分布和关键文件

| 模块 | 关键文件 | 职责 |
| --- | --- | --- |
| 共享契约 | `agent/runtime_contracts/src/model.rs`, `config.rs` | 定义 `ScenarioKind`、`SignalKind`、`Event`、`FeatureWindow`、`WorkloadProfile`、`PolicyContext`、`ActionPlan`、`AppliedAction` 和安全/策略配置类型 |
| Daemon 入口 | `agent/runtime_daemon/src/main.rs` | 解析 CLI，加载 repo-root config，选择 source/metadata/backend，创建 `RuntimeLoop` 和 `RuntimeOrchestrator`，打印或追加 summary |
| Source 层 | `agent/runtime_daemon/src/source.rs` | 定义 `SourceEvent`、`EventSource`、Linux probe plan、procfs sampler、bpftrace/helper pipe、probe reader、mock source |
| Metadata 层 | `agent/runtime_daemon/src/metadata.rs` | 定义 `MetadataProvider`，支持 demo/static/noop/procfs 元数据补全 |
| Runtime loop | `agent/runtime_daemon/src/runtime_loop.rs` | 批量 poll source，metadata enrichment，定时 tick rollback，统计 signal/window/audit/tool-call lifecycle summary |
| Orchestrator | `agent/runtime_orchestrator/src/runtime_orchestrator.rs` | 串联 collector、awareness classifier、policy engine、actuator、metrics recorder |
| Config loader | `agent/runtime_orchestrator/src/config.rs` | 从固定 `configs/` 示例路径读取 runtime/classifier/awareness/safety/scenario 配置 |
| Collector | `agent/collector/src/collector.rs` | 维护窗口、watermark、late/noise filter、recent events，并输出 trailing feature window |
| Classifier | `agent/classifier/src/model.rs`, `classifier.rs`, `config.rs` | 规则解析与匹配，生成 workload class、stage label、latency sensitivity 和 matched rules |
| Policy engine | `agent/policy_engine/src/engine.rs` | 对 enabled policies 评估 candidate，执行 cooldown，按 scenario priority 消解同一 pid/action slot 冲突 |
| Tail policy | `scenarios/inference_tail_guard/src/policy.rs` | 只匹配 `AI_INFERENCE` + `INTERACTIVE_LATENCY_SENSITIVE`，按尾延迟信号触发动作 |
| Tool policy | `scenarios/tool_call_booster/src/policy.rs` | 匹配 `TOOL_CALL`，推断 executor/retrieval/rerank，缩放 duration 并写入 tool-call 审计字段 |
| Actuator | `agent/actuator/src/actuator.rs`, `backend.rs` | action lease 管理，noop/recording/linux 后端，Linux command dry-run/live apply/rollback |
| Metrics | `agent/metrics/src/recorder.rs`, `model.rs` | 生成 metric records、evaluation/action/rollback traces、scenario stats 和 trend |
| Explain/tune | `agent/explain_tune/src/engine.rs` | 从 metrics/traces/policies 生成 report、trigger explanation、tune suggestion |
| eBPF contract | `ebpf/ebpf_probe/src/*.rs` | probe kind、attach point、event validation、filter、registry 和 default descriptors |
| Helper | `agent/ebpf_helper/src/main.rs` | `--check` 和 `stream --offcpu --io --pid/--process-name` CLI，启动固定 bpftrace 路径 |

## 核心数据模型

控制闭环热路径使用一组小而稳定的结构：

- `SourceEvent`：source 层事件，包含 timestamp、pid/tid、signal、value，以及可选进程元
  数据。
- `Event`：metadata enrichment 后的 runtime event，进程名和命令行等字段已经补齐到策略
  可用程度。
- `FeatureWindow`：collector 产生的每进程窗口特征，包括最大 run queue delay、最大
  off-CPU time、迁核率、major fault 率、queue wait 等。
- `WorkloadProfile`：classifier 输出，包含 workload class、stage label、latency
  sensitivity、tags 和 matched rules。
- `PolicyContext`：`EventContext + FeatureWindow + WorkloadProfile + audit_fields`，是策略评估
  的输入。
- `ActionPlan`：策略输出，包含 scenario、target pid、actions、duration、rationale 和审计
  字段。
- `AppliedAction`：actuator 输出的 applied/rolled-back 记录，包含租约时间、状态和 backend
  审计字段。
- `MetricRecord`/`MetricTrace`：metrics 层记录的结构化评估、触发、动作、回滚和趋势上下文。
- `RuntimeRunSummary`：daemon CLI 输出摘要，包含事件数、动作数、回滚数、signal
  observations、feature maxima、audit highlights 和 tool-call lifecycle summary。

当前共享 contract 支持的信号：

```text
run_queue_delay
offcpu_time
cpu_migration
major_page_fault
subprocess_start_delay
queue_wait
io_latency
```

当前共享 contract 支持的动作：

```text
RaiseNice       # nice/priority delta
SetAffinity     # CPU affinity strategy + max CPU ratio
UseCpuset       # cpuset intent, live path mostly guarded/placeholder
WarmupExecutor  # tool-call executor/cache warmup intent, default deferred unless an explicit command backend warmup command is provided
```

## 配置接口

默认 local demo 路径仍通过 `RuntimeOrchestratorConfig::load_from_repo_root` 从 repo root
读取示例配置：

```text
configs/runtime/runtime.example.toml
configs/classifier/process_rules.example.toml
configs/scenarios/ai_workload_awareness.example.toml
configs/scenarios/inference_tail_guard.example.toml
configs/scenarios/tool_call_booster.example.toml
configs/safety/default.toml
```

生产 profile 路径已经独立出来：

- daemon CLI 支持 `--config-profile <name>`，优先级高于 `AEGISAI_CONFIG_PROFILE`。
- 未指定 profile 时使用 local demo 默认值，继续读取上面的 `*.example.toml` 文件。
- named profile 只能是 ASCII identifier，不能包含路径分隔符或 `.` segment。
- named profile 从 `configs/profiles/<name>/` 读取非 example 文件：
  `runtime.toml`、`classifier/process_rules.toml`、
  `scenarios/ai_workload_awareness.toml`、`safety/default.toml`、
  `scenarios/inference_tail_guard.toml` 和
  `scenarios/tool_call_booster.toml`。
- named production profile 会启用严格 schema 与 cross-file safety validation；
  local demo 示例路径保持兼容现有 benchmark 和测试配置。

配置到运行时类型的映射：

- `configs/runtime/runtime.example.toml` -> `RuntimeConfig`
  - 目标平台：`deployment_target = "linux"`、`kernel_min = "5.15"`、`cgroup_version = "v2"`。
  - runtime 选择：`process_names = ["ollama", "llama-server"]` 或 `pid_allowlist`。
  - collection：`focus_signals` 决定 Linux probe plan 和 collector 是否接收某信号。
  - metrics：`track` 决定 metrics recorder 跟踪哪些指标。
- `configs/classifier/process_rules.example.toml` -> `Vec<ProcessRule>`
  - 当前示例包含 `ollama`、`llama-server`、`python inference_worker`、
    `tool-executor`、`retrieval-worker`、`rerank-worker`、`stress-ng`。
- `configs/scenarios/ai_workload_awareness.example.toml` -> `AwarenessConfig`
  - 控制 cmdline/cgroup/parent/PID allowlist 规则开关。
  - 定义 interactive/tool/background 默认标签集合。
- `configs/scenarios/inference_tail_guard.example.toml` -> `ScenarioPolicy`
  - `evaluation_window_ms = 500`、`cooldown_ms = 1500`、`max_boost_duration_ms = 800`。
  - 触发项包括 run queue delay、off-CPU spike、CPU migration rate、major fault rate。
  - 动作包括 `raise_nice = -5`、`pin_strategy = "prefer_reserved_cores"`、`use_cpuset = false`。
- `configs/scenarios/tool_call_booster.example.toml` -> `ScenarioPolicy`
  - `evaluation_window_ms = 300`、`cooldown_ms = 800`、`max_boost_duration_ms = 1200`。
  - 触发项包括 subprocess startup delay、queue wait、optional I/O latency。
  - 动作包括 `raise_nice = -3`、`pin_strategy = "prefer_low_contention_cores"`、
    `warmup_executor = true`。
- `configs/safety/default.toml` -> `SafetyConfig`
  - `require_revert = true`
  - `allow_background_throttle = false`
  - `max_priority_delta = 5`
  - `max_boost_duration_ms = 800`
  - `max_affinity_change_ratio = 0.5`

局限：配置解析器是仓库内手写的最小 TOML 子集解析器，服务当前配置结构；它不是完整
TOML 解析库，也没有动态 reload、profile overlay 或远程配置分发。

## 对外接口

### Daemon CLI

常用 mock 路径：

```bash
cargo run -p aegisai-runtime-daemon -- \
  --repo-root . \
  --source mock \
  --metadata demo \
  --actuator-backend noop
```

Tool-call lifecycle mock：

```bash
cargo run -p aegisai-runtime-daemon -- \
  --repo-root . \
  --source mock \
  --mock-profile tool-call-lifecycle \
  --metadata noop \
  --actuator-backend noop
```

Linux source，允许部分 probe 失败：

```bash
cargo run -p aegisai-runtime-daemon -- \
  --repo-root . \
  --source linux \
  --metadata procfs \
  --actuator-backend linux-skeleton \
  --allow-partial-probes
```

Linux command dry-run：

```bash
cargo run -p aegisai-runtime-daemon -- \
  --repo-root . \
  --source linux \
  --metadata procfs \
  --actuator-backend linux-command-dry-run \
  --allow-partial-probes
```

Live command path 被强制加门：

```bash
cargo run -p aegisai-runtime-daemon -- \
  --repo-root . \
  --source linux \
  --metadata procfs \
  --actuator-backend linux-command \
  --confirm-live-actuator \
  --live-pid-allowlist <pid,...>
```

默认 live command 只允许 nice 相关动作；只有显式加 `--enable-live-affinity` 才会允许
`taskset` 亲和性动作。`--live-pid-allowlist` 也会把 runtime selection 改为 PID allowlist
模式，并清空 process-name selection，防止 live actuator 扩散到非目标进程。

### Helper CLI

helper readiness：

```bash
cargo run -p aegisai-ebpf-helper -- --check
```

helper stream：

```bash
cargo run -p aegisai-ebpf-helper -- \
  stream --offcpu --io --process-name ollama
```

helper 支持的外部选择器是 `--pid` 和 `--process-name`，支持的固定信号开关是 `--offcpu`
和 `--io`。如果 helper 不在 `PATH`，daemon 侧可通过 `AEGISAI_EBPF_HELPER=/path/to/helper`
指定；helper 内部 bpftrace 命令可通过 `AEGISAI_BPFTRACE=/path/to/bpftrace` 指定。

### Rust trait/API 边界

主要内部接口：

- `EventSource`
  - `source_name()`
  - `next_event()`
  - `poll_batch(max_batch)`
- `MetadataProvider`
  - `provider_name()`
  - `snapshot_process(pid)`
- `ProbeEventReader`
  - `start(plan, config)`
  - `next_probe_event()`
  - `poll_probe_events(max_events)`
  - `stop()`
- `LinuxProbeDriver`
  - `attach_probe(probe, config)`
  - `poll_events(max_events, timeout_ms)`
  - `stop()`
- `ActuatorBackend`
  - `apply(plan, now_ms)`
  - `rollback(applied, lease, now_ms)`
- `LinuxSyscallExecutor` / `LinuxSyscallApplier` / `LinuxProcessStateProvider`
  - 分离 Linux syscall plan、状态捕获、命令执行和回滚审计。

主要可复用结构：

- `RuntimeOrchestrator::with_actuator(config, actuator)`
- `RuntimeOrchestrator::process_event(event) -> OrchestrationOutcome`
- `RuntimeOrchestrator::tick(now_ms) -> Vec<AppliedAction>`
- `Collector::ingest(event)` 与 `Collector::process_window(pid, now, trailing_window_us)`
- `Classifier::classify_process(snapshot)`
- `PolicyEngine::evaluate_all(contexts)`
- `Actuator::apply(plan, now_ms, require_revert)` 与 `Actuator::expire(now_ms)`
- `MetricsRecorder::record(input)`
- `ExplainTuneEngine::analyze(records, traces, policies)`

## 系统运行完整调用流程

### 1. CLI 解析和配置加载

`agent/runtime_daemon/src/main.rs` 的 `main` 先解析 `CliConfig`，再按
`--config-profile`、`AEGISAI_CONFIG_PROFILE`、local demo 默认值的优先级调用
`RuntimeOrchestratorConfig::load_from_repo_root_with_profile` 读取配置。

随后有两处运行时调整：

1. `runtime_config_for_source`
   - 如果 backend 是 `linux-command` 且 CLI 提供了 `--live-pid-allowlist`，则把 runtime
     selection 切到 `pid_allowlist`，并清空 `process_names`。
2. `config_for_actuator_scope`
   - 如果 backend 是 `linux-command` 且没有 `--enable-live-affinity`，则移除
     Inference Tail Guard 的 `pin_strategy`，让 live path 先收敛到 nice-only。

之后 `build_actuator` 创建 backend，`RuntimeOrchestrator::with_actuator` 创建组合点，
`RuntimeLoop::new` 创建主循环。

### 2. Source 选择

CLI 的 `--source` 决定事件来源：

- `mock`
  - `MockEventSource::demo_sequence()`
  - `MockEventSource::tool_call_lifecycle_sequence()`
- `linux`
  - `LinuxProbeSource::from_runtime_with_config(&runtime_config, cli.probe_reader_config())`

mock source 可以配 `demo`、`noop` 或 Linux 上的 `procfs` metadata；Linux source 只支持
`procfs` metadata。

### 3. Linux probe plan 生成

Linux source 根据 `RuntimeConfig.focus_signals` 调用 `LinuxProbePlan::from_runtime`：

```text
run_queue_delay      -> sched_probe
cpu_migration        -> sched_probe
major_page_fault     -> fault_probe
offcpu_time          -> offcpu_probe
io_latency           -> io_probe
subprocess_start_delay -> runtime-only
queue_wait             -> runtime-only
```

计划生成后进入 `DriverBackedProbeEventReader<RealLinuxProbeDriver>`：

- `ProcfsSchedstatProbeDriver`
  - 读取 `/proc/<pid>/schedstat`、`/proc/<pid>/sched`、`/proc/<pid>/stat`。
  - 生成 procfs 派生的 run queue delay、CPU migration、major page fault 事件。
- `BpfTraceProbeDriver<SystemEbpfHelperPipe>`
  - 通过 `aegisai-ebpf-helper --check` 验证 helper。
  - 启动 `aegisai-ebpf-helper stream --offcpu --io --pid ... --process-name ...`。
  - 读取 helper stdout/stderr 中的 `aegisai_probe ...` 行并转成 probe event。

如果 `ProbeReaderConfig.require_all_probes = true`，任何必要 probe attach 失败都会让 Linux
source 报错；`--allow-partial-probes` 会把它改成 false，让 procfs-backed 信号继续进入
闭环，同时在启动状态中记录失败 probe。

### 4. SourceEvent 转 Runtime Event

`RuntimeLoop::run` 从 `EventSource::poll_batch` 拿到 `SourceEvent` 后调用
`enrich_source_event`。

metadata provider 负责补齐缺失字段：

- `StaticMetadataProvider`：demo 和集成测试用。
- `NoopMetadataProvider`：mock event 已自描述时使用。
- `ProcfsMetadataProvider`：Linux 上读取 process name、cmdline、cgroup、parent pid、
  parent process name、parent cmdline。

补齐后的事件成为共享 contract 中的 `Event`。

### 5. RuntimeLoop 驱动节奏

`RuntimeLoop::run` 对每个事件执行：

1. 按 `tick_interval_ms` 调用 `orchestrator.tick(next_tick)`，让到期 lease 回滚。
2. enrichment 后记录 `signal_observations`。
3. 更新 tool-call lifecycle tracker。
4. 调用 `RuntimeOrchestrator::process_event(runtime_event)`。
5. 汇总 `feature_window_maxima`、applied action、inline rollback、triggered scenario。
6. 达到 `--max-events` 或 source drain 后退出。
7. 最后用 `drain_after_source_ms` 做一次最终 tick，回收 pending rollback。
8. 输出 `RuntimeRunSummary`，并可通过 `--verification-log` 追加到日志。

### 6. Orchestrator 单事件处理

`RuntimeOrchestrator::process_event` 是主热路径：

1. `self.actuator.expire(event.timestamp_ms)`
   - 在新评估前先回滚到期动作。
2. `should_collect_signal`
   - 如果 `focus_signals` 为空或包含该 signal，进入 collector。
3. `to_collector_event`
   - 把 runtime `SignalKind` 转成 collector `EventKind` 和 `ProbeSource`。
4. `self.collector.ingest(collector_event)`
   - 更新窗口和 recent events。
5. `self.classifier.classify(&event)`
   - runtime process name、PID allowlist、parent inference、process rules 一起生成
     `WorkloadProfile`。
6. 为每个 enabled policy 调用 `project_feature_window`
   - 用该 policy 的 `evaluation_window_ms` 生成 trailing process window。
7. 构造 `PolicyContext`
   - 包含 event context、feature window、profile、tool_call_id 等 audit fields。
8. `self.policy_engine.evaluate_all(policy_contexts.iter())`
   - 生成一个或多个 `ActionPlan`。
9. `self.actuator.apply(plan, event.timestamp_ms, self.config.safety.require_revert)`
   - backend apply，并记录 lease。
10. `self.metrics.record(...)`
   - 写 evaluation trace、action trace、rollback trace、notes 和 metric snapshot。
11. 返回 `OrchestrationOutcome`
   - 包含 profile、每个 scenario 的 feature window、applied actions、rollbacks、metric record。

### 7. Classifier/Awareness 细节

`AiWorkloadAwareness` 在 orchestrator 内部组合 runtime config、awareness config 和
`aegisai_classifier::Classifier`：

- runtime process name 命中时，加 `interactive_default` 标签。
- PID allowlist 命中时，加 `interactive_default` 标签。
- parent process 命中 runtime process 时，加 `interactive_default` 标签。
- process rules 命中时，合并规则定义的 tags 和 matched rule id。
- 若已有 `TOOL_CALL`，补 `tool_executor_default`。
- 若已有 `BACKGROUND_JOB`，补 `background_default`。

随后 `WorkloadClass::from_tags`、`StageLabel::from_tags`、`LatencySensitivity::from_tags` 把标签
归约成 policy 需要的 profile。

### 8. PolicyEngine 和场景策略

当前配置加载的 enabled scenario 是：

```text
inference_tail_guard
tool_call_booster
```

`ScenarioKind::AiWorkloadAwareness` 已存在于共享类型和优先级中，但当前 `load_from_repo_root`
没有把它作为独立 active policy 加载；awareness 主要作为 classifier 基础能力服务其他场景。

`PolicyEngine::evaluate_all` 的步骤：

1. 对每个 context 调 `evaluate_candidate`。
2. 应用 scenario-specific evaluator：
   - `inference_tail_guard::evaluate`
   - `tool_call_booster::evaluate`
   - 其他 scenario 走 generic evaluator。
3. 检查 `(pid, scenario)` cooldown。
4. 生成 candidate plan 后，按 `(pid, action slot)` 冲突消解。
5. 冲突时按 scenario priority 保留更高优先级动作：

```text
inference_tail_guard > tool_call_booster > ai_workload_awareness > unknown
```

Inference Tail Guard 只匹配同时带有：

```text
AI_INFERENCE
INTERACTIVE_LATENCY_SENSITIVE
```

Tool Call Booster 只匹配 `TOOL_CALL`，再按标签分 stage：

```text
TOOL_CALL only                    -> executor
TOOL_CALL + RETRIEVAL_STAGE       -> retrieval
TOOL_CALL + RERANK_STAGE          -> rerank
```

Tool Call Booster 的 duration ratio：

```text
executor  -> 1/1
retrieval -> 3/4
rerank    -> 1/2
```

### 9. Actuator apply/rollback

`Actuator` 不直接执行系统命令。它负责：

- 调用 backend apply。
- 合并 `backend.apply.*` 和 lease audit fields。
- 如果 `require_revert = true`，按 `(pid, scenario)` 存储 `ActionLease`。
- `expire(now_ms)` 时按到期时间稳定排序，调用 backend rollback。

后端分层：

- `NoopActuatorBackend`
  - 默认安全后端，只记录 simulated apply/rollback。
- `RecordingActuatorBackend`
  - 测试后端，记录操作序列。
- `LinuxActuatorBackend`
  - 把 `ActionPlan` 转为 `LinuxSyscallPlan`。
- `PlannedOnlyLinuxSyscallExecutor`
  - 捕获 nice/affinity/cpuset 原始状态，执行 apply/rollback operation，并写审计字段。
- `CommandLinuxSyscallApplier`
  - dry-run 或 live command applier。
  - live 时通过 `LiveLinuxCommandGuard` 强制确认、PID allowlist、操作 scope。

live command 当前实际会执行：

```text
renice <target_nice> -p <pid>
taskset -pc <cpu-list> <pid>   # only with --enable-live-affinity
<warmup command>               # only with explicit --warmup-executor-command and timeout
```

当前不会真正实现：

```text
cpuset cgroup write
```

`WarmupExecutor` 的 rollback 仍是审计 no-op：cache/process priming 不做反向清理，
报告会把 warmup side effect 与 scheduler benefit 分开统计。

### 10. Metrics 和 Explain/Tune

`MetricsRecorder::record` 会生成两种视图：

- `MetricRecord`
  - 当前事件的 evaluated scenarios、triggered scenarios、action count、rollback count、
    side effects、tracked metric trends。
- `MetricTrace`
  - scenario evaluation、measurement observed、action applied、action rolled back、side effect
    observed 等解释性 trace。

`ExplainTuneEngine::analyze` 消费 records、traces 和 policies，生成：

- 每个 scenario 的 evaluation/trigger/rollback/side-effect 汇总。
- trigger explanations。
- tool-call chain reports。
- tune suggestions。
- findings。

这个模块当前是离线分析工具，不在热路径里做在线调参。

## 权限边界

产品形态是：

```text
aegisai-runtime-daemon   普通用户进程，负责 config/policy/control/reporting
aegisai-ebpf-helper      受控 privileged helper，负责固定 eBPF/bpftrace attachment
```

daemon 不把任意 bpftrace source 交给 helper。helper CLI 只暴露：

```text
stream --offcpu --io --pid <pid> --process-name <name>
```

当前 helper 内部依赖 bpftrace，涉及：

```text
sched:sched_switch
block:block_rq_issue
block:block_rq_complete
```

因此不同 kernel 的 tracepoint 字段兼容性仍是验证风险。Linux source 支持
`--allow-partial-probes`，让 helper 不可用时仍可保留 procfs-backed signals。

## 验证和 Benchmarks

项目 preflight 清单：

```bash
bash bench/scripts/project_preflight.sh
```

完整执行项目 preflight：

```bash
bash bench/scripts/project_preflight.sh --check
```

workspace 总验证：

```bash
bash bench/scripts/verify_workspace.sh
```

该脚本覆盖：

- `cargo check --workspace`
- `cargo test --workspace`
- Tool Call Booster report unit tests
- Inference Tail Guard report unit tests
- `cargo fmt --all -- --check`
- `cargo clippy --all-targets --all-features -- -D warnings`
- mock daemon smoke
- Linux source partial-probe smoke

2026-05-11 最新系统审计复跑结果：

| check | result | evidence |
| --- | --- | --- |
| `cargo fmt --all -- --check` | `PASS` | 本地命令退出码 `0` |
| `cargo check --workspace` | `PASS` | 本地命令退出码 `0` |
| `cargo test --workspace` | `PASS` | Rust workspace unit/doc tests 全部通过 |
| `cargo clippy --all-targets --all-features -- -D warnings` | `PASS` | 本地命令退出码 `0` |
| `python3 -m unittest discover -s bench/tool_call_booster -p 'test_*.py'` | `PASS` | `14` tests |
| `python3 -m unittest discover -s bench/scripts -p 'test_*.py'` | `PASS` | `15` tests |
| `for f in bench/scripts/*.sh; do bash -n "$f" || exit 1; done` | `PASS` | shell 语法检查退出码 `0` |
| `AEGISAI_VERIFY_LOG=/tmp/aegisai_audit_verify_workspace_20260511.md bash bench/scripts/verify_workspace.sh` | `PASS` | mock daemon `processed_events=3`；Linux source preflight `processed_events=0` |
| `AEGISAI_VERIFY_LOG=/tmp/aegisai_audit_toolchain_preflight_20260511.md bash bench/scripts/toolchain_preflight.sh` | `PASS` | bpftool、bpftrace、clang、llc、taskset、rustfmt、clippy、stress-ng 可用 |
| `AEGISAI_VERIFY_LOG=/tmp/aegisai_audit_inference_preflight_20260511.md bash bench/scripts/inference_tail_guard_preflight.sh` | `PASS` | Ollama `0.21.3-rc0` 可用；llama.cpp binary 未找到并按设计跳过 |
| `bd lint` | `PASS` | 已补齐现有 open issue 的 acceptance criteria |

2026-05-12 项目 preflight 模板替换验证：

| check | result | evidence |
| --- | --- | --- |
| `bash bench/scripts/project_preflight.sh --check` | `PASS` | 执行 Rust/Python/shell/bench preflight；重型 bench 日志写入 `/tmp/aegisai_project_preflight_*.md` |
| `bd lint` | `PASS` | `19` 个 open issue 无 template warning |

本轮测试没有发现当前 suite 的失败用例。审计发现的是边界和盲点：

- Linux source preflight 仍可能是 `processed_events=0`；它证明启动、权限降级和
  partial-probe 安全，不证明真实 Linux ingestion，也不证明性能收益。
- Inference preflight 不拉取或运行模型；`stress-ng` 只记录可用性，不启动压力负载。
- `bd doctor` 在 embedded mode 下不支持；上游 `bd preflight` 在 bd `1.0.3`
  仍输出 Beads 自身 Go/Nix 模板。该输出与本 Rust workspace 无关；本仓库
  readiness gate 是 `bash bench/scripts/project_preflight.sh`。
- code-review-graph 显示 `1286` nodes、`10276` edges、风险低，但也标出
  `20` 个 untested hotspot 和 `28` 个 300 行以上的大文件/大类。
- 图分析曾未找到 `build_linux_rollback_report`、`CliConfig::parse`、
  `BpfTracePipe::start` 的直接测试映射；这些 P1/P2 热点现在已有直接回归测试，
  但 source/report 路径仍应保持窄改动和测试先行。

Inference Tail Guard Phase 4 严格收益报告：

```bash
bash bench/scripts/inference_tail_guard_phase4_report.sh
```

Tool Call Booster repeated A/B harness：

```bash
bash bench/scripts/tool_call_booster_real_executor_harness.sh
```

当前 `docs/mvp_benefit_report.md` 结论：

- `Result: PASS`
- 最新来源是 `docs/mvp_benefit_report.md`。
- live action 有效：live guarded 已记录 `3` 次有效 host-level 动作。
- 稳定收益过线：在 `sample_sizing` 受控复跑中，live guarded jitter 在 `2/3`
  可比较轮次改善，mean delta 为 `5.89%`。
- dry-run/noop 的改善被视为闭环证据，不视为 MVP benefit proof。

最新 Inference Tail Guard artifact 索引：

| run id | CSV | live effective action count | verdict |
| --- | --- | --- | --- |
| `live_guarded_phase4_sample_sizing_20260511T000000Z` | `.cache/aegisai/inference_tail_guard_phase4/live_guarded_phase4_sample_sizing_20260511T000000Z/phase4_runs.csv` | `3` | `PASS`：live guarded jitter `2/3` 轮改善，mean delta `5.89%` |
| `live_guarded_phase4_sample_sizing_20260511T000000Z` | `.cache/aegisai/inference_tail_guard_phase4/live_guarded_phase4_sample_sizing_20260511T000000Z/phase4_aggregate.csv` | `3` | `PASS`：有效 live action 加稳定收益 |
| `live_guarded_phase4_calibrated_20260510T043859Z` | `.cache/aegisai/inference_tail_guard_phase4/live_guarded_phase4_calibrated_20260510T043859Z/phase4_runs.csv` | `3` | noisy workload；稳定收益未证明 |
| `live_guarded_phase4_calibrated_20260510T043859Z` | `.cache/aegisai/inference_tail_guard_phase4/live_guarded_phase4_calibrated_20260510T043859Z/phase4_aggregate.csv` | `3` | noisy workload；稳定收益未证明 |

最新 Tool Call Booster artifact 索引：

| run id | artifact | contract verdict | benefit verdict |
| --- | --- | --- | --- |
| `codex_fixed_work_guarded_final_20260511T141942Z` | `.cache/aegisai/tool_call_booster/codex_fixed_work_guarded_final_20260511T141942Z/tool_call_booster_benefit_report.md` | `PASS` | `PASS`：`live_guarded` 3/3 可比较轮次达到 `5.0%` latency improvement，avg delta `-26.832%`；executor/retrieval/rerank stage effectiveness 全部 `PASS` |
| `codex_fixed_work_guarded_final_20260511T141942Z` | `.cache/aegisai/tool_call_booster/codex_fixed_work_guarded_final_20260511T141942Z/tool_call_booster_summary.csv` | `PASS` | `PASS` |
| `codex_fixed_work_guarded_final_20260511T141942Z` | `.cache/aegisai/tool_call_booster/codex_fixed_work_guarded_final_20260511T141942Z/tool_call_booster_stage_effectiveness.csv` | `PASS` | `PASS` |
| `live_guarded_tcb_fixed_work_verified_pass_20260511T135213Z` | `.cache/aegisai/tool_call_booster/live_guarded_tcb_fixed_work_verified_pass_20260511T135213Z/tool_call_booster_benefit_report.md` | `PASS` | `PASS`：`live_guarded` 3/3 可比较轮次达到 `5.0%` latency improvement，avg delta `-21.495%`；executor/retrieval/rerank stage effectiveness 全部 `PASS` |
| `live_guarded_tcb_fixed_work_verified_pass_20260511T135213Z` | `.cache/aegisai/tool_call_booster/live_guarded_tcb_fixed_work_verified_pass_20260511T135213Z/tool_call_booster_summary.csv` | `PASS` | `PASS` |
| `live_guarded_tcb_stable_executor_20260511T000000Z` | `.cache/aegisai/tool_call_booster/live_guarded_tcb_stable_executor_20260511T000000Z/tool_call_booster_benefit_report.md` | `PASS` | `FAIL`：`live_guarded` 只有 `0/3` 可比较轮次达到 `5.0%` latency improvement，avg delta `1.077%` |
| `live_guarded_tcb_stable_executor_20260511T000000Z` | `.cache/aegisai/tool_call_booster/live_guarded_tcb_stable_executor_20260511T000000Z/tool_call_booster_summary.csv` | `PASS` | `FAIL` |
| `live_guarded_tcb_issue_94s_final_20260510T053527Z` | `.cache/aegisai/tool_call_booster/live_guarded_tcb_issue_94s_final_20260510T053527Z/tool_call_booster_benefit_report.md` | `PASS` | `FAIL`：`live_guarded` 只有 `0/3` 可比较轮次达到 `5.0%` latency improvement |
| `live_guarded_tcb_issue_94s_final_20260510T053527Z` | `.cache/aegisai/tool_call_booster/live_guarded_tcb_issue_94s_final_20260510T053527Z/tool_call_booster_summary.csv` | `PASS` | `FAIL` |

受控 fixed-work proof 可用以下命令复跑：

```bash
AEGISAI_TCB_PROFILE=fixed_work_guarded \
AEGISAI_CONFIRM_LIVE_ACTUATOR=1 \
  bash bench/scripts/tool_call_booster_real_executor_harness.sh
```

该 profile 固化 fixed hash work、baseline/background CPU affinity 和 live
guarded nice+affinity，用于证明 scheduler 隔离收益；stable executor-control
`FAIL` 仍保留为非受控工作量边界，不再代表 Tool Call Booster 的最新总体收益状态。

## 已知问题和差距

`bd` 是任务源，最新精细任务清单见 `docs/latest_tasks.md`。当前 open issue：

- `AegisAI_Runtime-0ry.2` / `AegisAI_Runtime-0ry.3`：dashboard 和 GPU
  coordination 已拆成 evidence-gated future work；production packaging gate
  已完成，但二者仍需独立 safety/benchmark/verification gate。

最近关闭的父级 gap：

- `AegisAI_Runtime-ufp`：Debian/Ubuntu systemd packaging 已实现到
  `packaging/debian-systemd/`，包含 rootless daemon service、单独 helper
  install path、production profile staging、preflight、remove/purge 和 dry-run
  smoke。
- `AegisAI_Runtime-0ry`：deferred extension parent 已关闭；dashboard、GPU
  coordination 和 online adaptive policy 拆成独立 evidence-gated future work。
- `AegisAI_Runtime-0ry.4`：online adaptive policy 的 shadow-only evidence
  gate 已完成，设计在 `docs/adaptive_policy_gate.md`，离线 replay/benchmark
  gate 在 `bench/scripts/adaptive_policy_gate.py`，最新 artifact 在
  `.cache/aegisai/adaptive_policy_gate/codex_adaptive_policy_gate_20260514T000000Z/`。
- `AegisAI_Runtime-dxh`：`aegisai-runtime-daemon --help` 现在打印当前 usage
  并以 `0` 退出；无效或不完整参数仍保持 nonzero。
- `AegisAI_Runtime-cqv`：production config profile selector、strict schema
  validation 和 cross-file safety validation 已完成。
- `AegisAI_Runtime-51c`：helper compatibility taxonomy、two-kernel helper
  matrix、controlled Linux ingestion smoke 和 BpfTracePipe startup failure
  coverage 已完成。
- `AegisAI_Runtime-8le`：Beads Dolt remote sync 已配置为 local-only filesystem
  remote。

源码和设计层面的限制：

- `ai_workload_awareness` 当前是 classifier/awareness 基础能力，不是已加载的独立 active
  scenario policy。
- eBPF crate 主要提供 probe contract/descriptor/registry；当前 Linux 主路径的 sched/fault
  信号来自 procfs 派生，offcpu/io 来自 helper-backed bpftrace。
- bpftrace I/O 程序依赖 host block tracepoint 字段，跨 kernel 可移植性仍需实测。
- `linux-command` 通过 `renice`/`taskset` 命令执行，而不是直接 syscall 或 cgroup API。
- cpuset/background throttling 在 policy/audit surface 中存在；当前 dry-run-only
  planner 已完成并覆盖 rejection matrix，但 live cgroup write 仍未启用。
- warmup executor 默认仍是 deferred audit；只有显式 CLI warmup command 才会产生受超时约束的真实 side effect，且 rollback 是 no-op audit。
- named production profile 已支持 selector、strict schema validation 和 cross-file safety
  validation；仍没有动态 reload、profile overlay、远程配置分发或完整 TOML 解析库。
- Linux source direct preflight 仍可能在 `processed_events=0` 时通过；受控 procfs
  ingestion proof 使用 `bash bench/scripts/linux_source_ingestion_smoke.sh`。
- 项目 readiness gate 已由 `bash bench/scripts/project_preflight.sh` 统一列出
  Cargo、Python unittest、shell 语法和 bench preflight 组合。上游
  `bd preflight` 的 Go/Nix 输出只适用于 Beads 自身模板，不代表本仓库质量门。
- Beads Dolt remote 已按 local-only 策略配置；`bd dolt remote list` 显示 `origin`
  指向 `file:///home/gg/AegisAI_Runtime/.beads/backup/dolt-remote/AegisAI_Runtime`。
- 当前热点大文件仍包括 `agent/runtime_daemon/src/source.rs`、
  `agent/actuator/src/backend.rs`、`bench/scripts/inference_tail_guard_ollama_smoke.sh`、
  `agent/actuator/src/lib.rs`、`agent/runtime_daemon/src/main.rs`、
  `bench/scripts/inference_tail_guard_phase4_report.sh`、
  `bench/tool_call_booster/summarize_ab.py`、`agent/runtime_orchestrator/src/runtime_orchestrator.rs`、
  `agent/explain_tune/src/engine.rs`、`agent/runtime_daemon/src/runtime_loop.rs`、
  `scenarios/tool_call_booster/src/policy.rs` 和 `agent/policy_engine/src/engine.rs`；
  后续修改应小步、测试先行。
- 还没有 dashboard、GPU scheduler 或在线 adaptive policy live loop；adaptive policy
  当前只有 shadow-only evidence gate。

## 阅读路线

- `docs/README.md`：docs 目录职责边界和阅读顺序。
- `docs/status.md`：当前状态、最新 artifact 索引和开放缺口。
- `docs/latest_tasks.md`：基于当前缺口拆出的最新精细任务清单。
- `docs/strategy.md`：MVP 定义、严格收益规则、当前阶段和实验方法。
- `docs/acceptance_ledger.md`：已验收 19 项 evidence-hardening 台账。
- `docs/linux_validation.md`：Linux 主机、helper 和 live guarded 实验检查清单。
- `docs/adaptive_policy_gate.md`：online adaptive policy 的 shadow-only
  evidence gate 和 promotion boundary。
- `docs/mvp_benefit_report.md`：最新收益证据和当前 Inference Tail Guard PASS 结论。
- `docs/verification_log.md`：append-only 验证历史。
- `docs/architecture.md`：稳定架构、部署边界和工程债边界。
- `agent/runtime_daemon/README.md`：CLI/source/metadata 细节。
- `agent/actuator/README.md`：backend 和 rollback 设计。
- `agent/ebpf_helper/README.md`：helper 权限边界。
- `bench/README.md`：benchmark 组织原则。
