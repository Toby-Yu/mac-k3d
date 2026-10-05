#!/usr/bin/env python3
"""What each task declares it needs, and how many of them fit on this worker.

Harbor applies ``[verifier.environment]`` cpus/memory_mb/storage_mb from task.toml
unless something overrides them. Up to PF.3 mac-k3d overrode both with
``CPU_LOCK_QTY / EVAL_SLOTS``, which handed a task declaring 2 CPUs and 8 GB a
single CPU and whatever memory was left. Now the declared numbers stand and this
module only decides how many trials run at once.
"""

from __future__ import annotations

import argparse
import json
import re
import shutil
import sys
import time
from pathlib import Path

# Leave this much RAM to the host, the Jenkins agent and Docker itself.
HOST_RESERVE_GB = 2
# Below this much free space Docker image pulls fail mid-run.
MIN_DISK_GB = 40

_CPUS = re.compile(r"^\s*cpus\s*=\s*([0-9]+(?:\.[0-9]+)?)\s*$", re.M)
_MEMORY_MB = re.compile(r"^\s*memory_mb\s*=\s*([0-9]+)\s*$", re.M)
_STORAGE_MB = re.compile(r"^\s*storage_mb\s*=\s*([0-9]+)\s*$", re.M)


def _meminfo_kb(key: str) -> int | None:
    try:
        text = Path("/proc/meminfo").read_text(encoding="utf-8")
    except OSError:
        return None
    for line in text.splitlines():
        if line.startswith(f"{key}:"):
            parts = line.split()
            if len(parts) >= 2 and parts[1].isdigit():
                return int(parts[1])
    return None


def mem_available_kb() -> int | None:
    """Free right now. Decides how many trials start, not whether any can."""
    return _meminfo_kb("MemAvailable")


def mem_total_kb() -> int | None:
    """Installed RAM. Decides whether this worker can host the suite at all."""
    return _meminfo_kb("MemTotal")


def disk_free_gb(path: str | Path) -> float | None:
    try:
        return shutil.disk_usage(str(path)).free / (1024**3)
    except OSError:
        return None


def declared_for_task(task_toml: Path) -> dict:
    """Largest cpus / memory_mb / storage_mb any environment block in this task asks for.

    A task.toml carries the same keys under ``[verifier.environment]`` and
    ``[environment]``; the agent and the verifier both have to fit, so take the max.
    """
    try:
        text = task_toml.read_text(encoding="utf-8")
    except OSError:
        return {}
    out: dict = {}
    cpus = [float(m) for m in _CPUS.findall(text)]
    mem = [int(m) for m in _MEMORY_MB.findall(text)]
    storage = [int(m) for m in _STORAGE_MB.findall(text)]
    if cpus:
        out["cpus"] = max(cpus)
    if mem:
        out["memory_mb"] = max(mem)
    if storage:
        out["storage_mb"] = max(storage)
    return out


def declared_for_tasks(tasks_dir: Path, task_ids: list[str]) -> dict:
    """Per-task declarations plus the peak across them all."""
    per_task: dict[str, dict] = {}
    for tid in task_ids:
        got = declared_for_task(tasks_dir / tid / "task.toml")
        if got:
            per_task[tid] = got
    peak = {
        "cpus": max((d.get("cpus", 0) for d in per_task.values()), default=0),
        "memory_mb": max((d.get("memory_mb", 0) for d in per_task.values()), default=0),
        "storage_mb": max((d.get("storage_mb", 0) for d in per_task.values()), default=0),
    }
    return {"per_task": per_task, "peak": peak, "tasks": len(task_ids), "declared_tasks": len(per_task)}


