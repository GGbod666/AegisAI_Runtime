# runtime_orchestrator

`runtime_orchestrator` 是 AegisAI Runtime 用户态控制闭环的统一入口。

它负责把仓库中已经定义好的几个能力层串起来：

- `collector`：接收事件并维护时间窗口特征
- `classifier`：根据 runtime 规则和进程规则打标签
- `policy_engine`：按场景判断是否触发 bounded action
- `actuator`：以可审计、可回退的方式管理动作生命周期
- `metrics`：记录触发、回退和场景命中结果

当前实现特性：

- 复用 `aegisai-classifier` 统一完成 AI workload awareness 识别
- 直接加载仓库现有 `configs/` 下的样例 TOML
- 支持 `inference_tail_guard` 和 `tool_call_booster`
- 通过独立的 `aegisai-actuator` crate 管理动作租约、回退和审计轨迹
- 暴露 richer `WorkloadProfile`，包含 workload class / stage / latency sensitivity / scope

后续可以在这个 crate 基础上继续把真实 eBPF 事件流和系统调用执行器接进来。
