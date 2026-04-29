# Subagents Workflow

## Overview

Use this project workflow when the user explicitly asks to use subagents. Keep the starter setup small: `explorer`, `pm`, and `builder`, and treat the current main thread as the `main-brain` orchestrator.

## Default Flow

Starter flow:

1. `main-brain` frames the task, constraints, and dispatch order
2. `explorer` collects context, files, constraints, and risks
3. `pm` turns that context into sub-tasks and acceptance criteria
4. `builder` implements the smallest viable change and verifies it
5. `main-brain` reviews the outputs, fixes mistakes, and decides whether to loop again

Expanded flow when needed:

`main-brain -> explorer -> pm -> builder -> main-brain review -> tester -> reporter`

If only the starter roster exists, let `builder` cover basic verification and let `main-brain` produce the final report.

## Role Intent

- `main-brain`: dispatch tasks, audit artifacts, optimize corrections, own final output
- `explorer`: read first, map the terrain, avoid implementation
- `pm`: scope the work, define done, prevent sprawl
- `builder`: implement surgically, verify, summarize residual risk

## Prompt Template

Start with:

`请使用subagents完成这个任务`

Then provide:

1. Goal
2. Context
3. Constraints
4. Statement that the current main thread is `main-brain`
5. Role flow
6. Done criteria

Example:

```text
请使用subagents完成这个任务

目标：
新增登录接口

上下文：
- 目录：agent/runtime_daemon、agent/runtime_orchestrator
- 重点文件：src/main.rs、src/lib.rs

约束：
- 先探索再实施
- 保持最小改动
- 不要编造验证结果

主脑中枢：
- 当前主聊天负责派发任务、审核子智能体产物、做系统性修正

分工：
main-brain -> explorer -> pm -> builder -> main-brain review

如有必要再追加：
tester -> reporter

完成标准：
- 功能可用
- 测试通过
- 列出修改文件与剩余风险
```

## Rules

- 长期协作规则写进 `AGENTS.md`
- 角色定义写进 `.codex/agents/`
- 主脑中枢职责固定由当前主线程承担，不额外建成第 4 个子智能体
- 重复任务沉淀成正式 skill
