# Classifier

负责把系统中的 process / thread / cgroup 映射成 AI runtime 语义标签。

当前目录已经落成一个最小 Rust crate：

- `Cargo.toml`：crate 定义
- `src/model.rs`：输入对象、规则模型、画像输出
- `src/config.rs`：第一版规则配置加载
- `src/classifier.rs`：规则匹配与 profile 生成
- `src/lib.rs`：对外导出与单元测试

## 当前已支持的识别条件

- 进程名精确匹配
- 命令行子串匹配
- PID allowlist
- cgroup 路径子串匹配
- 外部 tag marker 匹配
- 父子进程关系补充识别
- 父进程标签补充识别

另外，classifier 现在支持通过开关按场景禁用：

- cmdline 规则
- cgroup 规则
- PID allowlist 规则
- parent-child inference 规则

## 输出画像

`WorkloadProfile` 当前包含：

- `workload_class`
- `stage`
- `latency_sensitivity`
- `scope`
- `tags`
- `matched_rules`

## 支持的规则字段

`configs/classifier/process_rules.example.toml` 当前支持这些字段：

- `id`
- `name` / `process_name`
- `cmdline_contains`
- `cgroup_contains`
- `pids` / `pid_allowlist`
- `tag_markers`
- `parent_name` / `parent_process_name`
- `parent_cmdline_contains`
- `parent_has_any_tags`
- `tags`

## 输出标签

- `AI_INFERENCE`
- `TOOL_CALL`
- `RETRIEVAL_STAGE`
- `RERANK_STAGE`
- `BACKGROUND_JOB`
- `INTERACTIVE_LATENCY_SENSITIVE`
