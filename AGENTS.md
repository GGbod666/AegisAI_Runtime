## Project Guidance

Keep changes focused on the user's request. Do not revert unrelated user changes. Verify with the smallest relevant checks before claiming completion.

### MCP Tools: code-review-graph

Use the `code-review-graph` MCP tools before grep/read when exploring this repo.

- `semantic_search_nodes` or `query_graph` for finding code.
- `get_impact_radius` for blast radius.
- `detect_changes` and `get_review_context` for code review.
- Fall back to shell search only when the graph is not enough.

### MCP Services

Available MCP services should follow the user home configuration first. Project-local `.codex/config.toml` may mirror or supplement MCP server entries, but it should not override global model, provider, context window, or storage settings.

- Keep `code-review-graph` as the first stop before grep/read when exploring repository code. Use shell search only after graph tools do not provide enough context.
- Use `sequential-thinking` for complex or ambiguous multi-step reasoning, root-cause analysis, plan revision, cross-MCP coordination, or when the next step is not obvious. Keep it evidence-driven; do not use it as a substitute for inspecting code or running verification.
- Use `context7` before relying on memory or general web search for current third-party library, framework, SDK, or API documentation. Use it after code exploration when a dependency-specific behavior, version, or example matters.
- Use `openspace` when the user explicitly asks for OpenSpace, delegation, skill discovery, or autonomous execution, and when a bounded side task clearly benefits from OpenSpace's skills or tooling. The main Codex thread remains responsible for scoping, reviewing results, and final verification.

Coordinate MCP services actively:

1. For repository work, start with `code-review-graph` to locate code, dependencies, blast radius, or review context.
2. If the task needs careful reasoning or coordination, use `sequential-thinking` to structure the next steps.
3. If external library behavior matters, use `context7` for current docs before making or explaining changes.
4. If delegation or skill-guided execution is useful, use `openspace` with a narrow task and review its output before acting on it.
5. Do not call every MCP service mechanically, but do not leave a relevant service unused when its trigger condition is met.

### Working Rules

- Prefer small, reviewable, reversible diffs.
- Prefer existing patterns and utilities over new abstractions.
- Preserve unrelated configuration and local environment settings.
- When reviewing, focus first on bugs, regressions, risks, and missing tests.

<!-- BEGIN BEADS INTEGRATION v:1 profile:minimal hash:ca08a54f -->
## Beads Issue Tracker

This project uses **bd (beads)** for issue tracking. Run `bd prime` to see full workflow context and commands.

### Quick Reference

```bash
bd ready              # Find available work
bd show <id>          # View issue details
bd update <id> --claim  # Claim work
bd close <id>         # Complete work
```

### Rules

- Use `bd` for ALL task tracking — do NOT use TodoWrite, TaskCreate, or markdown TODO lists
- Run `bd prime` for detailed command reference and session close protocol
- Use `bd remember` for persistent knowledge — do NOT use MEMORY.md files

## Session Completion

**When ending a work session**, you MUST complete ALL steps below. Work is NOT complete until `git push` succeeds.

**MANDATORY WORKFLOW:**

1. **File issues for remaining work** - Create issues for anything that needs follow-up
2. **Run quality gates** (if code changed) - Tests, linters, builds
3. **Update issue status** - Close finished work, update in-progress items
4. **PUSH TO REMOTE** - This is MANDATORY:
   ```bash
   git pull --rebase
   bd dolt push
   git push
   git status  # MUST show "up to date with origin"
   ```
5. **Clean up** - Clear stashes, prune remote branches
6. **Verify** - All changes committed AND pushed
7. **Hand off** - Provide context for next session

**CRITICAL RULES:**
- Work is NOT complete until `git push` succeeds
- NEVER stop before pushing - that leaves work stranded locally
- NEVER say "ready to push when you are" - YOU must push
- If push fails, resolve and retry until it succeeds
<!-- END BEADS INTEGRATION -->
Use 'bd' for task tracking
