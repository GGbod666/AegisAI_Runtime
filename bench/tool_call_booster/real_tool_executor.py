#!/usr/bin/env python3
"""Small real process-tree sample for the Tool Call Booster harness."""

from __future__ import annotations

import argparse
import hashlib
import json
import os
import subprocess
import sys
import time
from pathlib import Path


def burn_cpu(duration_s: float) -> str:
    deadline = time.monotonic() + duration_s
    seed = b"aegisai-tool-call-booster"
    digest = seed
    while time.monotonic() < deadline:
        digest = hashlib.sha256(digest + seed).digest()
    return digest.hex()


def write_io(path: Path, size_kb: int) -> None:
    if size_kb <= 0:
        return

    chunk = b"aegisai-tool-call-booster\n" * 64
    remaining = size_kb * 1024
    with path.open("ab") as handle:
        while remaining > 0:
            payload = chunk[: min(len(chunk), remaining)]
            handle.write(payload)
            remaining -= len(payload)
        handle.flush()
        os.fsync(handle.fileno())


def run_worker(args: argparse.Namespace) -> int:
    started = time.perf_counter_ns()
    if args.start_delay_ms:
        time.sleep(args.start_delay_ms / 1000)

    digest = burn_cpu(args.cpu_ms / 1000)
    write_io(args.output_dir / f"{args.stage}-{os.getpid()}.dat", args.io_kb)
    ended = time.perf_counter_ns()

    print(
        json.dumps(
            {
                "role": args.stage,
                "tool_call_id": args.tool_call_id,
                "pid": os.getpid(),
                "ppid": os.getppid(),
                "duration_ms": round((ended - started) / 1_000_000, 3),
                "digest_prefix": digest[:12],
            },
            sort_keys=True,
        ),
        flush=True,
    )
    return 0


def run_executor(args: argparse.Namespace) -> int:
    args.output_dir.mkdir(parents=True, exist_ok=True)
    started = time.perf_counter_ns()
    children: list[subprocess.Popen[str]] = []

    for stage in ("retrieval-worker", "rerank-worker", "background-worker"):
        cmd = [
            sys.executable,
            str(Path(__file__).resolve()),
            stage,
            "--tool-call-id",
            args.tool_call_id,
            "--output-dir",
            str(args.output_dir),
            "--cpu-ms",
            str(args.worker_cpu_ms),
            "--io-kb",
            str(args.worker_io_kb),
            "--start-delay-ms",
            str(args.worker_start_delay_ms),
        ]
        children.append(subprocess.Popen(cmd, text=True))

    digest = burn_cpu(args.executor_cpu_ms / 1000)
    statuses = [child.wait() for child in children]
    ended = time.perf_counter_ns()

    print(
        json.dumps(
            {
                "role": "tool-executor",
                "tool_call_id": args.tool_call_id,
                "pid": os.getpid(),
                "child_statuses": statuses,
                "duration_ms": round((ended - started) / 1_000_000, 3),
                "digest_prefix": digest[:12],
            },
            sort_keys=True,
        ),
        flush=True,
    )
    return 0 if all(status == 0 for status in statuses) else 1


def parser() -> argparse.ArgumentParser:
    parser = argparse.ArgumentParser(description=__doc__)
    subcommands = parser.add_subparsers(dest="role", required=True)

    executor = subcommands.add_parser("tool-executor")
    executor.add_argument("--tool-call-id", required=True)
    executor.add_argument("--output-dir", type=Path, required=True)
    executor.add_argument("--executor-cpu-ms", type=int, default=1800)
    executor.add_argument("--worker-cpu-ms", type=int, default=2600)
    executor.add_argument("--worker-io-kb", type=int, default=256)
    executor.add_argument("--worker-start-delay-ms", type=int, default=50)
    executor.set_defaults(func=run_executor)

    for role in ("retrieval-worker", "rerank-worker", "background-worker"):
        worker = subcommands.add_parser(role)
        worker.add_argument("--tool-call-id", required=True)
        worker.add_argument("--output-dir", type=Path, required=True)
        worker.add_argument("--cpu-ms", type=int, default=2600)
        worker.add_argument("--io-kb", type=int, default=256)
        worker.add_argument("--start-delay-ms", type=int, default=50)
        worker.set_defaults(stage=role, func=run_worker)

    return parser


def main() -> int:
    args = parser().parse_args()
    return args.func(args)


if __name__ == "__main__":
    raise SystemExit(main())
