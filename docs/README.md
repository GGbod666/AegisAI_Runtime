# Documentation Map

This directory keeps durable project context and generated evidence. `bd` is the
source of truth for active task state; docs should explain context, boundaries,
and artifact interpretation.

## File Boundaries

| File | Responsibility | Do not put here |
| --- | --- | --- |
| `status.md` | Current factual snapshot, latest artifact index, open gap index, restart context. | Long task ledgers, historical plans, experiment procedures. |
| `acceptance_ledger.md` | Accepted 19-task evidence-hardening ledger and acceptance gate record. | Active task tracking; use `bd` for that. |
| `strategy.md` | MVP definition, strict benefit rules, current product-evidence stage, roadmap, experiment method. | Current artifact tables or command transcripts. |
| `architecture.md` | Durable system architecture, deployment boundaries, production-config and hotspot-refactor debt boundaries. | Current status or active work queues. |
| `linux_validation.md` | Linux host setup, preflight, helper validation, live guarded experiment checklist. | Product strategy or acceptance history. |
| `mvp_benefit_report.md` | Generated/latest Phase 4 Inference Tail Guard benefit report. | Manual status edits not backed by artifacts. |
| `resume_pitch.md` | External resume/interview wording only. | Engineering status, tasks, or evidence. |
| `verification_log.md` | Append-only verification history written by scripts. | Manual edits. |

## Reading Order

1. `status.md`
2. `strategy.md`
3. `mvp_benefit_report.md`
4. `acceptance_ledger.md` when acceptance history matters
5. `linux_validation.md` before Linux host or live guarded work
6. Latest entries in `verification_log.md` only when raw evidence detail is
   needed

## Maintenance Rules

- Put active execution tasks in `bd`, not docs.
- Keep generated/run-specific evidence in `mvp_benefit_report.md`, cache
  artifacts, or `verification_log.md`.
- Keep `verification_log.md` append-only; redirect `AEGISAI_VERIFY_LOG` to
  `/tmp/...` for checks that should not append to the repository log.
