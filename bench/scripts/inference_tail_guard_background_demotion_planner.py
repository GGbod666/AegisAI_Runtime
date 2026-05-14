#!/usr/bin/env python3
"""Dry-run background demotion planner for Inference Tail Guard.

This planner only classifies processes and writes proposed controls. It never
calls renice, taskset, cgcreate, or writes cgroup files.
"""

from __future__ import annotations

import argparse
import csv
import json
import os
import pathlib
import sys
from dataclasses import asdict, dataclass, field
from datetime import datetime, timezone
from typing import Iterable


REPO_ROOT = pathlib.Path(__file__).resolve().parents[2]
DEFAULT_REPORT_PATH = REPO_ROOT / "docs" / "tail_guard_background_demotion_plan.md"

INTERACTIVE_NAMES = {
    "bash",
    "zsh",
    "fish",
    "ssh",
    "sshd",
    "tmux",
    "screen",
    "gnome-shell",
    "plasmashell",
    "xorg",
    "wayland",
    "code",
    "firefox",
    "chrome",
    "chromium",
}
INFERENCE_NAMES = {"ollama", "llama-server", "llama-cli"}
BACKGROUND_NAMES = {"stress-ng"}
BACKGROUND_MARKERS = {
    "aegisai-background-job",
    "aegisai_background_job",
    "background_job",
    "batch_worker",
    "batch-job",
    "batch_job",
}
ROLLBACK_REQUIREMENTS = (
    "current_nice",
    "cgroup_path",
    "cpus_allowed_list",
    "cpu.weight",
    "cpu.max",
    "cgroup.procs membership",
)


@dataclass(frozen=True)
class ProcessRecord:
    pid: int
    ppid: int = 0
    name: str = ""
    cmdline: str = ""
    cgroup_path: str = ""
    tags: tuple[str, ...] = ()
    nice: int | None = None
    cpus_allowed_list: str = ""
    tty_nr: int = 0
    uid: int | None = None


@dataclass(frozen=True)
class PlanRow:
    pid: int
    ppid: int
    name: str
    classification: str
    decision: str
    rejection_reason: str
    current_nice: str
    proposed_nice: str
    proposed_cpu_weight: str
    proposed_cpu_max: str
    proposed_owned_cgroup: str
    rollback_capture_required: str
    live_mutation: bool
    cmdline: str
    cgroup_path: str
    tags: str


@dataclass(frozen=True)
class PlannerSummary:
    run_id: str
    live_mutation_count: int
    protected_inference_count: int
    candidate_background_count: int
    rejected_unknown_count: int
    rejected_interactive_count: int
    rejected_limit_count: int
    affected_set_count: int
    max_affected_set: int
    verdict: str
    artifacts: dict[str, str] = field(default_factory=dict)


def parse_args(argv: list[str]) -> argparse.Namespace:
    run_id = os.environ.get(
        "AEGISAI_TAIL_DEMOTION_RUN_ID",
        "tail_background_demotion_" + datetime.now(timezone.utc).strftime("%Y%m%dT%H%M%SZ"),
    )
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--run-id", default=run_id)
    parser.add_argument(
        "--inventory",
        type=pathlib.Path,
        default=pathlib.Path(os.environ["AEGISAI_TAIL_DEMOTION_INVENTORY"])
        if os.environ.get("AEGISAI_TAIL_DEMOTION_INVENTORY")
        else None,
        help="Optional JSON inventory. Defaults to a read-only /proc scan.",
    )
    parser.add_argument(
        "--protected-pids",
        default=os.environ.get("AEGISAI_TAIL_DEMOTION_PROTECTED_PIDS", ""),
        help="Comma or space separated inference PIDs that must never be demoted.",
    )
    parser.add_argument(
        "--max-candidates",
        type=int,
        default=int(os.environ.get("AEGISAI_TAIL_DEMOTION_MAX_CANDIDATES", "8")),
        help="Bounded affected-set limit.",
    )
    parser.add_argument(
        "--artifact-dir",
        type=pathlib.Path,
        default=pathlib.Path(os.environ["AEGISAI_TAIL_DEMOTION_ARTIFACT_DIR"])
        if os.environ.get("AEGISAI_TAIL_DEMOTION_ARTIFACT_DIR")
        else None,
    )
    parser.add_argument(
        "--report",
        type=pathlib.Path,
        default=pathlib.Path(os.environ.get("AEGISAI_TAIL_DEMOTION_REPORT", DEFAULT_REPORT_PATH)),
    )
    args = parser.parse_args(argv)
    if args.artifact_dir is None:
        args.artifact_dir = (
            REPO_ROOT / ".cache" / "aegisai" / "inference_tail_guard_background_demotion" / args.run_id
        )
    return args


