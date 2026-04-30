# Handoff

## Current State

As of `2026-04-29`, the project is no longer at the pre-Ollama gate.

The current Linux-side baseline is:

- the shared runtime mainline is wired:
  `collector -> classifier -> policy_engine -> actuator -> metrics`
- `runtime_daemon` runs with:
  - `mock` source for deterministic local validation
  - `linux` source backed by the procfs schedstat driver for real `run_queue_delay` observation
  - `procfs` metadata enrichment for process name, cmdline, cgroup, and parent fields
- `inference_tail_guard` has been observed on a real `ollama` request using the local `qwen2.5:0.5b` model
- the bench layer now has:
  - pre-Ollama readiness gate
  - first real `ollama` smoke harness
  - append-only verification logging in `docs/verification_log.md`
- a safe command-path preview mode now exists:
  `linux-command-dry-run`

## What Is Verified

### 1. Preflight gate

The project already has a safe pre-Ollama gate:

```bash
bash bench/scripts/inference_tail_guard_preflight.sh
```

This confirms:

- procfs / cgroup / cpuset visibility
- mock/noop daemon path health
- optional tool inventory for `ollama`, `llama.cpp`, `stress-ng`, and `taskset`

### 2. Real runtime observation

The current real-runtime smoke entrypoint is:

```bash
bash bench/scripts/inference_tail_guard_ollama_smoke.sh
```

Default behavior:

- target runtime: `ollama`
- default model: `qwen2.5:0.5b`
- optional interference: `stress-ng --cpu 2 --timeout 12s`
- observation backend: `noop`

Most recent strong signal:

- verification log entry:
  `2026-04-29T13:51:10+00:00 - Inference Tail Guard Ollama smoke`
- result:
  - `processed_events: 6`
  - `inference_tail_guard: 5`
  - `Overall result: PASS`

This proves:

- a real `ollama` request can be observed through the Linux procfs source
- the classifier/runtime selection matches the real target process
- the policy can trigger on a real model request under controlled CPU pressure

### 3. Safe command-path preview

The daemon now supports:

- `noop`
- `linux-skeleton`
- `linux-command`
- `linux-command-dry-run`

`linux-command-dry-run` is the next safe checkpoint because it:

- captures real process state from `/proc`
- computes the same bounded actions as the real command backend
- records dry-run `renice` / `taskset` audit details
- does **not** apply those commands to the live process

Latest dry-run checkpoint:

- verification log entry:
  `2026-04-29T14:18:59+00:00 - Inference Tail Guard Ollama smoke`
- result:
  - `actuator_backend: linux-command-dry-run`
  - `inference_tail_guard: 1`
  - dry-run audit highlights were emitted for both apply and rollback paths
  - cpuset rollback noise is no longer emitted when policy keeps `use_cpuset = false`
  - `Overall result: PASS`

### 4. Workspace regression check

Post-change workspace verification has also been re-run through:

```bash
bash bench/scripts/verify_workspace.sh
```

Most recent result:

- `cargo test --workspace`: `PASS`
- `cargo fmt --all -- --check`: `PASS`
- `cargo clippy --all-targets --all-features -- -D warnings`: `PASS`
- mock daemon smoke: `PASS`
- Linux source preflight smoke: `PASS`

## Current Route

The recommended continuation path is:

1. keep `qwen2.5:0.5b` as the default first model
2. re-run the real smoke with `noop` when validating runtime visibility only
3. run the same smoke with `linux-command-dry-run`
4. inspect audit highlights in `docs/verification_log.md`, especially that rollback remains limited to the actions actually enabled by policy
5. only then decide whether the environment is ready for a real `linux-command` A/B run

## Recommended Commands

### Reconfirm current observation path

```bash
bash bench/scripts/inference_tail_guard_ollama_smoke.sh
```

### Preview planned command actions safely

```bash
AEGISAI_DAEMON_BACKEND=linux-command-dry-run \
  bash bench/scripts/inference_tail_guard_ollama_smoke.sh
```

### Re-run workspace checks

```bash
bash bench/scripts/verify_workspace.sh
```

## Known Constraints

- `linux-command` may attempt real `renice` and `taskset` changes against the observed target PID
- those changes may require privileges depending on the host policy and target process ownership
- `linux-command-dry-run` is safe for logging and route validation, but it is not an A/B performance result by itself
- current smoke validation is strongest for `run_queue_delay`; the other planned Linux signals still need broader real-runtime coverage

## Resume Prompt

If a future session needs a direct restart prompt, use this:

> Continue from the current post-Ollama-smoke state in `/home/gg/AegisAI_Runtime`. Treat `docs/handoff.md` and the latest `Inference Tail Guard Ollama smoke` entries in `docs/verification_log.md` as the source of truth. Keep `qwen2.5:0.5b` as the default first model, preserve append-only logging, and prefer `linux-command-dry-run` before any real `linux-command` experiment.

## Source Of Truth

When resuming, read these first:

- `docs/handoff.md`
- `docs/verification_log.md`
- `bench/scripts/inference_tail_guard_ollama_smoke.sh`
- `bench/inference_tail_guard/README.md`
