#!/usr/bin/env python3
"""Write one run's artifact.json, summary.md, and report.html, and back them up in the repo."""

from __future__ import annotations

import argparse
import json
import os
import shutil
import tarfile
from datetime import datetime
from pathlib import Path

from eval_metrics import summarize_arm
from score_results import load_json, parse_reward_value, rates_for_trial
from icode_usage import find_icode_usage

_TRIAL_FILES = {
    "result.json",
    "reward.json",
    "usage.json",
    "notes.txt",
    "icode-usage.json",
    "icode.json",
    "icode.txt",
    "timing.json",
    "agent.patch",
    "model.patch",
}


def _resolved(data: dict) -> bool:
    if "reward" in data:
        return parse_reward_value(data.get("reward")) is True
    if "resolved" in data:
        return parse_reward_value(data.get("resolved")) is True
    return False


def _parse_ts(value) -> datetime | None:
    if not isinstance(value, str) or not value.strip():
        return None
    text = value.strip()
    if text.endswith("Z"):
        text = text[:-1] + "+00:00"
    try:
        return datetime.fromisoformat(text)
    except ValueError:
        return None


def _seconds_between(start, finish) -> float | None:
    a = _parse_ts(start)
    b = _parse_ts(finish)
    if a is None or b is None:
        return None
    delta = (b - a).total_seconds()
    if delta < 0:
        return None
    return delta


def _timing_from_result(result: dict) -> tuple[float | None, str | None, str | None]:
    """Prefer the Harbor agent span, then the trial span. Ignore a bare duration_seconds field."""
    if not isinstance(result, dict):
        return None, None, None
    agent = result.get("agent_execution")
    if isinstance(agent, dict):
        dur = _seconds_between(agent.get("started_at"), agent.get("finished_at"))
        if dur is not None:
            return dur, agent.get("started_at"), agent.get("finished_at")
    dur = _seconds_between(result.get("started_at"), result.get("finished_at"))
    if dur is not None:
        started = result.get("started_at")
        finished = result.get("finished_at")
        return dur, started if isinstance(started, str) else None, finished if isinstance(finished, str) else None
    return None, None, None


def _attempt_from_trial(trial: Path, reward_path: Path | None) -> dict:
    data = load_json(reward_path) if reward_path is not None else None
    data = data if isinstance(data, dict) else {}
    rates = rates_for_trial(trial)
    usage = find_icode_usage(trial)
    usage_file = load_json(trial / "usage.json") or {}
    if usage is None and isinstance(usage_file, dict):
        raw = usage_file.get("usage") or {}
        if raw:
            usage = {
                "prompt": int(raw.get("prompt") or 0),
                "completion": int(raw.get("completion") or 0),
            }
    result = load_json(trial / "result.json")
    dur, started, finished = _timing_from_result(result if isinstance(result, dict) else {})
    if dur is None and isinstance(usage_file, dict) and isinstance(usage_file.get("duration_seconds"), (int, float)):
        dur = float(usage_file["duration_seconds"])
    notes = ""
    note_path = trial / "notes.txt"
    if note_path.is_file():
        notes = note_path.read_text(encoding="utf-8").strip()
    has_reward = bool(data) and ("reward" in data or "resolved" in data)
    if reward_path is None:
        has_reward = False
    if not notes and not has_reward:
        notes = "missing reward.json"
    row = {
        "resolved": _resolved(data),
        "has_reward": has_reward,
        "f2p": rates.get("f2p"),
        "f2p_pass": rates.get("f2p_pass"),
        "f2p_total": rates.get("f2p_total"),
        "p2p": rates.get("p2p"),
        "p2p_pass": rates.get("p2p_pass"),
        "p2p_total": rates.get("p2p_total"),
        "partial": rates.get("partial"),
        "tok_in": None if usage is None else int(usage.get("prompt") or 0),
        "tok_out": None if usage is None else int(usage.get("completion") or 0),
        "dur_s": dur,
        "notes": notes,
    }
    if isinstance(started, str):
        row["started_at"] = started
    if isinstance(finished, str):
        row["finished_at"] = finished
    return row


def _missing_attempt() -> dict:
    return {
        "resolved": False,
        "has_reward": False,
        "f2p": None,
        "f2p_pass": None,
        "f2p_total": None,
        "p2p": None,
        "p2p_pass": None,
        "p2p_total": None,
        "partial": None,
        "tok_in": None,
        "tok_out": None,
        "dur_s": None,
        "notes": "missing reward.json",
    }


