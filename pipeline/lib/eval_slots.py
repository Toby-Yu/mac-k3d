"""CPU_LOCK_QTY slot pool: Docker memory cap, disk floor, one question at a time."""

from __future__ import annotations

import argparse
import json
import os
import re
import subprocess
from pathlib import Path

HOST_RESERVE_GB = 2
MIN_DISK_GB = 40
MEM_BUFFER = 1.5
# Ignore startup noise / mis-parses below ~10 MiB so they cannot become peak 0.00.
MIN_SAMPLE_GB = 0.01
PEAK_NAME = "container_mem_peak_gb"
CURRENT_NAME = "container_mem_current.json"
JSONL_NAME = "container_mem.jsonl"
HISTORY_NAME = "container_mem_history.jsonl"
ACTIVE_QUESTION_NAME = "active_question.txt"
# Used only when a benchmark has no live container sample. DeepSWE and LoLBench
# follow CPU_LOCK_QTY until docker stats sees an eval container.
FALLBACK_GB = {
    "swebenchpro": 8,
}
_MEM_RE = re.compile(r"([0-9]+(?:\.[0-9]+)?)\s*([KMGT]i?B)", re.IGNORECASE)
_MAIN_RE = re.compile(r"-main-\d+")


def mem_available_kb() -> int | None:
    path = Path("/proc/meminfo")
    if not path.is_file():
        return None
    for line in path.read_text(encoding="utf-8", errors="replace").splitlines():
        if line.startswith("MemAvailable:"):
            parts = line.split()
            if len(parts) >= 2 and parts[1].isdigit():
                return int(parts[1])
    return None


def parse_mem_gb(text: str) -> float | None:
    match = _MEM_RE.search(text.split("/")[0])
    if not match:
        return None
    value = float(match.group(1))
    unit = match.group(2).lower()
    scale = {"kb": 1 / 1024 / 1024, "kib": 1 / 1024 / 1024, "mb": 1 / 1024, "mib": 1 / 1024, "gb": 1.0, "gib": 1.0, "tb": 1024.0, "tib": 1024.0}
    factor = scale.get(unit)
    if factor is None:
        return None
    return value * factor


def usable_sample_gb(sample_gb: float | None) -> float | None:
    """Drop readings below MIN_SAMPLE_GB so noise cannot become the recorded peak."""
    if sample_gb is None or sample_gb < MIN_SAMPLE_GB:
        return None
    return sample_gb


def _is_eval_container(line: str) -> bool:
    # Harbor job names contain _icode_. Older trials use __env-main-1; current
    # Harbor often names the main service task__hash-main-1. Egress sidecars
    # also contain __ and must not set the memory peak.
    name = line.split("\t", 1)[0]
    low = name.lower()
    if "egress" in low or "sidecar" in low:
        return False
    if "_icode_" in name or "__env-main-" in name:
        return True
    return "__" in name and _MAIN_RE.search(name) is not None


def max_icode_mem_gb(stats_text: str) -> float | None:
    best: float | None = None
    for line in stats_text.splitlines():
        if not _is_eval_container(line):
            continue
        gb = usable_sample_gb(parse_mem_gb(line))
        if gb is None:
            continue
        best = gb if best is None else max(best, gb)
    return best


def measure_container_gb() -> float | None:
    try:
        proc = subprocess.run(
            ["docker", "stats", "--no-stream", "--format", "{{.Name}}\t{{.MemUsage}}"],
            check=False,
            capture_output=True,
            text=True,
            timeout=20,
        )
    except (OSError, subprocess.TimeoutExpired):
        return None
    if proc.returncode != 0:
        return None
    return max_icode_mem_gb(proc.stdout)


def peak_path(workdir: str | Path) -> Path:
    return Path(workdir) / "harness" / PEAK_NAME


def noted_peak_gb(workdir: str | Path, sample_gb: float | None) -> float | None:
    path = peak_path(workdir)
    prev: float | None = None
    if path.is_file():
        try:
            prev = float(path.read_text(encoding="utf-8").strip())
        except (OSError, ValueError):
            prev = None
    sample = usable_sample_gb(sample_gb)
    prev_ok = usable_sample_gb(prev)
    vals = [v for v in (prev_ok, sample) if v is not None]
    if not vals:
        return None
    peak = max(vals)
    if sample is not None:
        try:
            path.parent.mkdir(parents=True, exist_ok=True)
            path.write_text(f"{peak:.6f}\n", encoding="utf-8")
        except OSError:
            pass
    return peak


