# scripts

用于放置实验驱动脚本、标签回放脚本和结果整理脚本。

## 当前脚本

- `verify_workspace.sh`：运行当前工作区验证，并把命令、退出码和关键输出追加到 `docs/verification_log.md`。
- `toolchain_preflight.sh`：盘点 pre-Ollama 阶段需要的开发、eBPF 和 demo 工具；不执行安装，只记录缺失项和建议安装命令。必需工具缺失会使脚本失败；可选工具缺失只作为 inventory 记录。
- `inference_tail_guard_preflight.sh`：检查 Linux VM/demo 是否具备 `Inference Tail Guard` 下一步需要的基础面。必需项是 procfs/cgroup 可见性和 mock/noop runtime daemon smoke test；`ollama`、`llama.cpp`、`stress-ng`、`taskset` 只做可选工具盘点。该阶段不安装 Ollama、不拉取模型、不启动压力负载。
- `inference_tail_guard_ollama_smoke.sh`：运行正式 `ollama` A/B harness。默认安全三档是 `baseline`、`noop_observation`、`dry_run`；每档固定同一模型、prompt、并发和 CPU 干扰强度，输出 TTFT、P95/P99、jitter、trigger count、rollback count、`cpu_migration` 与 `major_page_fault` 观测统计，并把原始样本和汇总追加到 `docs/verification_log.md`。
- 该 harness 同时会写出 2R-0 验收基线：`acceptance_baseline.env`、`cpu_topology.txt`、`permission_state.txt` 和 `mode_contract.csv`，用于锁定模型/prompt/并发/干扰/样本数/CPU 拓扑/权限状态，并把 `noop_observation`、`dry_run`、`live_guarded` nice-only 分档验收。
- `inference_tail_guard_phase2r2_actuator_quality.sh`：阶段 2R-2 actuator 质量收敛入口。它先跑至少 3 轮 `live_guarded` nice-only，要求无 action audit error、记录 lease、记录 rollback、cpuset 禁用；通过后才跑一轮 affinity。
- `inference_tail_guard_phase4_report.sh`：阶段 4 多轮收益报告入口。它会循环跑 CPU 干扰和可选 CPU+I/O 扰动矩阵，汇总每轮 `summary.csv`，输出 `docs/mvp_benefit_report.md` 和 `.cache/aegisai/inference_tail_guard_phase4/<run_id>/` 下的对照 CSV。
- `tool_call_booster_real_executor_harness.sh`：阶段 2R-5 Tool Call Booster 入口。它启动真实本地 tool executor / retrieval / rerank / background 进程树，再用 runtime daemon `linux` + `procfs` source 验证 `tool_call_lifecycles`、`tool_call_booster` 触发和 noop/dry-run 可回滚链路。

## Tool Call Booster real executor harness

Phase 2R 通过后，Tool Call Booster 小阶段使用真实 executor 样本：

```bash
bash bench/scripts/tool_call_booster_real_executor_harness.sh
```

默认会跑 `noop` 和 `linux-command-dry-run` 两档，artifact 写入
`.cache/aegisai/tool_call_booster/<run_id>/`。`PASS` 表示 daemon 从真实进程树
中捕获同一 `tool_call_id` 的 executor / retrieval / rerank lifecycle，
并且 `tool_call_booster` 至少触发一次可回滚 action。该阶段仍不声明
background isolation 或 explain/tune 已正式固化。

## 真实 Ollama A/B harness 前置条件

- 必需命令：`ollama`、`cargo`、`curl`、`python3`、`stress-ng`
- 可选 live 命令：`live_guarded` nice-only 需要 `renice`；启用 `AEGISAI_ENABLE_LIVE_AFFINITY=1` 时还需要 `taskset`
- 必需环境：本地 `ollama serve` 已启动，且 `AEGISAI_OLLAMA_API_URL` 指向的地址可达
- 必需模型：目标模型已经在本机可用，至少能通过 `ollama show <model>` 成功返回
- `live_guarded` 档必须显式加入 `AEGISAI_AB_MODES`，并设置 `AEGISAI_CONFIRM_LIVE_ACTUATOR=1` 和 `AEGISAI_LIVE_PID_ALLOWLIST=<pid,...>`；默认只执行/回滚 `renice`，`taskset` 需要 `AEGISAI_ENABLE_LIVE_AFFINITY=1`