def harness_task_attempts(harness_dir: Path, tid: str, n_rollouts: int = 0) -> list[dict]:
    """Attempts in `_aNN` order. A missing slot stays put as no-response, not shifted to the end."""
    from score_results import attempt_index, find_harbor_task_dir, trial_dirs

    job = find_harbor_task_dir(harness_dir, tid)
    root = job if job is not None else harness_dir / tid
    indexed: dict[int, dict] = {}
    loose: list[dict] = []
    for trial in trial_dirs(root):
        reward = trial / "verifier" / "reward.json"
        if not reward.is_file():
            reward = trial / "reward.json"
        row = _attempt_from_trial(trial, reward if reward.is_file() else None)
        index = attempt_index(trial)
        if index is not None:
            previous = indexed.get(index)
            if previous is None:
                indexed[index] = row
            else:
                indexed[index] = row
        else:
            loose.append(row)
    if not indexed and not loose:
        return []
    width = n_rollouts if n_rollouts >= 1 else len(indexed) + len(loose)
    rows: list[dict] = []
    for index in range(1, width + 1):
        if index in indexed:
            rows.append(indexed[index])
        elif loose:
            rows.append(loose.pop(0))
        else:
            rows.append(_missing_attempt())
    return rows


def baseline_task_attempts(baseline_dir: Path, tid: str) -> list[dict]:
    task = baseline_dir / tid
    attempts = sorted(p for p in task.glob("attempt-*") if p.is_dir())
    rows = []
    for attempt in attempts:
        reward = attempt / "verifier" / "reward.json"
        if not reward.is_file():
            reward = attempt / "reward.json"
        rows.append(_attempt_from_trial(attempt, reward if reward.is_file() else None))
    if rows:
        return rows
    reward = task / "reward.json"
    if reward.is_file() or (task / "agent.patch").is_file():
        return [_attempt_from_trial(task, reward if reward.is_file() else None)]
    return []


CHAT_API_BASE = "https://api.deepseek.com/v1"


def eval_model_label(model: str) -> str:
    """Artifact label. The HTTP model id stays the catalog id without this prefix."""
    raw = (model or "").strip() or "deepseek-v4-pro"
    if raw.startswith("openai/"):
        return raw
    return f"openai/{raw}"


def build_artifact(
    *,
    suite: str,
    model: str,
    api_base: str,
    task_ids: list[str],
    harness_dir: Path,
    baseline_dir: Path,
    n_rollouts: int,
    concurrency: int,
    cpus_each: int,
    run_id: str,
) -> dict:
    del baseline_dir
    icode_rows = [(tid, harness_task_attempts(harness_dir, tid, n_rollouts)) for tid in task_ids]
    arm = summarize_arm(icode_rows, n_rollouts, concurrency)
    progress_path = harness_dir / "progress.json"
    if progress_path.is_file():
        prog = load_json(progress_path)
        if isinstance(prog, dict):
            done = prog.get("done")
            needed = prog.get("needed")
            if isinstance(done, int) and isinstance(needed, int) and done < needed:
                note = prog.get("eta_note")
                if isinstance(note, str) and note.strip():
                    timing = arm.get("timing")
                    if not isinstance(timing, dict):
                        timing = {}
                        arm["timing"] = timing
                    timing["eta_note"] = note
    return {
        "suite": suite,
        "model": model,
        "api_base": api_base,
        "run_id": run_id,
        "run_label": run_label_for(suite, run_id),
        "n_tasks": len(task_ids),
        "n_rollouts": n_rollouts,
        "concurrency": concurrency,
        "cpus_each": cpus_each,
        "icode": arm,
    }


def _pct(value) -> str:
    if not isinstance(value, (int, float)) or isinstance(value, bool):
        return "-"
    return f"{float(value) * 100:.1f}%"


def _num(value, digits=4) -> str:
    if not isinstance(value, (int, float)) or isinstance(value, bool):
        return "-"
    return f"{float(value):.{digits}f}"


def _count(rate, passed, total) -> str:
    if not isinstance(rate, (int, float)) or isinstance(rate, bool):
        return "-"
    if isinstance(passed, int) and isinstance(total, int):
        return f"{float(rate):.4f} ({passed}/{total})"
    return f"{float(rate):.4f}"


