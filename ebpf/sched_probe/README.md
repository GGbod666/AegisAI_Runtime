# sched_probe

负责采集与调度行为直接相关的高价值信号。

首批关注：

- run queue delay
- CPU migration
- context switch

输出目标：

- 为 `inference_tail_guard` 提供核心触发依据
- 与 `collector` 对接成统一事件流
