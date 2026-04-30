# Inference Tail Guard Benchmark

用于验证 AI 推理尾延迟保护是否成立。

## 下一步 smoke/preflight

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

## 第一轮建议

- 固定 runtime：`ollama` 或 `llama.cpp`
- 固定 prompt 集
- 引入 CPU / I/O 干扰
- 记录 TTFT、P95/P99 latency、jitter

## 后续 harness 形状

预检通过后，再把该入口扩展成真实 demo wrapper：

- baseline：只运行目标 inference runtime 和固定 prompt 集
- interference：用 `stress-ng` 加入可控 CPU 压力
- guard：启用 `AI Workload Awareness -> Inference Tail Guard` 控制路径
- output：记录 TTFT、P95/P99 latency、jitter、rollback count，并继续追加到 verification log

当前仓库已经补了一条最小真实 runtime smoke 路径：

```bash
bash bench/scripts/inference_tail_guard_ollama_smoke.sh
```

运行前请先确认：

- 本地 `ollama serve` 已启动，且默认 API 地址 `http://127.0.0.1:11434` 可达
- 目标模型已经在本机准备好，至少能通过 `ollama show <model>` 成功返回
- `cargo`、`curl` 在 PATH 中

`bench/scripts/inference_tail_guard_preflight.sh` 仍然是推荐前置，但不是这个 smoke 脚本的内建 gate。

这个脚本默认选择本地 `ollama` 模型并走 `noop` 观测后端，目的不是直接做最终 A/B 结论，而是先确认下面这条真实链路能稳定工作：

- 本地模型可被真实请求唤起
- `stress-ng` 干扰可以受控叠加
- `aegisai-runtime-daemon --source linux --metadata procfs` 能捕获真实 runtime 事件
- `inference_tail_guard` 是否在真实模型请求期间被触发

这条 smoke 当前证明的是“真实请求 + daemon 观测链路”是否能跑通，不证明：

- 当前策略已经执行真实调度干预
- 没有 runtime 事件时就可以判定 harness 无效
- 单次 `PASS` 已经足够支撑完整 benchmark 或最终 A/B 结论

日志解读时要额外关注：

- `processed_events`
- `Observed inference_tail_guard trigger count`
- `Interpretation`

如果日志里出现 “request succeeded but no runtime events were captured”，当前语义只是把它记录为观察结果，不会单独把本次 smoke 判成失败。因此 `PASS` 不能直接解读为“已经成功观测到 runtime 事件”。

当 `noop` smoke 稳定后，推荐先跑一轮 dry-run：

```bash
AEGISAI_DAEMON_BACKEND=linux-command-dry-run \
  bash bench/scripts/inference_tail_guard_ollama_smoke.sh
```

这一轮不会真正改目标进程的 nice/affinity，但会把计划中的 `renice/taskset` 命令审计摘要写进日志。确认 dry-run 结果合理之后，再继续推进到更高风险的真实 `linux-command` 后端或完整 A/B 对照。