def _fmt_token(value) -> str:
    if not isinstance(value, (int, float)) or isinstance(value, bool):
        return "-"
    n = float(value)
    sign = "-" if n < 0 else ""
    n = abs(n)
    if n >= 1_000_000:
        text = f"{n / 1_000_000:.2f}".rstrip("0").rstrip(".")
        return sign + text + "M"
    if n >= 1_000:
        text = f"{n / 1_000:.1f}".rstrip("0").rstrip(".")
        return sign + text + "k"
    if n == int(n):
        return sign + str(int(n))
    return sign + f"{n:.1f}"


def _dur_cell(value) -> str:
    if not isinstance(value, (int, float)) or isinstance(value, bool):
        return "-"
    return str(int(round(float(value))))


def _dur_label(value) -> str:
    cell = _dur_cell(value)
    if cell == "-":
        return "-"
    return cell + "s"


def _span_label(value) -> str:
    """Wall and per-rollout text: 45s, 15m, 2h 5m, 1d 6h."""
    if not isinstance(value, (int, float)) or isinstance(value, bool):
        return "-"
    seconds = int(round(float(value)))
    if seconds < 0:
        seconds = 0
    if seconds < 60:
        return f"{seconds}s"
    if seconds < 3600:
        return f"{seconds // 60}m"
    if seconds < 86400:
        hours, minutes = divmod(seconds, 3600)
        minutes //= 60
        if minutes == 0:
            return f"{hours}h"
        return f"{hours}h {minutes}m"
    days, hours = divmod(seconds, 86400)
    hours //= 3600
    if hours == 0:
        return f"{days}d"
    return f"{days}d {hours}h"


def _suite_title(suite) -> str:
    names = {"deepswe": "DeepSWE", "swebenchpro": "SWE-bench Pro", "lolbench": "LOLBench"}
    text = "" if suite is None else str(suite)
    return names.get(text.strip().lower(), text or "-")


def run_label_for(suite: str, run_id: str) -> str:
    text = str(run_id or "")
    prefix = "jenkins-"
    if text.startswith(prefix) and text[len(prefix) :]:
        return f"{suite}-{text[len(prefix) :]}"
    return text


def _micro_count(rate, passed, total) -> str:
    if not isinstance(rate, (int, float)) or isinstance(rate, bool):
        return "-"
    if isinstance(passed, int) and isinstance(total, int) and total >= 1000:
        return f"**{float(rate):.4f}** ({_fmt_token(passed)}/{_fmt_token(total)})"
    if isinstance(passed, int) and isinstance(total, int):
        return f"**{float(rate):.4f}** ({passed}/{total})"
    return f"**{float(rate):.4f}**"


def _md_table(headers: list[str], rows: list[list[str]]) -> list[str]:
    cells = [headers] + rows
    widths = [max(len(row[i]) for row in cells) for i in range(len(headers))]

    def fmt(row: list[str]) -> str:
        return "| " + " | ".join(row[i].ljust(widths[i]) for i in range(len(row))) + " |"

    sep = "| " + " | ".join("-" * widths[i] for i in range(len(headers))) + " |"
    return [fmt(headers), sep] + [fmt(row) for row in rows]


