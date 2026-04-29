<!-- code-review-graph MCP tools -->
## MCP Tools: code-review-graph

**IMPORTANT: This project has a knowledge graph. ALWAYS use the
code-review-graph MCP tools BEFORE using Grep/Glob/Read to explore
the codebase.** The graph is faster, cheaper (fewer tokens), and gives
you structural context (callers, dependents, test coverage) that file
scanning cannot.

### When to use graph tools FIRST

- **Exploring code**: `semantic_search_nodes` or `query_graph` instead of Grep
- **Understanding impact**: `get_impact_radius` instead of manually tracing imports
- **Code review**: `detect_changes` + `get_review_context` instead of reading entire files
- **Finding relationships**: `query_graph` with callers_of/callees_of/imports_of/tests_for
- **Architecture questions**: `get_architecture_overview` + `list_communities`

Fall back to Grep/Glob/Read **only** when the graph doesn't cover what you need.

### Key Tools

| Tool | Use when |
|------|----------|
| `detect_changes` | Reviewing code changes — gives risk-scored analysis |
| `get_review_context` | Need source snippets for review — token-efficient |
| `get_impact_radius` | Understanding blast radius of a change |
| `get_affected_flows` | Finding which execution paths are impacted |
| `query_graph` | Tracing callers, callees, imports, tests, dependencies |
| `semantic_search_nodes` | Finding functions/classes by name or keyword |
| `get_architecture_overview` | Understanding high-level codebase structure |
| `refactor_tool` | Planning renames, finding dead code |

### Workflow

1. The graph auto-updates on file changes (via hooks).
2. Use `detect_changes` for code review.
3. Use `get_affected_flows` to understand impact.
4. Use `query_graph` pattern="tests_for" to check coverage.

## Multi-Agent Workflow

Use subagents only when the user explicitly asks for subagents, delegation, or parallel agent work.

Default starter roster for this project:

- `explorer`: explore code and constraints first, without implementation
- `pm`: break work down, define acceptance criteria, sequence, and scope boundaries
- `builder`: implement the approved plan with minimal diffs and run verification

Primary control role:

- The current main thread is the `main-brain` orchestrator
- `main-brain` is not a child agent role file; it is the primary chat that dispatches work
- `main-brain` owns task routing, artifact review, system-level consistency checks, and final correction passes

Default execution order:

1. `main-brain` defines the task frame, constraints, and dispatch order
2. `explorer` gathers context, files, risks, and open questions
3. `pm` turns that context into 3-6 sub-tasks and acceptance criteria
4. `builder` makes the smallest viable change set and verifies it
5. `main-brain` audits subagent outputs, fixes inconsistencies, and decides whether another pass is needed
6. If no dedicated `tester` or `reporter` exists, `main-brain` handles final verification and reporting

Working rules for subagent tasks:

- Explore before implementing
- Keep diffs surgical and scoped to the request
- List modified files and residual risks in the final report
- Verify with tests or explicit smoke checks before closing
- Escalate ambiguity instead of guessing
- Treat subagent output as draft input until `main-brain` reviews it

Main-brain responsibilities:

- Decide when to spawn subagents and in what order
- Refuse low-quality or conflicting subagent outputs
- Perform system-level review across all subagent artifacts
- Apply final optimization and error correction before presenting results
- Keep the final answer aligned with user scope, evidence, and verification

Prompt shape for delegated work:

1. Start with `请使用subagents完成这个任务`
2. State the goal
3. State the relevant context: directories, files, interfaces
4. State constraints: explore first, minimal changes, no fabricated results
5. State that the current main thread acts as `main-brain`
6. State the role flow: `main-brain -> explorer -> pm -> builder -> main-brain review`
7. Optionally extend to `tester` and `reporter` only when the task needs them
8. State done criteria: feature works, verification passes, changed files and risks are listed
