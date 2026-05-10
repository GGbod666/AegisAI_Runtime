# 配置层规划

配置改为按职责拆分：

- `runtime/`：运行环境与目标 runtime
- `classifier/`：process / stage 识别规则
- `scenarios/`：场景级策略
- `safety/`：全局安全限制

生产 profile 的选择、schema 校验和暂缓范围见 `docs/engineering_debt_boundaries.md`。