def summary_markdown(doc: dict) -> str:
    suite = doc.get("suite")
    title = _suite_title(suite)
    n_tasks = doc.get("n_tasks")
    k = int(doc.get("n_rollouts") or 1)
    label = doc.get("run_label") or run_label_for(str(suite or ""), str(doc.get("run_id") or ""))
    lines = [f"# Eval metrics summary — {suite}", ""]
    lines.append(f"- Suite: `{suite}`")
    lines.append(f"- Run: `{label}`")
    lines.append(f"- Run dir: `{doc.get('run_dir') or '-'}`")
    lines.append(f"- Eval model: `{doc.get('model')}`")
    lines.append(f"- Eval API base: `{doc.get('api_base')}`")
    lines.append("")
    arm = doc.get("icode")
    if isinstance(arm, dict):
        tasks = arm.get("tasks") or []
        scored = arm.get("scored_rollouts")
        excluded = arm.get("infra_excluded")
        if not isinstance(excluded, int) or isinstance(excluded, bool):
            excluded = 0
        metrics_n = len(tasks) if isinstance(n_tasks, int) else len(tasks)
        if isinstance(n_tasks, int) and not isinstance(n_tasks, bool):
            metrics_n = n_tasks
        lines.append(f"- Tasks with metrics: **{metrics_n}** (missing/excluded: {excluded})")
        sd = arm.get("macro_pass@1_sd")
        sd_txt = "" if not isinstance(sd, (int, float)) or isinstance(sd, bool) else f" ±{float(sd) * 100:.1f} pp (SD)"
        sd_s = arm.get("macro_pass@1_scored_sd")
        sd_s_txt = "" if not isinstance(sd_s, (int, float)) or isinstance(sd_s, bool) else f" ±{float(sd_s) * 100:.1f} pp (SD)"
        padded_bits = [f"Pass@{i} **{_pct(arm.get(f'pass@{i}'))}**" for i in range(1, k + 1)]
        scored_bits = [f"Pass@{i} **{_pct(arm.get(f'pass@{i}_scored'))}**" for i in range(1, k + 1)]
        lines.append(
            f"- {title} Pass@1..k (padded, missing=fail): " + " · ".join(padded_bits)
        )
        lines.append(
            f"- {title} Pass@1..k (scored-only, missing omitted): " + " · ".join(scored_bits)
        )
        lines.append(
            f"- {title} macro Pass@1 (padded, mean of c/n): **{_pct(arm.get('macro_pass@1'))}**{sd_txt}"
        )
        lines.append(
            f"- {title} macro Pass@1 (scored-only, mean of c_scored/n_scored): **{_pct(arm.get('macro_pass@1_scored'))}**{sd_s_txt}"
        )
        hits = arm.get("best_attempt_hits")
        if not isinstance(hits, int) or isinstance(hits, bool):
            hits = sum(1 for row in tasks if row.get("best"))
        lines.append(
            f"- {title} detail — first-rollout (= padded Pass@1): **{_pct(arm.get('pass@1'))}** · any-pass: **{_pct(arm.get('any_pass'))}** · scored rollouts: **{scored}** · infra-excluded: **{excluded}**"
        )
        lines.append(f"- Best-attempt pass (secondary): **{hits}** / {metrics_n} ({_pct(arm.get('any_pass'))})")
        macro = arm.get("macro") or {}
        micro = arm.get("micro") or {}
        lines.append(
            f"- Macro avg — F2P: **{_num(macro.get('f2p'))}** · P2P: **{_num(macro.get('p2p'))}** · partial: **{_num(macro.get('partial'))}**"
        )
        lines.append(
            "- Micro — F2P: "
            + _micro_count(micro.get("f2p"), micro.get("f2p_pass"), micro.get("f2p_total"))
            + " · P2P: "
            + _micro_count(micro.get("p2p"), micro.get("p2p_pass"), micro.get("p2p_total"))
            + " · partial: "
            + _micro_count(micro.get("partial"), micro.get("partial_pass"), micro.get("partial_total"))
        )
        timing = arm.get("timing") or {}
        mem_peak = doc.get("container_mem_max_gb")
        if isinstance(mem_peak, (int, float)) and not isinstance(mem_peak, bool):
            lines.append(f"- Container memory — max peak: **{float(mem_peak):.2f} GB**")
        skipped = doc.get("skipped_questions")
        if isinstance(skipped, list) and skipped:
            lines.append("- Skipped questions: " + ", ".join(str(item) for item in skipped))
        else:
            lines.append("- Skipped questions: none")
        lines.append(f"- Median best-attempt duration: **{_dur_label(timing.get('median_best_attempt_s'))}**")
        expected = metrics_n * k if isinstance(metrics_n, int) else None
        wall_txt = _span_label(timing.get("wall_seconds"))
        if isinstance(scored, int) and isinstance(expected, int) and expected > 0 and scored >= expected:
            wall_txt = f"{wall_txt} (complete)"
        lines.append(
            f"- Timing — wall: **{wall_txt}** · concurrency: **{doc.get('concurrency')}** · rollouts/task: **{k}**"
        )
        progress = "-"
        if isinstance(scored, int) and isinstance(expected, int) and expected > 0:
            done = metrics_n - excluded if isinstance(metrics_n, int) else metrics_n
            progress = f"**{scored}/{expected} rollouts ({scored / expected * 100:.1f}%) · {done}/{metrics_n} tasks done**"
        lines.append(
            f"- Timing — mean/median per rollout: **{_span_label(timing.get('mean_rollout_s'))}/{_span_label(timing.get('median_rollout_s'))}** · progress: {progress}"
        )
        tokens = arm.get("tokens") or {}
        avg = tokens.get("avg_total_per_task")
        if avg is None and isinstance(tokens.get("total"), int) and isinstance(tokens.get("tasks_with_data"), int) and tokens.get("tasks_with_data"):
            avg = tokens["total"] / tokens["tasks_with_data"]
        token_sum = f"{_fmt_token(tokens.get('in'))}/{_fmt_token(tokens.get('out'))}/{_fmt_token(tokens.get('total'))}"
        lines.append(
            f"- Tokens — tasks with data: **{tokens.get('tasks_with_data')}** · total in/out/sum: **{token_sum}** · avg total/task: **{_fmt_token(avg)}**"
        )
        lines.append("")
        headers = ["#", "task", "c/n", "c_scored/n_scored", "pass_frac", "first", "best", "reward", "f2p", "p2p", "partial", "tok_in/out", "dur_s", "notes"]
        table_rows = []
        ordered = sorted(arm["tasks"], key=lambda row: (-row["pass_frac"], row["id"]))
        for i, row in enumerate(ordered, start=1):
            tok = "-" if row.get("tok_in") is None else f"{_fmt_token(row.get('tok_in'))}/{_fmt_token(row.get('tok_out'))}"
            c_s = row.get("c_scored")
            n_s = row.get("n_scored")
            scored_cn = f"{c_s}/{n_s}" if isinstance(c_s, int) and isinstance(n_s, int) else "-"
            table_rows.append(
                [
                    str(i),
                    str(row["id"]),
                    f"{row['c']}/{row['n']}",
                    scored_cn,
                    _num(row["pass_frac"]),
                    "yes" if row["first"] else "no",
                    "yes" if row["best"] else "no",
                    _num(row["reward"]),
                    _count(row.get("f2p"), row.get("f2p_pass"), row.get("f2p_total")),
                    _count(row.get("p2p"), row.get("p2p_pass"), row.get("p2p_total")),
                    _num(row.get("partial")),
                    tok,
                    _dur_cell(row.get("dur_s")),
                    str(row.get("notes") or "-").replace("|", "/"),
                ]
            )
        lines.extend(_md_table(headers, table_rows))
        lines.append("")
        unscored_tasks = arm.get("unscored_tasks") if isinstance(arm.get("unscored_tasks"), list) else []
        unscored_tasks = [row for row in unscored_tasks if isinstance(row, dict)]
        if not unscored_tasks:
            lines.append("unscored rollouts: 0")
            lines.append("")
        else:
            lines.append("## Unscored / no-response (not in Pass@k as a scored fail)")
            lines.append("")
            lines.append("Tasks with at least one attempt that returned no reward. High rates here can move padded Pass@k.")
            lines.append("")
            u_rows = []
            for row in unscored_tasks:
                u_rows.append(
                    [
                        str(row.get("id") or ""),
                        f"{row.get('unscored')}/{row.get('n')}",
                        str(row.get("notes") or "-").replace("|", "/"),
                    ]
                )
            lines.extend(_md_table(["Task", "unscored/n", "notes"], u_rows))
            lines.append("")
    return "\n".join(lines) + "\n"


