# Inference Tail Guard Benchmark

用于验证 AI 推理尾延迟保护是否成立。

## 预检

先运行不需要真实模型下载的 Linux VM/demo 预检：

```bash
bash bench/scripts/inference_tail_guard_preflight.sh
```

该脚本会把 validation-style 结果追加到 `docs/verification_log.md`。这个阶段只确认安装 Ollama/模型之前的 VM readiness，不会安装 Ollama、不会拉取模型。

必需检查：

- Linux kernel、procfs、cgroup/cpuset 可见性
- `aegisai-runtime-daemon` mock/noop 路径是否能安全跑通

可选工具盘点：

- `ollama` 是否在 PATH 中
- 常见 `llama.cpp` 二进制是否在 PATH 中：`llama-cli`、`llama-server`、`llama-main`
- `stress-ng` 和 `taskset` 是否在 PATH 中

缺失或版本命令失败的 demo 工具会被标记为 `SKIPPED` 或 `NON_BLOCKING`，不会导致预检失败。脚本默认不会运行 `ollama run`、不会读取 GGUF 模型、不会执行 `stress-ng` 压力负载。

可用 `AEGISAI_VERIFY_LOG=/path/to/log.md` 覆盖日志路径。

## Phase 2 MVP A/B harness

真实 Ollama 对照实验入口：

```bash
bash bench/scripts/inference_tail_guard_ollama_smoke.sh
```

这个入口现在是可复现 A/B harness，不再是单次 smoke。默认安全三档为：

- `baseline`：固定 prompt / 模型 / 并发 / CPU 干扰，只跑 Ollama 请求，不启动 daemon
- `noop_observation`：启动 daemon 和 `noop` backend，只观察策略触发与回退生命周期
- `dry_run`：启动 `linux-command-dry-run`，记录计划中的 `renice/taskset` apply/rollback 审计，不改系统状态

可选 live 档：

- `live_guarded`：启动 `linux-command`，只在显式确认和 PID allowlist 下对真实目标 PID 执行受限 boost 与 rollback

默认固定项：

- 模型：`qwen2.5:0.5b`
- prompt：脚本内固定中文 prompt，可用 `AEGISAI_OLLAMA_PROMPT` 覆盖
- 每档样本数：`AEGISAI_AB_SAMPLES=12`
- 并发：`AEGISAI_AB_CONCURRENCY=2`
- CPU 干扰：`stress-ng --cpu 2`
- 压力源生命周期：默认 `AEGISAI_STRESS_TIMEOUT=0`，由 harness 在每个档位开始/结束时启动和停止；设置为正整数时作为 self-timeout 上限，若压力源提前结束则该档失败
- 输出：`TTFT p50/p95/p99`、latency `P95/P99`、jitter、trigger count、rollback count、`cpu_migration` 与 `major_page_fault` 观测统计

运行前请先确认：

- 本地 `ollama serve` 已启动，且默认 API 地址 `http://127.0.0.1:11434` 可达
- 目标模型已经在本机准备好，至少能通过 `ollama show <model>` 成功返回
- `cargo`、`curl`、`python3`、`stress-ng` 在 PATH 中
- `live_guarded` 会调用真实 `renice`，并在显式启用 affinity 时调用真实 `taskset`；需要当前主机权限和实验窗口允许
- `live_guarded` 必须设置 `AEGISAI_CONFIRM_LIVE_ACTUATOR=1` 和 `AEGISAI_LIVE_PID_ALLOWLIST=<pid,...>`；默认 scope 是 nice-only
- `AEGISAI_ENABLE_LIVE_AFFINITY=1` 会启用 `taskset`，当前 planner 会先求 `/proc/<pid>/status` allowed CPU 与 `/sys/devices/system/cpu/online` 的交集；`cpuset` 继续禁用

`bench/scripts/inference_tail_guard_preflight.sh` 仍然是推荐前置，但不是这个 harness 的内建 gate。

常用覆盖：

```bash
AEGISAI_AB_MODES=baseline,noop_observation,dry_run \
AEGISAI_AB_SAMPLES=12 \
AEGISAI_AB_CONCURRENCY=2 \
AEGISAI_STRESS_CPU=2 \
  bash bench/scripts/inference_tail_guard_ollama_smoke.sh
```

可选 I/O 扰动：

```bash
AEGISAI_AB_MODES=baseline,noop_observation,dry_run \
AEGISAI_STRESS_CPU=2 \
AEGISAI_STRESS_IO=1 \
AEGISAI_STRESS_HDD=1 \
AEGISAI_STRESS_HDD_BYTES=128M \
  bash bench/scripts/inference_tail_guard_ollama_smoke.sh
```