`inference_tail_guard_preflight.sh` 是强烈推荐的前置步骤，但不是 `inference_tail_guard_ollama_smoke.sh` 的内建 hard gate。harness 会直接尝试请求本地 Ollama 服务；如果服务未启动、模型未准备好，脚本会把失败写进 `docs/verification_log.md` 并退出非零。

## 使用方式

```bash
bash bench/scripts/verify_workspace.sh
```

```bash
bash bench/scripts/toolchain_preflight.sh
```

```bash
bash bench/scripts/inference_tail_guard_preflight.sh
```

```bash
bash bench/scripts/inference_tail_guard_ollama_smoke.sh
```

```bash
bash bench/scripts/tool_call_booster_real_executor_harness.sh
```

可用 `AEGISAI_VERIFY_LOG=/path/to/log.md` 覆盖日志路径。
可用 `AEGISAI_OLLAMA_MODEL=qwen2.5:0.5b`、`AEGISAI_AB_SAMPLES=12`、`AEGISAI_AB_CONCURRENCY=2`、`AEGISAI_STRESS_CPU=2` 覆盖默认实验参数。
可用 `AEGISAI_STRESS_IO=1`、`AEGISAI_STRESS_HDD=1`、`AEGISAI_STRESS_HDD_BYTES=128M` 给单次 harness 增加可选 I/O 扰动。
默认 `AEGISAI_STRESS_TIMEOUT=0`，表示 `stress-ng` 由 harness 在每个档位开始/结束时启动和停止；设置为正整数时作为压力源 self-timeout 上限，若压力源提前结束则该档失败。
默认安全三档：

```bash
bash bench/scripts/inference_tail_guard_ollama_smoke.sh
```

显式受控 live nice-only：

```bash
AEGISAI_AB_MODES=baseline,noop_observation,dry_run,live_guarded \
AEGISAI_CONFIRM_LIVE_ACTUATOR=1 \
AEGISAI_LIVE_PID_ALLOWLIST=1234 \
  bash bench/scripts/inference_tail_guard_ollama_smoke.sh
```

也可以只跑单档做故障定位，但这不构成完整 Phase 2 MVP A/B 证明：

```bash
AEGISAI_AB_MODES=dry_run \
  bash bench/scripts/inference_tail_guard_ollama_smoke.sh
```

阶段 4 多轮收益报告：

```bash
bash bench/scripts/inference_tail_guard_phase4_report.sh
```

阶段 2R-2 actuator 质量收敛：

```bash
AEGISAI_CONFIRM_LIVE_ACTUATOR=1 \
AEGISAI_LIVE_PID_ALLOWLIST=1234 \
  bash bench/scripts/inference_tail_guard_phase2r2_actuator_quality.sh
```

2R-2 默认先跑 `AEGISAI_PHASE2R2_NICE_ROUNDS=3` 轮 nice-only；只有三轮都满足 `mode_contract=PASS`、`actuator_quality_contract=PASS`、`live_cpuset_disabled_contract=PASS`、`action_error_count=0`，才会用 `AEGISAI_ENABLE_LIVE_AFFINITY=1` 跑 affinity。临时只收敛 nice-only 可设 `AEGISAI_PHASE2R2_RUN_AFFINITY=0`。

常用覆盖：

```bash
AEGISAI_PHASE4_ROUNDS=3 \
AEGISAI_AB_SAMPLES=8 \
AEGISAI_AB_CONCURRENCY=2 \
AEGISAI_PHASE4_SCENARIOS=cpu,cpu_io \
  bash bench/scripts/inference_tail_guard_phase4_report.sh
```

