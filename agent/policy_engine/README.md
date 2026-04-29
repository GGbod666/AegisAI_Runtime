# Policy Engine 模块规划

负责把观测特征和 workload 标签转换成场景决策。

## 设计原则

- 避免堆积硬编码 if-else
- 公共决策机制和场景规则分离
- 策略由条件、动作、冷却时间、安全限制组成
- 策略之间支持优先级和冲突处理

## 与场景目录的关系

- `agent/policy_engine` 负责决策框架
- `scenarios/` 负责定义具体问题场景的触发条件、动作集合和评价指标

## 第一版建议

- 先支持 `inference_tail_guard`
- 为 `tool_call_booster` 预留生命周期型策略接口
- 策略输出动作结构，如 `RaiseNice`、`SetAffinity`