def parse_pid_set(raw: str) -> set[int]:
    pids: set[int] = set()
    for item in raw.replace(",", " ").split():
        if item.isdigit() and int(item) > 0:
            pids.add(int(item))
    return pids


def read_text(path: pathlib.Path) -> str:
    try:
        return path.read_text(encoding="utf-8", errors="replace").strip()
    except OSError:
        return ""


def parse_proc_stat(path: pathlib.Path) -> tuple[int | None, int]:
    text = read_text(path)
    if not text:
        return None, 0
    try:
        rest = text.rsplit(")", 1)[1].strip().split()
        tty_nr = int(rest[4])
        nice = int(rest[16])
        return nice, tty_nr
    except (IndexError, ValueError):
        return None, 0


def scan_proc(proc_root: pathlib.Path = pathlib.Path("/proc")) -> list[ProcessRecord]:
    records: list[ProcessRecord] = []
    for child in proc_root.iterdir():
        if not child.name.isdigit():
            continue
        pid = int(child.name)
        status = read_text(child / "status")
        fields: dict[str, str] = {}
        for line in status.splitlines():
            if ":" in line:
                key, value = line.split(":", 1)
                fields[key] = value.strip()
        cmdline = read_text(child / "cmdline").replace("\x00", " ").strip()
        cgroup = read_text(child / "cgroup")
        cgroup_path = ""
        for line in cgroup.splitlines():
            if line.startswith("0::"):
                cgroup_path = line[3:]
                break
        nice, tty_nr = parse_proc_stat(child / "stat")
        cpus_allowed = fields.get("Cpus_allowed_list", "")
        uid = None
        if fields.get("Uid"):
            first_uid = fields["Uid"].split()[0]
            uid = int(first_uid) if first_uid.isdigit() else None
        records.append(
            ProcessRecord(
                pid=pid,
                ppid=int(fields.get("PPid", "0") or 0),
                name=fields.get("Name", ""),
                cmdline=cmdline,
                cgroup_path=cgroup_path,
                tags=(),
                nice=nice,
                cpus_allowed_list=cpus_allowed,
                tty_nr=tty_nr,
                uid=uid,
            )
        )
    return records


def load_inventory(path: pathlib.Path | None) -> list[ProcessRecord]:
    if path is None:
        return scan_proc()
    raw = json.loads(path.read_text(encoding="utf-8"))
    items = raw.get("processes", raw) if isinstance(raw, dict) else raw
    records = []
    for item in items:
        records.append(
            ProcessRecord(
                pid=int(item["pid"]),
                ppid=int(item.get("ppid", 0)),
                name=str(item.get("name", "")),
                cmdline=str(item.get("cmdline", "")),
                cgroup_path=str(item.get("cgroup_path", item.get("cgroup", ""))),
                tags=tuple(str(tag) for tag in item.get("tags", ())),
                nice=item.get("nice"),
                cpus_allowed_list=str(item.get("cpus_allowed_list", "")),
                tty_nr=int(item.get("tty_nr", 0)),
                uid=item.get("uid"),
            )
        )
    return records


def has_tag(record: ProcessRecord, tag: str) -> bool:
    return any(existing.upper() == tag for existing in record.tags)


def text_contains_any(text: str, markers: Iterable[str]) -> bool:
    lower = text.lower()
    return any(marker in lower for marker in markers)


def classify(record: ProcessRecord, protected_pids: set[int]) -> tuple[str, str]:
    name = record.name.lower()
    if record.pid in protected_pids:
        return "protected_inference", "protected_pid_allowlist"
    if has_tag(record, "AI_INFERENCE") or has_tag(record, "INTERACTIVE_LATENCY_SENSITIVE"):
        return "protected_inference", "interactive_or_inference_tag"
    if name in INFERENCE_NAMES or text_contains_any(record.cmdline, {"inference_worker", "ollama serve"}):
        return "protected_inference", "inference_name_or_cmdline"
    if record.tty_nr != 0 or name in INTERACTIVE_NAMES:
        return "interactive_sensitive", "interactive_name_or_tty"
    if has_tag(record, "BACKGROUND_JOB") or name in BACKGROUND_NAMES or text_contains_any(record.cmdline, BACKGROUND_MARKERS):
        if record.nice is None or not record.cgroup_path or not record.cpus_allowed_list:
            return "background_job", "rollback_capture_incomplete"
        return "background_job", "candidate"
    return "unknown", "unknown_classification"


