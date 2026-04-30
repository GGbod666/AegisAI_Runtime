# scripts

用于放置实验驱动脚本、标签回放脚本和结果整理脚本。

## 当前脚本

- `verify_workspace.sh`：运行当前工作区验证，并把命令、退出码和关键输出追加到 `docs/verification_log.md`。
- `toolchain_preflight.sh`：盘点 pre-Ollama 阶段需要的开发、eBPF 和 demo 工具；不执行安装，只记录缺失项和建议安装命令。必需工具缺失会使脚本失败；可选工具缺失只作为 inventory 记录。
- `inference_tail_guard_preflight.sh`：检查 Linux VM/demo 是否具备 `Inference Tail Guard` 下一步需要的基础面。必需项是 procfs/cgroup 可见性和 mock/noop runtime daemon smoke test；`ollama`、`llama.cpp`、`stress-ng`、`taskset` 只做可选工具盘点。该阶段不安装 Ollama、不拉取模型、不启动压力负载。
- `inference_tail_guard_ollama_smoke.sh`：运行第一轮真实 `ollama` smoke。它会预热一个本地模型，启动 `stress-ng`（若可用），并让 `aegisai-runtime-daemon` 用 `linux/procfs` 路径观察真实推理请求，再把请求结果和 daemon 观测结果统一追加到 `docs/verification_log.md`。默认后端为 `noop`，先验证策略观测链路；也支持 `linux-command-dry-run`，把将要执行的 `renice/taskset` 审计细节记录出来，但不真正修改系统状态。

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
可用 `AEGISAI_OLLAMA_MODEL=qwen2.5:0.5b`、`AEGISAI_DAEMON_BACKEND=noop`、`AEGISAI_STRESS_CPU=2` 覆盖默认实验参数。
推荐把下一步安全实验跑成：

```bash
AEGISAI_DAEMON_BACKEND=linux-command-dry-run \
  bash bench/scripts/inference_tail_guard_ollama_smoke.sh
```

这样日志里会出现 dry-run audit highlights，方便确认真实 `ollama` 进程在当前策略下本来会执行哪些 `renice/taskset` 命令。

推荐顺序：

```bash
bash bench/scripts/toolchain_preflight.sh
# 按日志中的 required-tool install 建议安装缺失的必需工具后，再重跑：
bash bench/scripts/toolchain_preflight.sh
bash bench/scripts/inference_tail_guard_preflight.sh
bash bench/scripts/inference_tail_guard_ollama_smoke.sh
bash bench/scripts/verify_workspace.sh
```
