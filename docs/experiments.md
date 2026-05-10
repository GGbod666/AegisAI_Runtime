# 实验与验证设计

## 1. 实验目标

实验不是证明“系统很复杂”，而是证明：

- AI workload 能被稳定识别
- 系统级动态干预对用户体验相关指标有正收益

## 2. 核心验证方法

所有实验建议使用 A/B 对照：

- A：无优化
- B：启用策略

当前报告约定更细：

- `baseline`：无 daemon 干预或未观测的同轮参考
- `noop_observation`：只证明识别、触发和生命周期观测
- `dry_run`：证明命令规划、审计和 rollback 闭环
- `live_guarded`：在显式确认和 PID allowlist 下执行真实受限动作

要求尽量保持：

- 同机器
- 同 runtime
- 同模型
- 同并发设置
- 同干扰强度

## 3. 验证矩阵

### 3.1 Awareness 验证

关注：

- 是否正确识别目标 AI runtime
- 是否能区分 background job
- 是否能输出稳定标签

### 3.2 Tail Guard 验证

关注：

- TTFT
- P95 latency
- P99 latency
- jitter / variance

### 3.3 Tool Call 验证

关注：

- tool call end-to-end latency
- executor 启动时延
- retrieval / rerank 子链路耗时

## 4. 系统侧指标

- run queue delay
- off-CPU time
- CPU migration
- page fault
- block I/O latency

## 5. 策略侧指标

- boost trigger 次数
- boost 持续时间
- rollback 次数
- 干预后副作用

## 6. MVP 推荐实验

### 实验 A：AI 推理 + CPU 干扰

配置建议：

- runtime：`ollama` 或 `llama.cpp`
- 干扰：`stress-ng`

### 实验 B：AI 推理 + I/O 扰动

配置建议：

- runtime 固定
- 背景运行读写密集任务

### 实验 C：规则识别回放

配置建议：

- 固定目标 runtime 进程样本
- 固定 background worker 样本
- 对 classifier 规则做覆盖验证

## 7. 实验记录建议

每次实验至少记录：

- runtime 名称
- 模型名称
- 干扰类型
- 干扰强度
- 场景名称
- 策略是否开启
- 指标结果
- 结论摘要

## 8. 结果判断

即使平均吞吐提升不明显，只要以下条件成立，项目依然有价值：

- P95/P99 明显下降
- TTFT 更稳定
- 响应波动减小
- 标签能稳定支撑策略路由

尾延迟和稳定性优先于平均吞吐。

但 MVP benefit 不能只靠价值判断声明。当前 strict gate 要求：

- live guarded 发生 effective host-level actuator change
- 至少三轮可比较结果
- 至少三分之二可比较轮次改善
- 平均改善达到报告配置阈值
- noop/dry-run 改善不得计入 host-level benefit
