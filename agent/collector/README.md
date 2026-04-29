# collector

负责把来自 eBPF 层的原始事件聚合成窗口化指标。

## 当前实现

- Rust crate：`agent/collector`
- 输入：统一 `Event` 流
- 输出：固定时间窗口的 `FeatureWindow`
- 聚合维度：`thread` / `process` / `cgroup`
- 指标摘要：`run queue delay`、`off-CPU`、`I/O latency`、`CPU migrations`、`major page faults`

## 设计取向

- 简单、稳定、可验证
- 默认 500ms 窗口，对齐 `inference_tail_guard` 配置
- 支持基础 noise filter 和 late event 丢弃
- 不追求复杂时序分析

## 对外接口

- `Collector::ingest(event)`：写入事件，并返回因 watermark 推进而关闭的窗口
- `Collector::flush_until(ts)`：手动推进 watermark 并输出已关闭窗口
- `Collector::finish()`：在退出时强制输出所有未关闭窗口

当前实现是给 `classifier` 和 `policy_engine` 提供稳定输入的最小版本，后续可以继续补充配置解析、采样策略和更细的摘要统计。
