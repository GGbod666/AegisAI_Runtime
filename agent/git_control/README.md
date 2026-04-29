# git_control

`git_control` is the repository-state and checkpoint-planning helper for AegisAI Runtime development.

It is intentionally narrow in scope:

- discover the current git repository root
- snapshot branch / HEAD / ahead-behind / dirty state
- generate normalized checkpoint names for experiment and validation phases

Current responsibilities:

- `GitControl`: high-level entry for discovery and status snapshots
- `GitStatusSnapshot`: normalized repository state for tooling and experiment guards
- `GitCheckpointPlan`: stable checkpoint naming plan for future version checkpoints
- `SystemGitCommandRunner`: real `git` command runner
- `aegisai-git-control`: CLI for `status` and `checkpoint --label ...`

Example:

```powershell
cargo run -p aegisai-git-control -- status --path .
cargo run -p aegisai-git-control -- checkpoint --path . --label "linux vm baseline"
```

Current non-goals:

- automatic commits
- automatic branch switching
- rewriting history
- replacing normal git workflows

This module exists so later Linux VM validation and benchmark work can record clean version checkpoints without scattering ad-hoc git parsing across scripts and docs.