def plan_slots(
    peak: dict,
    cpu_lock_qty: int,
    trials: int,
    mem_available_kb_: int | None,
    mem_total_kb_: int | None,
    free_disk_gb: float | None,
    measured_peak_gb: float | None = None,
) -> dict:
    """How many trials run at once, and why.

    Admission (``fits``) asks whether the worker could host one trial at the
    declared size, so it uses installed RAM. Throughput uses RAM free right now,
    and a measured container peak in preference to the declared ceiling, which is
    a limit Docker enforces rather than what a trial actually uses.
    """
    cpus = float(peak.get("cpus") or 0)
    mem_mb = int(peak.get("memory_mb") or 0)
    reasons: list[str] = []
    limits: list[int] = []
    blockers: list[str] = []

    if cpus > 0:
        by_cpu = int(cpu_lock_qty // cpus)
        limits.append(by_cpu)
        reasons.append(f"cpu_lock {cpu_lock_qty}/{cpus:g} = {by_cpu}")
        if by_cpu < 1:
            blockers.append(f"each trial declares {cpus:g} CPUs but this build locked only {cpu_lock_qty}")
    if mem_mb > 0 and mem_total_kb_:
        installed_gb = mem_total_kb_ / 1024 / 1024 - HOST_RESERVE_GB
        if installed_gb < mem_mb / 1024:
            blockers.append(
                f"each trial declares {mem_mb / 1024:.1f} GB but the worker has "
                f"{mem_total_kb_ / 1024 / 1024:.1f} GB installed"
            )
    per_trial_gb = measured_peak_gb if measured_peak_gb and measured_peak_gb > 0 else (mem_mb / 1024 if mem_mb else 0)
    if per_trial_gb > 0 and mem_available_kb_:
        usable_gb = mem_available_kb_ / 1024 / 1024 - HOST_RESERVE_GB
        by_mem = int(usable_gb // per_trial_gb)
        limits.append(by_mem)
        source = "measured peak" if measured_peak_gb else "declared"
        reasons.append(f"free ram {usable_gb:.1f}GB/{per_trial_gb:.1f}GB ({source}) = {by_mem}")

    slots = min(limits) if limits else 1
    if free_disk_gb is not None and free_disk_gb < MIN_DISK_GB:
        slots = min(slots, 1)
        reasons.append(f"free disk {free_disk_gb:.0f}GB below {MIN_DISK_GB}GB, one at a time")
    slots = max(1, slots)
    if trials > 0:
        slots = min(slots, trials)
    return {
        "slots": slots,
        "fits": not blockers,
        "blockers": blockers,
        "cpus_each": cpus or None,
        "memory_mb_each": mem_mb or None,
        "per_trial_gb": per_trial_gb or None,
        "reasons": reasons,
    }


def _shell(name: str, value) -> str:
    return f"{name}={value}"


def sample_peak(harness_dir: Path, slots: int = 0) -> float | None:
    """Append one docker-stats snapshot to the build's memory trace.

    One ``harbor run`` has no single active question, so the trace is per build:
    ``render_report`` takes the largest ``peak_gb`` it finds, and the next build
    feeds that back in as ``--measured-peak-gb``.
    """
    from eval_slots import measure_container_gb

    gb = measure_container_gb()
    if gb is None or gb <= 0:
        return None
    harness_dir.mkdir(parents=True, exist_ok=True)
    row = {"at": int(time.time()), "peak_gb": round(gb, 4), "slots": slots}
    with (harness_dir / "container_mem.jsonl").open("a", encoding="utf-8") as handle:
        handle.write(json.dumps(row) + "\n")
    peak_file = harness_dir / "container_mem_peak_gb"
    prev = 0.0
    if peak_file.is_file():
        try:
            prev = float(peak_file.read_text(encoding="utf-8").strip())
        except (OSError, ValueError):
            prev = 0.0
    peak = max(prev, gb)
    peak_file.write_text(f"{peak:.6f}\n", encoding="utf-8")
    return peak


def main() -> int:
    ap = argparse.ArgumentParser()
    sub = ap.add_subparsers(dest="cmd")

    sample_p = sub.add_parser("sample", help="record one docker-stats memory snapshot")
    sample_p.add_argument("--harness-dir", required=True)
    sample_p.add_argument("--slots", type=int, default=0)

    plan_p = sub.add_parser("plan", help="declared resources and how many trials fit")
    plan_p.add_argument("--tasks-dir", required=True)
    plan_p.add_argument("--selected", required=True, help="newline-separated task dir names")
    plan_p.add_argument("--cpu", type=int, required=True, help="CPU_LOCK_QTY for this build")
    plan_p.add_argument("--n-rollouts", type=int, default=1)
    plan_p.add_argument("--workdir", default=".")
    plan_p.add_argument(
        "--measured-peak-gb", type=float, default=0.0, help="observed container peak, when a prior run measured one"
    )
    plan_p.add_argument("--out", default="", help="also write the plan as JSON here")
    args = ap.parse_args()
    if args.cmd is None:
        ap.error("need a subcommand (plan or sample)")

    if args.cmd == "sample":
        sample_peak(Path(args.harness_dir), args.slots)
        return 0

    tasks_dir = Path(args.tasks_dir)
    ids = [ln.strip() for ln in Path(args.selected).read_text(encoding="utf-8").splitlines() if ln.strip()]
    declared = declared_for_tasks(tasks_dir, ids)
    peak = declared["peak"]
    trials = len(ids) * max(1, args.n_rollouts)
    avail_kb = mem_available_kb()
    total_kb = mem_total_kb()
    free_disk = disk_free_gb(args.workdir)
    plan = plan_slots(
        peak, max(1, args.cpu), trials, avail_kb, total_kb, free_disk, args.measured_peak_gb or None
    )

    if not plan["fits"]:
        print(
            "ERROR: this worker cannot run the selected tasks as declared: "
            + "; ".join(plan["blockers"])
            + ". Raise CPU_LOCK_QTY, send the job to a bigger worker, or set "
            "EVAL_OVERRIDE_CPUS / EVAL_OVERRIDE_MEMORY_MB to run below what the "
            "task declares (the run is then not comparable to an official one).",
            file=sys.stderr,
        )
        return 1

    doc = {
        "declared": declared,
        "applied": {
            "slots": plan["slots"],
            "cpus_each": plan["cpus_each"],
            "memory_mb_each": plan["memory_mb_each"],
            "per_trial_gb": plan["per_trial_gb"],
            "cpu_lock_qty": args.cpu,
            "source": "task.toml",
        },
        "host": {
            "mem_total_gb": round(total_kb / 1024 / 1024, 1) if total_kb else None,
            "mem_available_gb": round(avail_kb / 1024 / 1024, 1) if avail_kb else None,
            "free_disk_gb": round(free_disk, 1) if free_disk is not None else None,
        },
        "trials": trials,
        "reasons": plan["reasons"],
    }
    if args.out:
        out = Path(args.out)
        out.parent.mkdir(parents=True, exist_ok=True)
        out.write_text(json.dumps(doc, indent=2) + "\n", encoding="utf-8")

    why = "; ".join(plan["reasons"]) or "no declared resources found; one at a time"
    print(
        f"P5 resources: declared cpus={peak['cpus'] or '?'} memory_mb={peak['memory_mb'] or '?'} "
        f"per trial, {plan['slots']} at a time ({why})",
        file=sys.stderr,
    )
    print(_shell("EVAL_SLOTS", plan["slots"]))
    print(_shell("DECLARED_CPUS", peak["cpus"] or 0))
    print(_shell("DECLARED_MEMORY_MB", peak["memory_mb"] or 0))
    print(_shell("DECLARED_STORAGE_MB", peak["storage_mb"] or 0))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