受控 live nice-only：

```bash
AEGISAI_AB_MODES=baseline,noop_observation,dry_run,live_guarded \
AEGISAI_CONFIRM_LIVE_ACTUATOR=1 \
AEGISAI_LIVE_PID_ALLOWLIST=1234 \
  bash bench/scripts/inference_tail_guard_ollama_smoke.sh
```

默认结果会写到：

- append-only 验证日志：`docs/verification_log.md`
- 原始样本和汇总：`.cache/aegisai/inference_tail_guard/<run_id>/`
- 2R-0 验收基线：`.cache/aegisai/inference_tail_guard/<run_id>/acceptance_baseline.env`
- CPU 拓扑快照：`.cache/aegisai/inference_tail_guard/<run_id>/cpu_topology.txt`
- 权限状态快照：`.cache/aegisai/inference_tail_guard/<run_id>/permission_state.txt`
- 分档验收结果：`.cache/aegisai/inference_tail_guard/<run_id>/mode_contract.csv`

`mode_counts.csv` 和 `summary.csv` 会记录 Linux procfs fallback 观测到的
`cpu_migration_events`、`cpu_migration_total`、`cpu_migrations_per_sec_max`、
`major_page_fault_events`、`major_page_fault_total` 和
`major_page_faults_per_sec_max`。这些值来自目标进程/线程的
`/proc/<pid>/sched` 与 `/proc/<pid>/stat` delta，用于解释实机实验中的调度迁移
和 major fault 压力；值为 0 表示该轮没有观测到对应 delta，不会被当作采样失败。
`offcpu_time_events` 可由 helper-backed eBPF/bpftrace 路径提供；它仍是解释性观测，
不阻塞收益复验。

## Phase 2R-0 固定验收基线

阶段 2R-0 的目标不是证明收益，而是先锁住验收条件，避免后续把策略识别、dry-run 审计和主机权限问题混在一起解释。每次 harness 会固定并落盘：

- 模型、prompt 与 prompt sha256
- Ollama request shape：`num_predict`、`temperature`、`seed`、`keep_alive`
- 样本数、并发、干扰强度和压力源生命周期
- CPU 拓扑：online/configured CPU、allowed CPU list、cpuset、cgroup、`lscpu` 摘要
- 权限状态：uid/user/groups、当前 nice、`CapEff`、`CAP_SYS_NICE` 是否有效、`renice`/`taskset` 是否存在、live allowlist 和 live scope

2R-0 分档验收口径：

- `noop_observation`：只验收 runtime event 捕获、`inference_tail_guard` 触发和 rollback 生命周期，不验收系统命令权限。
- `dry_run`：验收同一策略识别闭环，加上 `linux-command-dry-run` action audit 无错误；它仍不证明主机层收益。
- `live_guarded`：验收 guarded live 闭环，要求 `AEGISAI_CONFIRM_LIVE_ACTUATOR=1` 和 `AEGISAI_LIVE_PID_ALLOWLIST=<pid,...>`；默认只覆盖 nice，设置 `AEGISAI_ENABLE_LIVE_AFFINITY=1` 时同时验收 `taskset` apply/rollback 是否有 action audit 错误。

## Phase 2R-2 actuator 质量收敛

阶段 2R-2 的目标是先把 live actuator 质量站稳，不做收益判断：

- nice-only 至少 3 轮通过，且 `action_error_count=0`
- 每次 apply 都能在 audit 中看到原始状态与 `backend.apply.lease.*`
- rollback 能在 audit 中看到恢复结果
- cpuset 继续禁用，不能出现 cpuset apply/restore 命令
- 只有 nice-only gate 通过后才启用 affinity

入口：

```bash
AEGISAI_CONFIRM_LIVE_ACTUATOR=1 \
AEGISAI_LIVE_PID_ALLOWLIST=1234 \
  bash bench/scripts/inference_tail_guard_phase2r2_actuator_quality.sh
```

常用覆盖：

```bash
AEGISAI_PHASE2R2_NICE_ROUNDS=3 \
AEGISAI_PHASE2R2_RUN_AFFINITY=1 \
AEGISAI_AB_SAMPLES=4 \
AEGISAI_AB_CONCURRENCY=2 \
AEGISAI_STRESS_CPU=2 \
  bash bench/scripts/inference_tail_guard_phase2r2_actuator_quality.sh
```

结果写入 `.cache/aegisai/inference_tail_guard_phase2r2/<run_id>/phase2r2_actuator_quality.csv`。单轮 `mode_contract.csv` 现在包含 `live_nice_only_contract`、`live_affinity_contract`、`live_cpuset_disabled_contract` 和 `actuator_quality_contract`，用于区分 nice-only、affinity、cpuset 禁用和 actuator audit 质量。

