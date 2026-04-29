# 干扰场景规划

用于统一定义 benchmark 时施加的背景干扰。

## 第一轮建议

- `stress-ng`：CPU 干扰
- `fio`：I/O 干扰
- 后台批处理任务：调度噪声
- 工具链 worker storm：tool call 链路噪声
