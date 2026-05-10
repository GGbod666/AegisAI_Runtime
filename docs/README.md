# Docs Map

This directory keeps project documentation, experiment evidence, and session
handoff notes. `bd` remains the source of truth for task state; docs should
explain context and link to issue IDs instead of becoming a second tracker.

## File Boundaries

| File | Owner Role | Boundary |
| --- | --- | --- |
| `current_status.md` | Current factual status | Short snapshot of what works, what is partial, and what is not proven. Do not duplicate the detailed task list here. |
| `task_list.md` | Acceptance ledger and gap index | Accepted 19-task ledger plus pointers to open `bd` gaps. Do not use it as a second task tracker. |
| `next_stage.md` | Stage strategy | Product-evidence objective, stage gates, and command sequence. Keep active task state in `bd`. |
| `mvp.md` | MVP definition | What counts as MVP proof and what does not. Keep experiment results in `mvp_benefit_report.md`. |
| `mvp_benefit_report.md` | Latest benefit report | Generated or artifact-backed report for the latest Phase 4 benefit run. |
| `architecture.md` | Stable architecture | Durable module/layer design and deployment boundaries. Avoid current task lists. |
| `roadmap.md` | Long-horizon phases | Phase sequence and future expansion. Avoid active-task detail. |
| `experiments.md` | Experiment method | General A/B and metric design guidance. Avoid current status. |
| `linux_vm_checklist.md` | Linux host checklist | Operator checklist for Linux validation host setup and safe live experiments. |
| `host_strategy.md` | Host split reference | Historical/current rationale for dev host vs Linux validation host. |
| `modular_execution_plan.md` | Historical implementation plan | Early staged execution plan. Treat as reference; current work lives in `next_stage.md` and `bd`. |
| `handoff.md` | Session restart | Minimal restart context and commands. Avoid duplicating full status or task detail. |
| `resume_pitch.md` | External pitch | Resume/interview wording only. Not a status or task document. |
| `verification_log.md` | Append-only log | Do not edit manually. Scripts append verification entries here. |

## Current Reading Order

1. `current_status.md`
2. `task_list.md`
3. `next_stage.md`
4. `mvp_benefit_report.md`
5. Latest entries in `verification_log.md` when evidence detail is needed

## Maintenance Rules

- Put new execution tasks in `bd` first.
- Keep active task state in `bd`; use `task_list.md` for acceptance status and
  durable gap pointers.
- Keep generated or run-specific evidence in `mvp_benefit_report.md` or
  artifacts referenced from it.
- Never rewrite `verification_log.md`; it is append-only history.
