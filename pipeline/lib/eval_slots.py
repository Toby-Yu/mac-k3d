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
# Conservative per-container estimate when no live/history sample exists.
# LoLBench Flink/Java images can exceed 6 GB; without a cap, 4× peaks OOM the
# host and kill the Jenkins agent (durable task exit -1).
FALLBACK_GB = {
    "swebenchpro": 8,
    "lolbench": 4.0,
    "deepswe": 1.5,
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
    # Drop run-global peak so the next question plans from its own history.
    peak_path = _mem_dir(workdir) / PEAK_NAME
    if peak_path.is_file():
        try:
            peak_path.unlink()
        except OSError:
            pass
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


def history_peak_gb(workdir: str | Path, question: str) -> float | None:
    """Best prior peak_gb for this question from container_mem_history.jsonl."""
    if not question:
        return None
    path = _mem_dir(workdir) / HISTORY_NAME
    if not path.is_file():
        return None
    best: float | None = None
    try:
        for line in path.read_text(encoding="utf-8", errors="replace").splitlines():
            line = line.strip()
            if not line:
                continue
            try:
                doc = json.loads(line)
            except json.JSONDecodeError:
                continue
            if doc.get("question") != question:
                continue
            sample = usable_sample_gb(doc.get("peak_gb"))
            if sample is None:
                continue
            best = sample if best is None else max(best, sample)
    except OSError:
        return None
    return best


def estimate_container_gb(
    workdir: str | Path,
    benchmark: str,
    *,
    question: str = "",
    measured_gb: float | None = None,
) -> float | None:
    """Prefer live sample, then per-question history, else benchmark fallback.

    When ``question`` is set, skip the run-global noted peak so a heavy prior
    wave (e.g. Flink) does not pin later light questions to one slot.
    """
    live = usable_sample_gb(measured_gb)
    if live is not None:
        return live
    if question:
        hist = history_peak_gb(workdir, question)
        if hist is not None:
            return hist
    else:
        noted = usable_sample_gb(noted_peak_gb(workdir, None))
        if noted is not None:
            return noted
        # No question id: still try history is impossible; use fallback.
    fb = FALLBACK_GB.get((benchmark or "deepswe").strip().lower())
    return float(fb) if fb is not None else None


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
    question: str = "",
) -> tuple[int, int]:
    """Return (EVAL_SLOTS, EVAL_CPUS_EACH).

    Cap slots by free disk and by RAM / per-container budget so a full suite
    cannot OOM-kill the Jenkins agent (see flink_1 @ ~6.4 GB × 4).
    """
    del measure
    slots = max(int(cpu_lock), 1)
    if resource_cap:
        if not disk_ok(workdir):
            slots = 1
        estimated = estimate_container_gb(
            workdir,
            benchmark,
            question=question,
            measured_gb=container_gb,
        )
        ram = ram_slots(benchmark, mem_kb=mem_kb, measured_gb=estimated)
        if ram is not None:
            slots = min(slots, ram)
    cpus_each = max(1, int(cpu_lock) // slots)
    return slots, cpus_each


def memory_limit_mb(
    mem_kb: int | None,
    slots: int,
    measured_gb: float | None,
) -> int:
    """Per-container Docker memory cap (MB) from history/estimate, host-capped.

    Prefer ``peak × MEM_BUFFER`` (or benchmark fallback) so light questions get
    a tight cap and more slots; never exceed the equal host share so a wave
    cannot OOM-kill the Jenkins agent.
    """
    kb = mem_available_kb() if mem_kb is None else mem_kb
    if kb is None or kb <= 0:
        return 0
    n = max(int(slots), 1)
    avail_mb = int(kb / 1024) - int(HOST_RESERVE_GB * 1024)
    if avail_mb < 512:
        host_share = 512
    else:
        host_share = max(512, avail_mb // n)
    need_mb = host_share
    if measured_gb is not None and measured_gb > 0:
        need_mb = max(512, int(measured_gb * MEM_BUFFER * 1024 + 0.999))
    per = min(need_mb, host_share)
    return int(per)


def format_task_ids(task_ids: list[str]) -> str:
    ids = [tid for tid in task_ids if tid]
    if not ids:
        return "-"
    if len(ids) <= 8:
        return ",".join(ids)
    return f"{ids[0]}..{ids[-1]} (full list in selected_tasks.txt)"


_ATTEMPT_RE = re.compile(r"_a(\d+)(?:_|$)")


def attempt_index_from_path(path: Path) -> int | None:
    """1-based attempt from Harbor job path segments like ``task_icode_23_a02``."""
    for part in path.parts:
        match = _ATTEMPT_RE.search(part)
        if match:
            number = int(match.group(1))
            if number >= 1:
                return number
    return None


def list_harbor_build_dirs(workdir: str | Path) -> list[Path]:
    runs = _mem_dir(workdir) / "harbor_runs"
    if not runs.is_dir():
        return []
    found: list[Path] = []
    try:
        for child in runs.iterdir():
            if child.is_dir() and child.name.startswith("jenkins-"):
                found.append(child)
    except OSError:
        return []
    return found


def resolve_resume_build(workdir: str | Path, current_build: str = "") -> str:
    """Pick which harbor_runs/jenkins-* tree to seed from.

    Prefer ``RESUME_FROM`` / explicit current build when that tree exists; else the
    newest prior ``jenkins-*`` dir (typical after agent death + a new build number).
    """
    explicit = (os.environ.get("RESUME_FROM") or "").strip()
    if explicit:
        name = explicit if explicit.startswith("jenkins-") else f"jenkins-{explicit}"
        path = _mem_dir(workdir) / "harbor_runs" / name
        if path.is_dir():
            return name
    cur = (current_build or os.environ.get("BUILD_NUMBER") or "").strip()
    if cur and cur != "local":
        name = cur if cur.startswith("jenkins-") else f"jenkins-{cur}"
        path = _mem_dir(workdir) / "harbor_runs" / name
        if path.is_dir() and any(path.rglob("reward.json")):
            return name
    builds = list_harbor_build_dirs(workdir)
    if not builds:
        return ""

    def _mtime(path: Path) -> float:
        newest = -1.0
        try:
            newest = max(newest, path.stat().st_mtime)
        except OSError:
            pass
        try:
            for reward in path.rglob("reward.json"):
                try:
                    newest = max(newest, reward.stat().st_mtime)
                except OSError:
                    pass
        except OSError:
            pass
        return newest

    # Prefer a prior build (not the empty new jenkins-$BUILD_NUMBER).
    candidates = builds
    if cur and cur != "local":
        skip = cur if cur.startswith("jenkins-") else f"jenkins-{cur}"
        prior = [p for p in builds if p.name != skip]
        if prior:
            candidates = prior
    best = max(candidates, key=_mtime)
    return best.name


def completed_units(
    workdir: str | Path,
    task_ids: list[str],
    n_rollouts: int,
    *,
    build: str = "",
) -> list[str]:
    """Units ``tid:attempt`` that already have reward.json under harbor_runs/.

    When ``build`` is set (e.g. ``jenkins-23``), only that jobs tree is scanned so
    an aborted suite does not inherit rewards from older campaigns.
    """
    runs = _mem_dir(workdir) / "harbor_runs"
    if build:
        name = build if build.startswith("jenkins-") else f"jenkins-{build}"
        runs = runs / name
    if not runs.is_dir() or n_rollouts < 1:
        return []
    found: set[str] = set()
    want = {tid for tid in task_ids if tid}
    try:
        for reward in runs.rglob("reward.json"):
            if not reward.is_file():
                continue
            attempt = attempt_index_from_path(reward)
            if attempt is None or attempt > n_rollouts:
                continue
            tid = ""
            for part in reward.parts:
                if part in want:
                    tid = part
                    break
            if not tid:
                continue
            found.add(f"{tid}:{attempt}")
    except OSError:
        return []
    out: list[str] = []
    for tid in task_ids:
        if not tid:
            continue
        for attempt in range(1, n_rollouts + 1):
            key = f"{tid}:{attempt}"
            if key in found:
                out.append(key)
    return out


def remaining_questions(
    task_ids: list[str],
    n_rollouts: int,
    assigned: set[tuple[str, int]] | list[tuple[str, int]],
) -> list[str]:
    taken = set(assigned)
    out: list[str] = []
    for tid in task_ids:
        if not tid:
            continue
        if any((tid, attempt) not in taken for attempt in range(1, n_rollouts + 1)):
            out.append(tid)
    return out


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
    resume_p = sub.add_parser("resume-seed")
    resume_p.add_argument("--tasks-file", required=True)
    resume_p.add_argument("--n-rollouts", type=int, required=True)
    resume_p.add_argument("--workdir", default=os.environ.get("WORKDIR", "."))
    resume_p.add_argument(
        "--build",
        default="",
        help="harbor_runs/jenkins-N tree to seed from (default: resolve_resume_build)",
    )
    args = ap.parse_args()
    cap = os.environ.get("EVAL_RESOURCE_CAP", "1") != "0"
    if args.cmd == "slots":
        sample = measure_container_gb() if cap else None
        question = (args.question or "").strip()
        estimated = (
            estimate_container_gb(
                args.workdir,
                args.benchmark,
                question=question,
                measured_gb=sample,
            )
            if cap
            else None
        )
        slots, cpus = eval_slots(
            args.cpu,
            args.benchmark,
            args.workdir,
            resource_cap=cap,
            container_gb=sample,
            question=question,
        )
        mem_mb = memory_limit_mb(None, slots, estimated) if cap else 0
        print(f"EVAL_SLOTS={slots}")
        print(f"EVAL_CPUS_EACH={cpus}")
        print("EVAL_FITS=1")
        print(f"EVAL_MEMORY_MB={mem_mb}")
        if question:
            observe_question(args.workdir, question, sample, slots, mem_mb, args.benchmark)
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
    if args.cmd == "resume-seed":
        ids = _read_ids(args.tasks_file)
        build = (args.build or "").strip() or resolve_resume_build(args.workdir)
        done = completed_units(args.workdir, ids, args.n_rollouts, build=build)
        assigned_pairs = parse_pairs(",".join(done))
        remaining = remaining_questions(ids, args.n_rollouts, assigned_pairs)
        print(f"RESUME_BUILD={build}")
        print(f"ASSIGNED={','.join(done)}")
        print(f"RESUME_DONE={len(done)}")
        print(f"RESUME_REMAINING={','.join(remaining)}")
        return 0
    if args.cmd == "report":
        ids = _read_ids(args.tasks_file)
        measured = noted_peak_gb(args.workdir, measure_container_gb()) if cap else None
        upcoming = ids[0] if ids else ""
        # Prefer history for the first question; ignore stale global peak.
        estimated = (
            estimate_container_gb(
                args.workdir,
                args.benchmark,
                question=upcoming,
                measured_gb=measure_container_gb() if cap else None,
            )
            if cap
            else None
        )
        del measured
        slots, _cpus = eval_slots(
            args.cpu,
            args.benchmark,
            args.workdir,
            resource_cap=cap,
            container_gb=estimated,
            question=upcoming,
        )
        print(
            parallel_report_line(
                ids,
                args.n_rollouts,
                slots,
                mem_available_kb(),
                estimated,
                budget_gb(args.benchmark, estimated) if cap else None,
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
