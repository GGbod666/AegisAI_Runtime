# runtime_contracts

`runtime_contracts` 是 AegisAI Runtime 主干上的共享契约层。

它统一这些跨模块都会使用的核心对象：

- `ScenarioKind`
- `SignalKind`
- `Event`
- `FeatureWindow`
- `WorkloadProfile`
- `PolicyContext`
- `ActionPlan`
- `AppliedAction`
- `SafetyConfig`
- `ScenarioPolicy`

这一层的目标是：

- 防止 `runtime_orchestrator / policy_engine / actuator` 各自生长私有模型
- 让场景策略和执行链路围绕同一份 domain model 演进
- 把后续系统级重构的成本从“改多份转换代码”降到“改一份共享契约”