## Phase 2R-3 观测信号补齐

阶段 2R-3 保留 procfs fallback，不把 `cpu_migration`、`major_page_fault` 等同于
eBPF 已完成，而是把它们变成实机实验可解释指标：

- daemon summary 输出 `signal_observations`：每个 signal 的事件数、delta 总量和单次最大 delta
- daemon summary 输出 `feature_window_maxima`：策略窗口里观察到的最大 `cpu_migrations_per_sec` 与 `major_page_faults_per_sec`
- harness 把这些值写入 `mode_counts.csv`、`summary.csv` 和验证日志摘录
- `mode_contract.csv` 增加 `observation_signal_contract`，只要求这些观测字段可解析；不要求每轮必须出现非零迁移或 major fault
- `offcpu_time` 可由 helper-backed 路径提供，但不阻塞 2R-3 和后续收益复验

建议把三类验收拆开跑，且显式复用同一组控制项：

```bash
AEGISAI_OLLAMA_MODEL=qwen2.5:0.5b \
AEGISAI_AB_SAMPLES=12 \
AEGISAI_AB_CONCURRENCY=2 \
AEGISAI_STRESS_CPU=2 \
AEGISAI_AB_MODES=noop_observation \
  bash bench/scripts/inference_tail_guard_ollama_smoke.sh
```

```bash
AEGISAI_OLLAMA_MODEL=qwen2.5:0.5b \
AEGISAI_AB_SAMPLES=12 \
AEGISAI_AB_CONCURRENCY=2 \
AEGISAI_STRESS_CPU=2 \
AEGISAI_AB_MODES=dry_run \
  bash bench/scripts/inference_tail_guard_ollama_smoke.sh
```

```bash
AEGISAI_OLLAMA_MODEL=qwen2.5:0.5b \
AEGISAI_AB_SAMPLES=12 \
AEGISAI_AB_CONCURRENCY=2 \
AEGISAI_STRESS_CPU=2 \
AEGISAI_AB_MODES=live_guarded \
AEGISAI_CONFIRM_LIVE_ACTUATOR=1 \
AEGISAI_LIVE_PID_ALLOWLIST=1234 \
AEGISAI_ENABLE_LIVE_AFFINITY=0 \
  bash bench/scripts/inference_tail_guard_ollama_smoke.sh
```

退出条件：

- 每档样本都必须完成，HTTP 200，并收到 Ollama streaming `done=true`
- 所有档位使用同一模型、prompt、样本数、并发和 CPU 干扰强度
- `noop_observation`、`dry_run`、`live_guarded` 必须捕获 daemon events、触发 `inference_tail_guard`，并产生 rollback
- `noop_observation`、`dry_run`、`live_guarded` 必须在 CSV 中暴露可解释的 `cpu_migration` / `major_page_fault` 观测字段
- `dry_run` 和 `live_guarded` 不能出现 apply/rollback 审计错误
- `mode_contract.csv` 必须显示每个已选档位的 `mode_contract=PASS`；其中 `live_guarded` 在 2R-0 必须保持 `live_nice_only_contract=PASS`
- `stress-ng` 不能在单档实验结束前提前耗尽

因此 `PASS` 表示实验矩阵可复现，不再只是“单次请求 smoke 通过”。

如果暂时不想执行真实系统干预，先跑安全子集：

```bash
AEGISAI_AB_MODES=baseline,noop_observation,dry_run \
  bash bench/scripts/inference_tail_guard_ollama_smoke.sh
```

## Phase 4 MVP benefit report

阶段 4 入口会跑多轮 CPU 干扰与可选 I/O 扰动矩阵，并生成对照表和解释报告：

```bash
bash bench/scripts/inference_tail_guard_phase4_report.sh
```

默认控制项：

- 场景：`cpu,cpu_io`
- 轮数：`AEGISAI_PHASE4_ROUNDS=3`
- 样本数：继承 `AEGISAI_AB_SAMPLES`，默认 `8`
- 并发：继承 `AEGISAI_AB_CONCURRENCY`，默认 `2`
- 模式：`baseline,noop_observation,dry_run`
- 报告：`docs/mvp_benefit_report.md`
- 汇总：`.cache/aegisai/inference_tail_guard_phase4/<run_id>/phase4_aggregate.csv`

Phase 4 tuning runs must declare exactly one changed variable with
`AEGISAI_PHASE4_TUNED_VARIABLE`. Allowed values are `none_control`,
`cpu_selection`, `stress_shape`, `sample_sizing`, `model_runtime`, and
`affinity_nice_interaction`. Add `AEGISAI_PHASE4_TUNED_VARIABLE_DETAIL` to record
the concrete change, for example:

