# scripts

用于放置实验驱动脚本、标签回放脚本和结果整理脚本。

## 当前脚本

- `verify_workspace.sh`：运行当前工作区验证，并把命令、退出码和关键输出追加到 `docs/verification_log.md`。
- `toolchain_preflight.sh`：盘点 pre-Ollama 阶段需要的开发、eBPF 和 demo 工具；不执行安装，只记录缺失项和建议安装命令。必需工具缺失会使脚本失败；可选工具缺失只作为 inventory 记录。
- `inference_tail_guard_preflight.sh`：检查 Linux VM/demo 是否具备 `Inference Tail Guard` 下一步需要的基础面。必需项是 procfs/cgroup 可见性和 mock/noop runtime daemon smoke test；`ollama`、`llama.cpp`、`stress-ng`、`taskset` 只做可选工具盘点。该阶段不安装 Ollama、不拉取模型、不启动压力负载。
- `inference_tail_guard_ollama_smoke.sh`：运行第一轮真实 `ollama` smoke。它会预热一个本地模型，启动 `stress-ng`（若可用），并让 `aegisai-runtime-daemon` 用 `linux/procfs` 路径观察真实推理请求，再把请求结果和 daemon 观测结果统一追加到 `docs/verification_log.md`。默认后端为 `noop`，先验证策略观测链路；也支持 `linux-command-dry-run`，把将要执行的 `renice/taskset` 审计细节记录出来，但不真正修改系统状态。

## 真实 Ollama smoke 前置条件

- 必需命令：`ollama`、`cargo`、`curl`
- 必需环境：本地 `ollama serve` 已启动，且 `AEGISAI_OLLAMA_API_URL` 指向的地址可达
- 必需模型：目标模型已经在本机可用，至少能通过 `ollama show <model>` 成功返回
- 可选工具：`stress-ng`

`inference_tail_guard_preflight.sh` 是强烈推荐的前置步骤，但不是 `inference_tail_guard_ollama_smoke.sh` 的内建 hard gate。真实 smoke 会直接尝试请求本地 Ollama 服务；如果服务未启动、模型未准备好，脚本会把失败写进 `docs/verification_log.md` 并退出非零。

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
推荐先跑默认 `noop` 路径：

```bash
AEGISAI_DAEMON_BACKEND=noop \
  bash bench/scripts/inference_tail_guard_ollama_smoke.sh
```

这一轮只验证“真实请求 + daemon 观测链路”能否跑通，不代表已经执行真实调度干预。

推荐把下一步安全实验跑成：

```bash
AEGISAI_DAEMON_BACKEND=linux-command-dry-run \
  bash bench/scripts/inference_tail_guard_ollama_smoke.sh
```

这样日志里会出现 dry-run audit highlights，方便确认真实 `ollama` 进程在当前策略下本来会执行哪些 `renice/taskset` 命令。

结果解释：

- `PASS` 代表必需命令、模型检查、warmup 请求、monitored 请求和 daemon 运行都没有返回非零。
- `PASS` 不自动等于“已经观测到 runtime 事件”或“已经触发 `inference_tail_guard`”；仍需查看日志里的 `processed_events` 和 trigger count。
- “请求成功但没有 runtime events were captured” 目前只作为观察结果记录，不会单独把本次 smoke 判成失败。
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