def memory_sidecar(harness_dir: Path) -> tuple[float | None, list[str]]:
    peak = None
    jsonl = harness_dir / "container_mem.jsonl"
    if jsonl.is_file():
        for line in jsonl.read_text(encoding="utf-8").splitlines():
            line = line.strip()
            if not line:
                continue
            try:
                row = json.loads(line)
                gb = float(row.get("peak_gb") or 0)
            except (json.JSONDecodeError, TypeError, ValueError):
                continue
            peak = gb if peak is None else max(peak, gb)
    skipped_path = harness_dir / "skipped_questions.txt"
    skipped: list[str] = []
    if skipped_path.is_file():
        skipped = [ln.strip() for ln in skipped_path.read_text(encoding="utf-8").splitlines() if ln.strip()]
    return peak, skipped


def copy_memory_sidecars(harness_dir: Path, out_dir: Path) -> None:
    for name in ("container_mem.jsonl", "skipped_questions.txt"):
        src = harness_dir / name
        if src.is_file():
            (out_dir / name).write_text(src.read_text(encoding="utf-8"), encoding="utf-8")


def write_report(doc: dict, out_dir: Path) -> Path:
    out_dir.mkdir(parents=True, exist_ok=True)
    path = out_dir / "artifact.json"
    path.write_text(json.dumps(doc, indent=2) + "\n", encoding="utf-8")
    (out_dir / "summary.md").write_text(summary_markdown(doc), encoding="utf-8")
    from report_html import report_html

    (out_dir / "report.html").write_text(report_html(doc), encoding="utf-8")
    return path


