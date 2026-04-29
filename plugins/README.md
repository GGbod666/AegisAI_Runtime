# Legacy Plugin Placeholders

`plugins/` is no longer an active implementation axis for AegisAI Runtime.

The current project direction is:

- capability axis under `agent/` and `ebpf/`
- scenario axis under `scenarios/`
- benchmark and validation under `bench/`

This directory only remains as a legacy placeholder from the earlier plugin-oriented skeleton.
Do not add new runtime logic here.

If a new feature is foundational and reusable, place it under `agent/` or `ebpf/`.
If a new feature is a problem-specific optimization policy, place it under `scenarios/`.
