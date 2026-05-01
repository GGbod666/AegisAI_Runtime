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

- Use `context7` for current third-party library documentation when needed.
- Use `openspace` only when explicitly requested or when a task clearly calls for OpenSpace delegation.

### Working Rules

- Prefer small, reviewable, reversible diffs.
- Prefer existing patterns and utilities over new abstractions.
- Preserve unrelated configuration and local environment settings.
- When reviewing, focus first on bugs, regressions, risks, and missing tests.