def run_folder_name(utc: str, task_ids: list[str], build_number: str = "") -> str:
    number = build_number.strip()
    if number:
        return f"jenkins-{number}-{utc}"
    if len(task_ids) == 1:
        safe = task_ids[0].replace("/", "_").replace("\\", "_")
        return f"{utc}-{safe}"
    return f"{utc}-n{len(task_ids)}"


def default_backup_root() -> Path:
    override = os.environ.get("MAC_K3D_OUTPUT_ROOT")
    if override:
        return Path(override)
    repo = Path(__file__).resolve().parents[2]
    if (repo / "Cargo.toml").is_file() and (repo / "pipeline").is_dir():
        return repo / "output"
    home = Path.home()
    for pattern in (
        "mac-k3d/Cargo.toml",
        "*/mac-k3d/Cargo.toml",
        "*/*/mac-k3d/Cargo.toml",
        "*/*/*/mac-k3d/Cargo.toml",
    ):
        for cargo in home.glob(pattern):
            if cargo.is_file() and (cargo.parent / "pipeline").is_dir():
                return cargo.parent / "output"
    return repo / "output"


def _copy_trial_files(trial: Path, dest: Path) -> None:
    if not trial.is_dir():
        return
    dest.mkdir(parents=True, exist_ok=True)
    for path in trial.rglob("*"):
        if not path.is_file():
            continue
        if path.name == ".harbor-env" or ".harbor-env" in path.parts:
            continue
        if path.name not in _TRIAL_FILES and path.suffix != ".patch":
            continue
        target = dest / path.relative_to(trial)
        target.parent.mkdir(parents=True, exist_ok=True)
        shutil.copy2(path, target)


def backup_run(
    *,
    backup_root: Path,
    suite: str,
    task_ids: list[str],
    report_dir: Path,
    run_folder: str,
    harness_dir: Path,
    baseline_dir: Path,
) -> Path:
    del baseline_dir
    dest = backup_root / suite / run_folder
    dest.mkdir(parents=True, exist_ok=True)
    for name in ("artifact.json", "summary.md", "report.html"):
        src = report_dir / name
        if src.is_file():
            shutil.copy2(src, dest / name)
    (dest / "tasks.txt").write_text("".join(f"{tid}\n" for tid in task_ids), encoding="utf-8")
    from score_results import find_harbor_task_dir, trial_dirs

    for tid in task_ids:
        job = find_harbor_task_dir(harness_dir, tid)
        root = job if job is not None else harness_dir / tid
        for index, trial in enumerate(trial_dirs(root), start=1):
            _copy_trial_files(trial, dest / "icode" / tid / f"attempt-{index:02d}")
    return dest


def compress_backup(dest: Path) -> Path:
    """Pack a run folder into ``<folder>.tar.gz`` and remove the loose directory.

    The archive's top entry is the folder name, so extracting recreates the
    same layout. A failed pack deletes the temporary archive and leaves the
    folder in place.
    """
    archive = dest.parent / f"{dest.name}.tar.gz"
    tmp = dest.parent / f".{dest.name}.tar.gz.tmp"
    try:
        with tarfile.open(tmp, "w:gz") as tar:
            tar.add(dest, arcname=dest.name)
        tmp.replace(archive)
    except Exception:
        tmp.unlink(missing_ok=True)
        raise
    shutil.rmtree(dest)
    return archive