def build_plan(
    records: Iterable[ProcessRecord],
    protected_pids: set[int],
    max_candidates: int,
) -> list[PlanRow]:
    if max_candidates <= 0:
        raise ValueError("max_candidates must be positive")
    rows: list[PlanRow] = []
    accepted_candidates = 0
    for record in sorted(records, key=lambda item: item.pid):
        classification, reason = classify(record, protected_pids)
        decision = "reject"
        rejection = reason
        proposed_nice = ""
        proposed_cpu_weight = ""
        proposed_cpu_max = ""
        proposed_cgroup = ""
        if classification == "background_job" and reason == "candidate":
            if accepted_candidates >= max_candidates:
                rejection = "affected_set_limit"
            else:
                decision = "candidate"
                rejection = ""
                accepted_candidates += 1
                proposed_nice = str(min(19, int(record.nice or 0) + 5))
                proposed_cpu_weight = "50"
                proposed_cpu_max = "max 100000"
                proposed_cgroup = "aegisai.tail_guard.background.dry_run"
        rows.append(
            PlanRow(
                pid=record.pid,
                ppid=record.ppid,
                name=record.name,
                classification=classification,
                decision=decision,
                rejection_reason=rejection,
                current_nice="" if record.nice is None else str(record.nice),
                proposed_nice=proposed_nice,
                proposed_cpu_weight=proposed_cpu_weight,
                proposed_cpu_max=proposed_cpu_max,
                proposed_owned_cgroup=proposed_cgroup,
                rollback_capture_required=",".join(ROLLBACK_REQUIREMENTS),
                live_mutation=False,
                cmdline=record.cmdline,
                cgroup_path=record.cgroup_path,
                tags=",".join(record.tags),
            )
        )
    return rows


def summarize(run_id: str, rows: list[PlanRow], max_candidates: int) -> PlannerSummary:
    live_mutations = sum(1 for row in rows if row.live_mutation)
    candidates = sum(1 for row in rows if row.decision == "candidate")
    summary = PlannerSummary(
        run_id=run_id,
        live_mutation_count=live_mutations,
        protected_inference_count=sum(1 for row in rows if row.classification == "protected_inference"),
        candidate_background_count=candidates,
        rejected_unknown_count=sum(1 for row in rows if row.rejection_reason == "unknown_classification"),
        rejected_interactive_count=sum(1 for row in rows if row.classification == "interactive_sensitive"),
        rejected_limit_count=sum(1 for row in rows if row.rejection_reason == "affected_set_limit"),
        affected_set_count=candidates,
        max_affected_set=max_candidates,
        verdict="PASS" if live_mutations == 0 and candidates <= max_candidates else "FAIL",
    )
    return summary


def write_plan_json(path: pathlib.Path, summary: PlannerSummary, rows: list[PlanRow]) -> None:
    payload = {
        "summary": asdict(summary),
        "mode": "dry_run_only",
        "live_mutation": False,
        "proposed_controls": {
            "nice_delta": "+5 up to nice 19",
            "cpu_weight": "50 in future owned cgroup",
            "cpu_max": "max 100000 until a live cgroup issue approves a hard cap",
        },
        "rollback_capture_requirements": ROLLBACK_REQUIREMENTS,
        "rows": [asdict(row) for row in rows],
    }
    path.write_text(json.dumps(payload, indent=2, sort_keys=True) + "\n", encoding="utf-8")


def write_plan_csv(path: pathlib.Path, rows: list[PlanRow]) -> None:
    fieldnames = list(PlanRow.__dataclass_fields__)
    with path.open("w", newline="", encoding="utf-8") as handle:
        writer = csv.DictWriter(handle, fieldnames=fieldnames)
        writer.writeheader()
        for row in rows:
            data = asdict(row)
            data["live_mutation"] = "false"
            writer.writerow(data)


def table(headers: list[str], rows: list[list[str]]) -> list[str]:
    return [
        "| " + " | ".join(headers) + " |",
        "| " + " | ".join(["---"] * len(headers)) + " |",
        *["| " + " | ".join(row) + " |" for row in rows],
    ]


