# eBPF 观测层规划

这一层负责低开销观测 AI runtime 关键路径上的系统干扰信号。

## 第一轮 probe

- `ebpf_probe`（共享事件模型与 probe 契约）
- `sched_probe`
- `offcpu_probe`
- `fault_probe`
- `io_probe`

## 为什么这些 probe 足够支撑第一轮

- `sched_probe` 解决排队等待和调度抖动
- `offcpu_probe` 解决关键线程被挂起的问题
- `fault_probe` 解决内存抖动和冷页问题
- `io_probe` 为后续 tool call 与 I/O 干扰验证预留能力

## 设计原则

- 只采高价值信号
- 目标进程 / cgroup 范围内观测
- 输出统一事件结构
- 控制观测开销
- 主控制面保持普通权限运行
- root 或 eBPF capability 只允许落在 `aegisai-ebpf-helper` 这类窄权限组件上