```bash
AEGISAI_PHASE4_TUNED_VARIABLE=stress_shape \
AEGISAI_PHASE4_TUNED_VARIABLE_DETAIL="Changed CPU workers from 1 to 2; model, samples, runtime, and live policy unchanged" \
  bash bench/scripts/inference_tail_guard_phase4_report.sh
```

The report writes the changed variable into `phase4_runs.csv`,
`phase4_aggregate.csv`, `docs/mvp_benefit_report.md`, and
`docs/verification_log.md`. A failed report classifies the primary cause as
`action_effectiveness`, `noisy_workload`, `insufficient_sample_size`, or
`no_measurable_benefit` so CPU selection, stress shape, sample sizing,
model/runtime behavior, and affinity/nice interaction can be evaluated
independently.

阶段 4 成功条件比单次 harness 更严格：只有当 `live_guarded` 的 TTFT P95/P99、latency P95/P99 或 jitter 在至少三分之二可比较轮次里相对 baseline 改善，平均改善不低于 5%，并且 live daemon 审计显示至少一次有效主机级 actuator 变化，才算看到稳定收益趋势。`noop_observation` 与 `dry_run` 可以证明识别、触发、审计和 rollback 闭环；真实收益仍需要 live guarded actuator 在当前主机权限下有效执行并出现同样趋势。如果 live `renice` 被权限限制为 no-op，报告必须标记为收益未证明，而不是把闭环或 dry-run 结果当成收益。

本机 affinity 收敛标记：当前 VM 上 `/proc/<pid>/status` 可能暴露 configured CPU 范围，而 online CPU 只有 `/sys/devices/system/cpu/online` 中的子集；live actuator 会先通过 `agent/actuator/src/cpu_affinity.rs` 取交集再规划 `taskset` 目标，Phase 4 也只在 `taskset -pc` 的 current/new affinity CPU 集合真的不同时计入 `live_effective_action_count`。

## Phase 5 前置归因与 dry-run isolation planning

Phase 5 不直接从当前 MVP 进入 live cgroup/background isolation。先生成三个前置证据：

```bash
python3 bench/scripts/inference_tail_guard_tail_attribution.py
```

默认读取当前基线
`.cache/aegisai/inference_tail_guard_phase4/live_guarded_phase4_sample_sizing_20260511T000000Z/phase4_runs.csv`，
输出：

- `docs/tail_guard_attribution_report.md`
- `.cache/aegisai/inference_tail_guard_tail_attribution/<run_id>/tail_attribution.csv`
- `.cache/aegisai/inference_tail_guard_tail_attribution/<run_id>/tail_attribution_summary.json`

报告会把 Phase 4 样本拆成 `model/runtime`、`run_queue_delay`、helper-backed
`offcpu_time` / `io_latency`、CPU migration、major page fault、trigger/apply/rollback
audit，并给出 `scheduler_attributable_tail_pct` 和 P95/P99 达到 `15%` 改善的
理论可行性判断。CPU migration 与 major page fault 是事件压力归因；只有
`run_queue_delay`、`offcpu_time` 和 `io_latency` 被加进 duration-backed tail 百分比。

helper-backed 信号用现有 smoke 重新分类：

```bash
bash bench/scripts/helper_portability_smoke.sh
```

无论 helper 当前是 `validated signal`、`helper unavailable`、`tracepoint incompatible`
还是 `no workload events`，脚本都会写：

- `helper_signal_availability.json`
- `helper_signal_availability.csv`

这些 artifact 明确 Phase 5 是否把 `offcpu_time` / `io_latency` 纳入，或者把它们作为 intentional-unavailable/excluded bucket 处理。

后台降级先保持 dry-run-only：

```bash
python3 bench/scripts/inference_tail_guard_background_demotion_planner.py
```

输出：

- `docs/tail_guard_background_demotion_plan.md`
- `.cache/aegisai/inference_tail_guard_background_demotion/<run_id>/background_demotion_plan.json`
- `.cache/aegisai/inference_tail_guard_background_demotion/<run_id>/background_demotion_candidates.csv`

planner 只接受明确分类为 `BACKGROUND_JOB` 的进程作为候选，显式拒绝 unknown、
interactive-sensitive 和 protected inference 进程，限制 affected set，并记录后续
live applier 必须捕获的 rollback 状态。它不会执行 `renice`、`taskset`，也不会写入
`cpu.weight`、`cpu.max`、`cgroup.procs` 或任何 cgroup 路径。
