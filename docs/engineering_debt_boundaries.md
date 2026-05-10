# Engineering Debt Boundaries

_Last reviewed: 2026-05-10_

This note records the boundaries for two P3 debt items. It is intentionally a
design/process note: it does not change runtime behavior.

## Production Config Profile Boundaries

Current state:

- `RuntimeOrchestratorConfig::load_from_repo_root` reads fixed files under
  `configs/*/*.example.toml` plus `configs/safety/default.toml`.
- The orchestrator parser accepts a small TOML subset for the runtime,
  scenario, and safety files. Classifier process rules already go through the
  classifier crate parser.
- The checked-in example files are suitable for tests, demos, and benchmark
  harnesses. They are not a production profile contract.

Profile selection boundary:

- Production startup should select one named profile explicitly before reading
  component config files.
- Selection precedence should be deterministic: CLI flag, then environment
  variable, then a documented non-production default for local development.
- Profile names should be identifiers, not paths. Accept lowercase letters,
  digits, `_`, and `-`; reject path separators, `.` segments, empty names, and
  absolute paths.
- A selected profile should resolve to a single config root and one complete
  set of component files. Mixing runtime, classifier, scenario, or safety files
  from different profile roots is outside the default contract.
- Production mode should not silently load `*.example.toml`. Example files can
  remain the compatibility default for tests and local demos until the loader is
  implemented.

Schema validation boundary:

- The schema source of truth should be the Rust typed config structs consumed by
  the orchestrator and downstream crates.
- A production loader should validate in stages: TOML syntax, schema keys and
  types, required fields, enum values, numeric ranges, cross-file consistency,
  then host/environment readiness.
- Unknown keys should be rejected for production profiles unless a field is
  explicitly documented as extensible metadata.
- Cross-file validation should catch mismatches such as inactive scenarios with
  scenario-specific trigger blocks, focus signals that no source can emit, and
  action durations that exceed global safety limits.
- Error messages should name the profile, file, section, key, and violated
  constraint. Missing files should be reported before partial startup.

Implementation shape when this becomes active work:

- Add a small profile descriptor around the existing config loading entrypoint
  instead of changing every downstream config consumer at once.
- Keep the current fixed-example loader as a test/demo compatibility path until
  profile tests cover equivalent behavior.
- Add targeted tests for profile-name rejection, missing files, unknown keys,
  cross-file safety violations, and the local-development default.

Intentionally deferred:

- Hot reload and dynamic profile switching.
- Remote config distribution.
- Secret storage and secret interpolation.
- Config migrations across schema versions.
- Dashboard or UI editing of profiles.
- Per-tenant profile inheritance.
- Online adaptive policy writes back into profile files.
- Enabling live cpuset writes by profile alone.

## Hotspot Refactor Boundaries

The following files are known hotspots:

- `agent/runtime_daemon/src/source.rs`
- `agent/actuator/src/backend.rs`
- `agent/explain_tune/src/engine.rs`
- `agent/runtime_orchestrator/src/runtime_orchestrator.rs`
- `agent/policy_engine/src/engine.rs`
- `bench/scripts/inference_tail_guard_ollama_smoke.sh`

These files should not be split as standalone cleanup. A split is acceptable
only when it is attached to active behavior work that already needs to touch the
hotspot, such as live affinity planning, rollback hardening, Linux source edge
tests, benefit report interpretation, or explain/tune policy changes.

Required boundaries for any hotspot split:

- Keep the public behavior and CLI/script outputs unchanged unless the active
  issue explicitly requires a behavior change.
- Extract one cohesive concern at a time. Do not combine splitting with broad
  renames, style cleanup, or unrelated module moves.
- Add or preserve targeted tests before claiming the split is behavior
  preserving.
- Run the smallest relevant verification: crate tests for Rust module splits,
  `bash -n` plus focused script tests for shell splits, and the existing
  workspace gate only when the blast radius justifies it.
- Record the active `bd` issue that justified the split.

Current disposition:

- No P3-only refactor is planned.
- `agent/actuator/src/backend.rs` already had attached work through
  `AegisAI_Runtime-v2y`; CPU affinity planning now lives in
  `agent/actuator/src/cpu_affinity.rs` and is covered by focused tests.
- `agent/runtime_daemon/src/source.rs` is best handled through the P2 Linux
  source/procfs edge-test pattern; edge tests are in place, but future source
  behavior changes should remain issue-led.
- `bench/scripts/inference_tail_guard_ollama_smoke.sh` should be split only
  when a benefit-proof or report-interpretation issue needs a script change.