def _mem_dir(workdir: str | Path) -> Path:
    return Path(workdir) / "harness"


def active_question_path(workdir: str | Path) -> Path:
    return _mem_dir(workdir) / ACTIVE_QUESTION_NAME


def read_active_question(workdir: str | Path) -> str:
    path = active_question_path(workdir)
    if not path.is_file():
        return ""
    try:
        return path.read_text(encoding="utf-8").strip()
    except OSError:
        return ""


def write_active_question(workdir: str | Path, question: str) -> None:
    path = active_question_path(workdir)
    try:
        path.parent.mkdir(parents=True, exist_ok=True)
        if question:
            path.write_text(f"{question}\n", encoding="utf-8")
        elif path.is_file():
            path.unlink()
    except OSError:
        return


def observe_question(
    workdir: str | Path,
    question: str,
    sample_gb: float | None,
    slots: int,
    memory_mb: int,
    benchmark: str,
) -> None:
    if not question:
        return
    path = _mem_dir(workdir) / CURRENT_NAME
    peak = 0.0
    if path.is_file():
        try:
            doc = json.loads(path.read_text(encoding="utf-8"))
        except (OSError, json.JSONDecodeError):
            doc = {}
        if doc.get("question") == question:
            try:
                peak = float(doc.get("peak_gb") or 0)
            except (TypeError, ValueError):
                peak = 0.0
    sample = usable_sample_gb(sample_gb)
    if sample is not None and sample > peak:
        peak = sample
    try:
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_text(
            json.dumps(
                {
                    "question": question,
                    "peak_gb": peak,
                    "slots": int(slots),
                    "memory_mb": int(memory_mb),
                    "benchmark": benchmark,
                }
            ),
            encoding="utf-8",
        )
    except OSError:
        return


def flush_question(
    workdir: str | Path,
    question: str,
    slots: int,
    memory_mb: int,
    benchmark: str,
    *,
    final_sample: bool = True,
) -> dict:
    if final_sample:
        sample = measure_container_gb()
        if sample is not None:
            observe_question(workdir, question, sample, slots, memory_mb, benchmark)
    path = _mem_dir(workdir) / CURRENT_NAME
    peak = 0.0
    if path.is_file():
        try:
            doc = json.loads(path.read_text(encoding="utf-8"))
        except (OSError, json.JSONDecodeError):
            doc = {}
        if doc.get("question") == question:
            try:
                peak = float(doc.get("peak_gb") or 0)
            except (TypeError, ValueError):
                peak = 0.0
        try:
            path.unlink()
        except OSError:
            pass
    write_active_question(workdir, "")
    measured = usable_sample_gb(peak)
    row = {
        "question": question,
        "peak_gb": round(measured, 2) if measured is not None else None,
        "slots": int(slots),
        "memory_mb": int(memory_mb),
        "benchmark": benchmark,
    }
    _append_jsonl(_mem_dir(workdir) / JSONL_NAME, row)
    history = dict(row)
    history["build"] = os.environ.get("BUILD_NUMBER") or "local"
    _append_jsonl(_mem_dir(workdir) / HISTORY_NAME, history)
    return row


def sample_active_question(
    workdir: str | Path,
    *,
    slots: int = 0,
    memory_mb: int = 0,
    benchmark: str = "deepswe",
) -> str:
    """Measure docker stats for the active question (heartbeat sampler)."""
    question = read_active_question(workdir)
    if not question:
        return ""
    sample = measure_container_gb()
    observe_question(workdir, question, sample, slots, memory_mb, benchmark)
    return question


def _append_jsonl(path: Path, row: dict) -> None:
    try:
        path.parent.mkdir(parents=True, exist_ok=True)
        with path.open("a", encoding="utf-8") as handle:
            handle.write(json.dumps(row) + "\n")
    except OSError:
        pass


def budget_gb(benchmark: str, measured_gb: float | None) -> float | None:
    if measured_gb is not None and measured_gb > 0:
        return measured_gb * MEM_BUFFER
    return FALLBACK_GB.get((benchmark or "deepswe").strip().lower())


