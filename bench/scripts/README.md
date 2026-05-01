# scripts

用于放置实验驱动脚本、标签回放脚本和结果整理脚本。

## 当前脚本

- `verify_workspace.sh`：运行当前工作区验证，并把命令、退出码和关键输出追加到 `docs/verification_log.md`。
- `toolchain_preflight.sh`：盘点 pre-Ollama 阶段需要的开发、eBPF 和 demo 工具；不执行安装，只记录缺失项和建议安装命令。必需工具缺失会使脚本失败；可选工具缺失只作为 inventory 记录。
- `inference_tail_guard_preflight.sh`：检查 Linux VM/demo 是否具备 `Inference Tail Guard` 下一步需要的基础面。必需项是 procfs/cgroup 可见性和 mock/noop runtime daemon smoke test；`ollama`、`llama.cpp`、`stress-ng`、`taskset` 只做可选工具盘点。该阶段不安装 Ollama、不拉取模型、不启动压力负载。
- `inference_tail_guard_ollama_smoke.sh`：运行正式 `ollama` A/B harness。默认安全三档是 `baseline`、`noop_observation`、`dry_run`；每档固定同一模型、prompt、并发和 CPU 干扰强度，输出 TTFT、P95/P99、jitter、trigger count、rollback count，并把原始样本和汇总追加到 `docs/verification_log.md`。
- `inference_tail_guard_phase4_report.sh`：阶段 4 多轮收益报告入口。它会循环跑 CPU 干扰和可选 CPU+I/O 扰动矩阵，汇总每轮 `summary.csv`，输出 `docs/mvp_benefit_report.md` 和 `.cache/aegisai/inference_tail_guard_phase4/<run_id>/` 下的对照 CSV。

## Tool Call Booster lifecycle harness

Phase 5 先使用 runtime daemon 内置 mock profile 固定 tool lifecycle 证据：

```bash
cargo run -p aegisai-runtime-daemon -- \
  --repo-root . \
  --source mock \
  --mock-profile tool-call-lifecycle \
  --metadata noop \
  --actuator-backend noop
```

该 profile 会回放 executor startup、retrieval queue/I/O、rerank queue 和
background noise 事件，并在 summary 中输出 `tool_call_lifecycles`。

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
- summary：`.cache/aegisai/inference_tail_guard/<run_id>/summary.csv`
- run controls：`.cache/aegisai/inference_tail_guard/<run_id>/run.env`
- Phase 4 report：`docs/mvp_benefit_report.md`
- Phase 4 aggregate：`.cache/aegisai/inference_tail_guard_phase4/<run_id>/phase4_aggregate.csv`

结果解释：

- `PASS` 代表选中的每个档位都完成固定样本数，并且 observation/guarded 档捕获 runtime events、触发 `inference_tail_guard`、完成 rollback，且没有 action audit errors。
- summary 中的 TTFT 来自 streaming request 的 `curl time_starttransfer`；P95/P99 和 jitter 来自 streaming request 的 total latency。
- `dry_run` 只预览 planned `renice/taskset` apply/rollback，不改系统状态。
- `live_guarded` 会实际执行并回滚系统命令，运行前要确认主机权限、PID allowlist 和实验窗口；cpuset 继续禁用。
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