阶段 4 判定更严格：只有当 `live_guarded` 档在至少三分之二可比较轮次里改善，且平均改善不低于 5%，并且指标属于 TTFT P95/P99、latency P95/P99 或 jitter，报告才会给出 `PASS`。`noop_observation` 和 `dry_run` 能证明闭环触发与回滚，但不会单独证明真实主机收益；若 live `renice` 被权限拒绝，需要在报告中标为收益未证明。

结果文件：

- raw samples：`.cache/aegisai/inference_tail_guard/<run_id>/samples.csv`
- per-mode counts：`.cache/aegisai/inference_tail_guard/<run_id>/mode_counts.csv`
- per-mode acceptance contracts：`.cache/aegisai/inference_tail_guard/<run_id>/mode_contract.csv`
- Phase 2R-2 actuator quality：`.cache/aegisai/inference_tail_guard_phase2r2/<run_id>/phase2r2_actuator_quality.csv`
- summary：`.cache/aegisai/inference_tail_guard/<run_id>/summary.csv`
- run controls：`.cache/aegisai/inference_tail_guard/<run_id>/run.env`
- fixed acceptance baseline：`.cache/aegisai/inference_tail_guard/<run_id>/acceptance_baseline.env`
- CPU topology snapshot：`.cache/aegisai/inference_tail_guard/<run_id>/cpu_topology.txt`
- permission snapshot：`.cache/aegisai/inference_tail_guard/<run_id>/permission_state.txt`
- Phase 4 report：`docs/mvp_benefit_report.md`
- Phase 4 aggregate：`.cache/aegisai/inference_tail_guard_phase4/<run_id>/phase4_aggregate.csv`

结果解释：

- `PASS` 代表选中的每个档位都完成固定样本数，并且 observation/guarded 档捕获 runtime events、触发 `inference_tail_guard`、完成 rollback，且没有 action audit errors。
- summary 中的 TTFT 来自 streaming request 的 `curl time_starttransfer`；P95/P99 和 jitter 来自 streaming request 的 total latency。
- `mode_counts.csv` 和 `summary.csv` 中的 `cpu_migration_*`、`major_page_fault_*` 来自 Linux procfs fallback：`/proc/<pid>/sched` 的 `se.nr_migrations` delta 与 `/proc/<pid>/stat` 的 majflt delta，并额外记录策略窗口中的最大 per-second rate。它们是实机解释指标，0 也表示该轮没有观测到对应 delta。
- `offcpu_time_events` 只作记录；`offcpu_time` 仍是 eBPF/后续增强项，不阻塞收益复验。
- `dry_run` 只预览 planned `renice/taskset` apply/rollback，不改系统状态。
- 2R-0 中 `noop_observation` 只看策略识别和 rollback 生命周期，`dry_run` 额外看 action audit，`live_guarded` 只看 nice-only 真实执行/回滚；这些结论以 `mode_contract.csv` 分开记录。
- 2R-2 中 `live_guarded` 额外检查 `actuator_quality_contract`：apply audit 要暴露原始状态与 `lease.*` 字段，rollback audit 要暴露恢复结果，cpuset 必须继续禁用。affinity 只有在 nice-only 连续 3 轮干净后才启用。
- `live_guarded` 会实际执行并回滚系统命令，运行前要确认主机权限、PID allowlist 和实验窗口；2R-0 保持 `AEGISAI_ENABLE_LIVE_AFFINITY=0`，cpuset 继续禁用。
- 常见失败原因是 `ollama` 不在 PATH、`ollama serve` 未启动、`AEGISAI_OLLAMA_API_URL` 不可达，或者目标模型尚未在本机准备好。

推荐顺序：

```bash
bash bench/scripts/toolchain_preflight.sh
# 按日志中的 required-tool install 建议安装缺失的必需工具后，再重跑：
bash bench/scripts/toolchain_preflight.sh
bash bench/scripts/inference_tail_guard_preflight.sh
bash bench/scripts/inference_tail_guard_ollama_smoke.sh
bash bench/scripts/verify_workspace.sh
```
