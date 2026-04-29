# ebpf_probe

`ebpf_probe` 是 AegisAI Runtime 在 `ebpf/` 层的共享基础模块。

它当前负责三件事：

- 定义 probe 到用户态的统一事件结构
- 定义目标过滤与低开销采集配置
- 给 `sched/offcpu/fault/io` 四类 probe 提供统一元数据描述

当前实现先聚焦“共享契约层”，不直接包含具体 eBPF 程序装载逻辑。这样可以先把：

- probe 输出格式
- 目标 pid/tid/cgroup 过滤接口
- 采样与 ring buffer 开销预算
- 第一轮 probe 注册表

固定下来，后续每个具体 probe 只需要补自己的内核程序与用户态装配代码。

## 当前公开能力

- `Event` / `EventTarget` / `EventMetric`
- `ProbeFilter`
- `ProbeConfig` / `OverheadBudget`
- `ProbeDescriptor`
- `ProbeRegistry`

## 后续建议对接

1. `sched_probe` 接入 `ProbeDescriptor::default_for(ProbeKind::Sched)`
2. 实际 eBPF 程序把 kernel event 规范化为 `Event`
3. `agent/collector` 直接消费统一事件流
