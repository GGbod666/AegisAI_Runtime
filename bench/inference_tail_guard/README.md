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