def build_report(summary: PlannerSummary, rows: list[PlanRow], json_path: pathlib.Path, csv_path: pathlib.Path) -> str:
    generated = datetime.now(timezone.utc).strftime("%Y-%m-%dT%H:%M:%SZ")
    display_rows = [
        row
        for row in rows
        if row.decision == "candidate" or row.classification == "protected_inference"
    ]
    if len(display_rows) < 20:
        display_rows.extend(
            row
            for row in rows
            if row not in display_rows
        )
    display_rows = display_rows[:20]
    candidate_lines = table(
        ["pid", "name", "class", "decision", "reject", "nice", "proposed nice", "cpu.weight"],
        [
            [
                str(row.pid),
                row.name,
                row.classification,
                row.decision,
                row.rejection_reason or "n/a",
                row.current_nice or "n/a",
                row.proposed_nice or "n/a",
                row.proposed_cpu_weight or "n/a",
            ]
            for row in display_rows
        ],
    )
    return "\n".join(
        [
            "# Tail Guard Background Demotion Dry-Run Plan",
            "",
            f"- Generated: `{generated}`",
            f"- Run ID: `{summary.run_id}`",
            "- Mode: `dry_run_only`",
            "- Runtime behavior: `not_connected`",
            "- Live mutation: `false`",
            f"- Verdict: `{summary.verdict}`",
            f"- Protected inference PIDs: `{summary.protected_inference_count}`",
            f"- Candidate background PIDs: `{summary.candidate_background_count}`",
            f"- Affected set bound: `{summary.affected_set_count}/{summary.max_affected_set}`",
            f"- Unknown processes rejected: `{summary.rejected_unknown_count}`",
            f"- Interactive-sensitive processes rejected: `{summary.rejected_interactive_count}`",
            f"- Limit rejections: `{summary.rejected_limit_count}`",
            "",
            "## Proposed Controls",
            "",
            "- `nice`: increase background candidates by `+5`, capped at nice `19`.",
            "- `cpu.weight`: plan `50` only inside a future administrator-created AegisAI-owned cgroup v2 subtree.",
            "- `cpu.max`: keep `max 100000` in this dry run; hard quota remains for the guarded owned-cgroup applier issue.",
            "- Rollback capture requirements: `" + ",".join(ROLLBACK_REQUIREMENTS) + "`.",
            "",
            "## Process Decisions",
            "",
            f"- Full process decision inventory is in `{csv_path}`.",
            f"- Markdown preview rows: `{len(display_rows)}/{len(rows)}`.",
            "",
            *candidate_lines,
            "",
            "## Safety Boundary",
            "",
            "- Unknown processes are rejected instead of inferred as safe background work.",
            "- Interactive-latency-sensitive and protected inference processes are rejected even when they match an allowlist by name.",
            "- This artifact does not call `renice`, `taskset`, or write `cpu.weight`, `cpu.max`, `cgroup.procs`, or any cgroup filesystem path.",
            "- Future live cgroup writes remain blocked until the guarded owned-cgroup isolation applier supplies explicit confirmation and rollback evidence.",
            "",
            "## Artifacts",
            "",
            f"- Plan JSON: `{json_path}`",
            f"- Candidate CSV: `{csv_path}`",
        ]
    ) + "\n"


def run(args: argparse.Namespace) -> PlannerSummary:
    protected_pids = parse_pid_set(args.protected_pids)
    records = load_inventory(args.inventory)
    rows = build_plan(records, protected_pids, args.max_candidates)
    summary = summarize(args.run_id, rows, args.max_candidates)

    args.artifact_dir.mkdir(parents=True, exist_ok=True)
    json_path = args.artifact_dir / "background_demotion_plan.json"
    csv_path = args.artifact_dir / "background_demotion_candidates.csv"
    write_plan_json(json_path, summary, rows)
    write_plan_csv(csv_path, rows)
    args.report.parent.mkdir(parents=True, exist_ok=True)
    args.report.write_text(build_report(summary, rows, json_path, csv_path), encoding="utf-8")
    return PlannerSummary(
        **{
            **asdict(summary),
            "artifacts": {
                "plan_json": str(json_path),
                "candidate_csv": str(csv_path),
                "report": str(args.report),
            },
        }
    )


def main(argv: list[str] | None = None) -> int:
    args = parse_args(argv or sys.argv[1:])
    try:
        summary = run(args)
    except Exception as error:  # pragma: no cover - CLI boundary
        print(f"background_demotion_planner=FAIL reason={error}", file=sys.stderr)
        return 1
    print(
        "background_demotion_planner="
        f"{summary.verdict} candidates={summary.candidate_background_count} "
        f"affected_set={summary.affected_set_count}/{summary.max_affected_set} "
        f"live_mutations={summary.live_mutation_count} "
        f"artifact_dir={args.artifact_dir}"
    )
    return 0 if summary.verdict == "PASS" else 1


if __name__ == "__main__":
    raise SystemExit(main())
