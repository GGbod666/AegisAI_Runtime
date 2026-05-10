# Next Stage Plan

_Updated: 2026-05-10_

## Current Conclusion

The repository is past the original runnable-control-loop and 19-task
evidence-hardening pass. The accepted mainline remains:

`collector -> classifier -> policy_engine -> actuator -> metrics`

Acceptance validation passed:

- `cargo fmt --all -- --check`
- `cargo test --workspace`
- `cargo clippy --all-targets --all-features -- -D warnings`
- Python report unit tests for `bench/tool_call_booster` and `bench/scripts`
- shell syntax checks for `bench/scripts/*.sh`
- `bench/scripts/verify_workspace.sh`, `toolchain_preflight.sh`, and
  `inference_tail_guard_preflight.sh` with `AEGISAI_VERIFY_LOG` redirected to
  `/tmp`

For compact status, see `docs/current_status.md`. For the accepted 19-task
ledger, see `docs/task_list.md`.

## Next Major Stage: Product Evidence

Primary objective:

Prove or falsify the remaining host-level benefit claims under controlled Linux
conditions without weakening the strict gates.

Current evidence:

- Inference Tail Guard: `docs/mvp_benefit_report.md` reports `FAIL` for
  `live_guarded_phase4_calibrated_20260510T043859Z`. Live guarded recorded `3`
  effective host-level actuator changes and mode contracts passed, but the run
  is classified as `noisy_workload` because stable repeated benefit was not
  proven.
- Tool Call Booster:
  `.cache/aegisai/tool_call_booster/live_guarded_tcb_issue_94s_final_20260510T053527Z/tool_call_booster_benefit_report.md`
  reports contract `PASS` and benefit `FAIL`; `live_guarded` improved `0/3`
  comparable rounds by at least `5.0%`.

## Required Work

### 1. Inference Tail Guard MVP Benefit

Beads issue: `AegisAI_Runtime-2kz`

Goal:

- continue from the current effective-action baseline
- reduce workload noise or produce a reproducible failure diagnosis
- keep PID allowlist, explicit live confirmation, and strict trend rules
  mandatory

Exit checks:

- `live_guarded` still records effective host-level action, or the report
  classifies `action_effectiveness` explicitly
- baseline, noop observation, dry-run, and live guarded modes remain comparable
- report gives `PASS` only when effective live action and stable repeated
  benefit are both present

### 2. Tool Call Booster Guarded Benefit

Beads issue: `AegisAI_Runtime-79d`

Goal:

- continue from the real executor lifecycle harness and live guarded artifact
- determine whether guarded scheduler actions can produce repeated tool-call
  latency benefit on this host
- preserve the current `WarmupExecutor` boundary unless a separate issue changes
  it

Exit checks:

- report includes latency deltas, trigger counts, rollback counts, action
  errors, and explicit PASS/FAIL verdict
- noop/dry-run remain control evidence, not host benefit proof
- guarded benefit PASS requires clean contracts plus repeated latency
  improvement versus baseline

### 3. Production Config Profiles

Beads issue: `AegisAI_Runtime-cqv`

Goal:

- replace production reliance on fixed `configs/*/*.example.toml` paths with an
  explicit profile selection and validation layer
- preserve the existing example-loader compatibility path for tests and local
  demos

Exit checks:

- profile names are validated as identifiers, not paths
- schema validation catches syntax, key/type, enum, range, and cross-file safety
  errors
- tests cover rejection paths and the local-development default

### 4. Helper Portability

Beads issue: `AegisAI_Runtime-51c`

Goal:

- validate helper-backed `offcpu_time` and `io_latency` across supported Linux
  kernels or add compatibility handling

Exit checks:

- tracepoint incompatibilities are reported with exact probe/field reasons
- helper conclusions continue to use `helper unavailable`,
  `tracepoint incompatible`, `no workload events`, or `validated signal`

### 5. Deferred Runtime Extensions

Beads issues:

- `AegisAI_Runtime-14r`: decide whether `WarmupExecutor` needs a real
  executor/cache warmup side effect.
- `AegisAI_Runtime-otk`: define live cpuset and background isolation safety
  boundaries before enabling host controls.
- `AegisAI_Runtime-ufp`: package the daemon and helper for production
  deployment.
- `AegisAI_Runtime-0ry`: plan dashboard, GPU, and adaptive policy extensions
  after the evidence-hardened MVP path is settled.

## Explicit Non-Goals For This Stage

- weakening the Inference Tail Guard or Tool Call Booster benefit gates
- treating noop or dry-run deltas as host-level benefit
- broad module decomposition without an active behavior issue
- enabling live cpuset writes by configuration alone
- claiming `WarmupExecutor` is live while it remains plan/audit-only

## Recommended Command Sequence

Safe reconfirmation:

```bash
AEGISAI_VERIFY_LOG=/tmp/aegisai_verify_workspace.md bash bench/scripts/verify_workspace.sh
AEGISAI_VERIFY_LOG=/tmp/aegisai_toolchain_preflight.md bash bench/scripts/toolchain_preflight.sh
AEGISAI_VERIFY_LOG=/tmp/aegisai_inference_preflight.md bash bench/scripts/inference_tail_guard_preflight.sh
```

Inference Tail Guard live benefit proof, only inside an approved experiment
window:

```bash
AEGISAI_AB_MODES=baseline,noop_observation,dry_run,live_guarded \
AEGISAI_CONFIRM_LIVE_ACTUATOR=1 \
AEGISAI_LIVE_PID_ALLOWLIST=<pid,...> \
  bash bench/scripts/inference_tail_guard_phase4_report.sh
```

Tool Call Booster guarded proof:

```bash
AEGISAI_TCB_MODES=baseline,noop_observation,dry_run,live_guarded \
AEGISAI_CONFIRM_LIVE_ACTUATOR=1 \
  bash bench/scripts/tool_call_booster_real_executor_harness.sh
```

## Stage Exit

This stage is complete only when the project can say one of the following with
evidence:

- MVP benefit proven: effective live guarded actions were observed and repeated
  tail-latency metrics passed the strict Phase 4 gate.
- MVP benefit not proven yet: the runtime closed loop works, but the live action
  or benefit trend still fails, with artifacts explaining why.

Either outcome is acceptable if the report is honest and reproducible.