def demo_artifact() -> dict:
    def attempts(pattern: list[bool]) -> list[dict]:
        rows = []
        for i, ok in enumerate(pattern):
            rows.append(
                {
                    "resolved": ok,
                    "f2p": 1.0 if ok else 0.5,
                    "f2p_pass": 2 if ok else 1,
                    "f2p_total": 2,
                    "p2p": 1.0,
                    "p2p_pass": 4,
                    "p2p_total": 4,
                    "partial": 1.0 if ok else 5 / 6,
                    "tok_in": 1000 * (i + 1),
                    "tok_out": 10 * (i + 1),
                    "dur_s": 100.0 + i,
                    "notes": "",
                }
            )
        return rows

    icode = [
        ("alpha", attempts([True, True, False, True])),
        ("beta", attempts([False, True, False, False])),
        ("gamma", attempts([False, False, False, False])),
    ]
    return {
        "suite": "deepswe",
        "model": "openai/deepseek-flash",
        "api_base": CHAT_API_BASE,
        "run_id": "jenkins-25",
        "run_label": "deepswe-25",
        "run_dir": "output/deepswe/jenkins-25-demo",
        "n_tasks": 3,
        "n_rollouts": 4,
        "concurrency": 4,
        "cpus_each": 1,
        "icode": summarize_arm(icode, 4, 4),
    }


def main() -> int:
    ap = argparse.ArgumentParser()
    ap.add_argument("--demo", action="store_true")
    ap.add_argument("--out-dir", default="")
    ap.add_argument("--harness-dir", default="")
    ap.add_argument("--baseline-dir", default="")
    ap.add_argument("--task-file", default="")
    ap.add_argument("--suite", default=os.environ.get("BENCHMARK", "deepswe"))
    ap.add_argument("--model", default=os.environ.get("DEEPSEEK_MODEL", "deepseek-v4-pro"))
    ap.add_argument("--api-base", default=os.environ.get("DEEPSEEK_API_BASE", CHAT_API_BASE))
    ap.add_argument("--run-id", default=os.environ.get("BUILD_NUMBER", "local"))
    ap.add_argument("--n-rollouts", type=int, default=0)
    ap.add_argument("--concurrency", type=int, default=0)
    ap.add_argument("--cpus-each", type=int, default=0)
    ap.add_argument("--utc", default="")
    ap.add_argument("--run-folder", default="")
    ap.add_argument("--backup-root", default="")
    args = ap.parse_args()
    if args.demo:
        doc = demo_artifact()
        out = Path(args.out_dir or "/tmp/mac-k3d-report-demo")
        path = write_report(doc, out)
        print(f"wrote {path}")
        return 0
    n_rollouts = args.n_rollouts or int(os.environ.get("N_ROLLOUTS") or "1")
    concurrency = args.concurrency or int(os.environ.get("EVAL_PARALLEL") or "1")
    cpus_each = args.cpus_each or int(os.environ.get("EVAL_CPUS_EACH") or "1")
    ids = [ln.strip() for ln in Path(args.task_file).read_text(encoding="utf-8").splitlines() if ln.strip()]
    run_id = args.run_id
    if run_id not in ("local",) and not str(run_id).startswith(("jenkins-", "local-")):
        run_id = f"jenkins-{run_id}"
    mem_peak, skipped = memory_sidecar(Path(args.harness_dir))
    doc = build_artifact(
        suite=args.suite,
        model=eval_model_label(args.model),
        api_base=args.api_base,
        task_ids=ids,
        harness_dir=Path(args.harness_dir),
        baseline_dir=Path(args.baseline_dir),
        n_rollouts=n_rollouts,
        concurrency=concurrency,
        cpus_each=cpus_each,
        run_id=run_id,
    )
    utc = args.utc or datetime.utcnow().strftime("%Y%m%dT%H%M%SZ")
    folder = args.run_folder or run_folder_name(utc, ids, os.environ.get("BUILD_NUMBER", ""))
    doc["run_dir"] = f"output/{args.suite}/{folder}.tar.gz"
    doc["container_mem_max_gb"] = mem_peak
    doc["skipped_questions"] = skipped
    out = Path(args.out_dir) if args.out_dir else Path("eval-runs") / "output" / args.suite
    if out.name != folder:
        out = out / folder
    path = write_report(doc, out)
    copy_memory_sidecars(Path(args.harness_dir), out)
    backup_root = Path(args.backup_root) if args.backup_root else default_backup_root()
    dest = backup_run(
        backup_root=backup_root,
        suite=args.suite,
        task_ids=ids,
        report_dir=out,
        run_folder=folder,
        harness_dir=Path(args.harness_dir),
        baseline_dir=Path(args.baseline_dir),
    )
    archive = compress_backup(dest)
    print(f"wrote {path}")
    print(f"backup {archive}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