def ram_slots(benchmark: str, mem_kb: int | None = None, measured_gb: float | None = None) -> int | None:
    per = budget_gb(benchmark, measured_gb)
    if per is None or per <= 0:
        return None
    kb = mem_available_kb() if mem_kb is None else mem_kb
    if kb is None or kb <= 0:
        return None
    avail_gb = kb / 1024 / 1024
    budget = avail_gb - HOST_RESERVE_GB
    if budget < 1:
        return 1
    return max(1, int(budget // per))


def disk_free_gb(path: str | Path) -> float | None:
    try:
        usage = os.statvfs(path)
    except OSError:
        return None
    return (usage.f_bavail * usage.f_frsize) / (1024 * 1024 * 1024)


def disk_ok(path: str | Path, min_gb: int = MIN_DISK_GB) -> bool:
    free = disk_free_gb(path)
    if free is None:
        return True
    return free >= min_gb


def eval_slots(
    cpu_lock: int,
    benchmark: str = "deepswe",
    workdir: str | Path = ".",
    resource_cap: bool = True,
    mem_kb: int | None = None,
    container_gb: float | None = None,
    measure: bool = False,
) -> tuple[int, int]:
    """Return (EVAL_SLOTS, EVAL_CPUS_EACH). Slots stay at CPU_LOCK_QTY.

    A measured container peak is recorded for the report and does not change
    the slot count. Free disk below MIN_DISK_GB is the only shrink, to 1 slot.
    """
    del benchmark, mem_kb, container_gb, measure
    slots = max(int(cpu_lock), 1)
    if resource_cap and not disk_ok(workdir):
        slots = 1
    cpus_each = max(1, int(cpu_lock) // slots)
    return slots, cpus_each


def memory_limit_mb(
    mem_kb: int | None,
    cpu_lock: int,
    measured_gb: float | None,
) -> int:
    """No Docker memory cap. A real out-of-memory kill skips that question."""
    del mem_kb, cpu_lock, measured_gb
    return 0


def format_task_ids(task_ids: list[str]) -> str:
    ids = [tid for tid in task_ids if tid]
    if not ids:
        return "-"
    if len(ids) <= 8:
        return ",".join(ids)
    return f"{ids[0]}..{ids[-1]} (full list in selected_tasks.txt)"


def parallel_report_line(
    task_ids: list[str],
    n_rollouts: int,
    slots: int,
    mem_kb: int | None,
    container_gb: float | None = None,
    budget_gb_value: float | None = None,
) -> str:
    avail = (mem_kb / 1024 / 1024) if mem_kb else 0.0
    four = "yes" if slots >= 4 else "no"
    seen = f"{container_gb:.2f}" if container_gb else "unmeasured"
    budget = f"{budget_gb_value:.2f}" if budget_gb_value else "cpu_lock"
    return (
        f"P5 parallel: questions={len(task_ids)} ids={format_task_ids(task_ids)} "
        f"rollouts={n_rollouts} mem_available_gb={avail:.1f} "
        f"container_gb={seen} budget_gb={budget} slots={slots} "
        f"parallel_containers={slots} four_containers={four}"
    )


def parse_pairs(raw: str) -> list[tuple[str, int]]:
    out: list[tuple[str, int]] = []
    for part in raw.replace(",", " ").split():
        if ":" not in part:
            continue
        tid, attempt = part.rsplit(":", 1)
        if not tid or not attempt.isdigit():
            continue
        out.append((tid, int(attempt)))
    return out


def first_free_attempt(tid: str, n_rollouts: int, taken: set[tuple[str, int]]) -> int | None:
    for attempt in range(1, n_rollouts + 1):
        if (tid, attempt) not in taken:
            return attempt
    return None


def next_unit(
    task_ids: list[str],
    n_rollouts: int,
    assigned: list[tuple[str, int]],
    inflight: list[tuple[str, int]],
) -> tuple[str, int] | None:
    """Finish one question's rollouts before starting the next.

    A free slot stays on the in-flight question. When that question's rollouts
    are already launched, return None so the caller waits for them to exit.
    The next question starts only after nothing is in flight.
    """
    if n_rollouts < 1 or not task_ids:
        return None
    taken = set(assigned)

    def first_free(tid: str) -> int | None:
        return first_free_attempt(tid, n_rollouts, taken)

    if inflight:
        seen: list[str] = []
        for tid, _ in inflight:
            if tid not in seen:
                seen.append(tid)
        for tid in seen:
            attempt = first_free(tid)
            if attempt is not None:
                return tid, attempt
        return None

    for tid in task_ids:
        attempt = first_free(tid)
        if attempt is not None:
            return tid, attempt
    return None


def _read_ids(path: str) -> list[str]:
    text = Path(path).read_text(encoding="utf-8")
    return [ln.strip() for ln in text.splitlines() if ln.strip()]


def main() -> int:
    ap = argparse.ArgumentParser()
    sub = ap.add_subparsers(dest="cmd", required=True)
    slots_p = sub.add_parser("slots")
    slots_p.add_argument("--cpu", type=int, required=True)
    slots_p.add_argument("--benchmark", default=os.environ.get("BENCHMARK", "deepswe"))
    slots_p.add_argument("--workdir", default=os.environ.get("WORKDIR", "."))
    slots_p.add_argument("--question", default="")
    next_p = sub.add_parser("next")
    next_p.add_argument("--tasks-file", required=True)
    next_p.add_argument("--n-rollouts", type=int, required=True)
    next_p.add_argument("--assigned", default="")
    next_p.add_argument("--inflight", default="")
    report_p = sub.add_parser("report")
    report_p.add_argument("--tasks-file", required=True)
    report_p.add_argument("--n-rollouts", type=int, required=True)
    report_p.add_argument("--cpu", type=int, required=True)
    report_p.add_argument("--benchmark", default=os.environ.get("BENCHMARK", "deepswe"))
    report_p.add_argument("--workdir", default=os.environ.get("WORKDIR", "."))
    flush_p = sub.add_parser("flush")
    flush_p.add_argument("--question", required=True)
    flush_p.add_argument("--slots", type=int, default=0)
    flush_p.add_argument("--memory-mb", type=int, default=0)
    flush_p.add_argument("--benchmark", default=os.environ.get("BENCHMARK", "deepswe"))
    flush_p.add_argument("--workdir", default=os.environ.get("WORKDIR", "."))
    sample_p = sub.add_parser("sample")
    sample_p.add_argument("--workdir", default=os.environ.get("WORKDIR", "."))
    sample_p.add_argument("--slots", type=int, default=0)
    sample_p.add_argument("--memory-mb", type=int, default=0)
    sample_p.add_argument("--benchmark", default=os.environ.get("BENCHMARK", "deepswe"))
    args = ap.parse_args()
    cap = os.environ.get("EVAL_RESOURCE_CAP", "1") != "0"
    if args.cmd == "slots":
        sample = measure_container_gb() if cap else None
        slots, cpus = eval_slots(
            args.cpu,
            args.benchmark,
            args.workdir,
            resource_cap=cap,
        )
        print(f"EVAL_SLOTS={slots}")
        print(f"EVAL_CPUS_EACH={cpus}")
        print("EVAL_FITS=1")
        print("EVAL_MEMORY_MB=0")
        if args.question:
            observe_question(args.workdir, args.question, sample, slots, 0, args.benchmark)
        return 0
    if args.cmd == "sample":
        if not cap:
            return 0
        sample_active_question(
            args.workdir,
            slots=args.slots,
            memory_mb=args.memory_mb,
            benchmark=args.benchmark,
        )
        return 0
    if args.cmd == "flush":
        row = flush_question(args.workdir, args.question, args.slots, args.memory_mb, args.benchmark)
        print(json.dumps(row))
        return 0
    if args.cmd == "report":
        ids = _read_ids(args.tasks_file)
        measured = noted_peak_gb(args.workdir, measure_container_gb()) if cap else None
        slots, _cpus = eval_slots(
            args.cpu,
            args.benchmark,
            args.workdir,
            resource_cap=cap,
        )
        print(
            parallel_report_line(
                ids,
                args.n_rollouts,
                slots,
                mem_available_kb(),
                measured,
                None,
            )
        )
        return 0
    ids = _read_ids(args.tasks_file)
    unit = next_unit(ids, args.n_rollouts, parse_pairs(args.assigned), parse_pairs(args.inflight))
    if unit is None:
        return 0
    print(f"{unit[0]} {unit[1]}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
