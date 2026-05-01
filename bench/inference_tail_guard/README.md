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
- 输出：`TTFT p50/p95/p99`、latency `P95/P99`、jitter、trigger count、rollback count

运行前请先确认：

- 本地 `ollama serve` 已启动，且默认 API 地址 `http://127.0.0.1:11434` 可达
- 目标模型已经在本机准备好，至少能通过 `ollama show <model>` 成功返回
- `cargo`、`curl`、`python3`、`stress-ng` 在 PATH 中
- `live_guarded` 会调用真实 `renice`，需要当前主机权限和实验窗口允许
- `live_guarded` 必须设置 `AEGISAI_CONFIRM_LIVE_ACTUATOR=1` 和 `AEGISAI_LIVE_PID_ALLOWLIST=<pid,...>`；默认 scope 是 nice-only
- 等 nice-only 稳定后，再设置 `AEGISAI_ENABLE_LIVE_AFFINITY=1` 推进 `taskset`；`cpuset` 继续禁用

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

退出条件：

- 每档样本都必须完成，HTTP 200，并收到 Ollama streaming `done=true`
- 所有档位使用同一模型、prompt、样本数、并发和 CPU 干扰强度
- `noop_observation`、`dry_run`、`live_guarded` 必须捕获 daemon events、触发 `inference_tail_guard`，并产生 rollback
- `dry_run` 和 `live_guarded` 不能出现 apply/rollback 审计错误
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

阶段 4 成功条件比单次 harness 更严格：只有当 `live_guarded` 的 TTFT P95/P99、latency P95/P99 或 jitter 在至少三分之二可比较轮次里相对 baseline 改善，且平均改善不低于 5%，才算看到稳定改善趋势。`noop_observation` 与 `dry_run` 可以证明识别、触发、审计和 rollback 闭环；真实收益仍需要 live guarded actuator 在当前主机权限下成功执行并出现同样趋势。如果 live `renice` 被权限拒绝，报告必须标记为收益未证明，而不是把 dry-run 结果当成收益。
