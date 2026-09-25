#!/usr/bin/env python3
"""Suite metrics for one arm: Pass@1..k, macro/micro rates, tokens, and duration."""

from __future__ import annotations

from datetime import datetime
from statistics import stdev


PASS_METHODS = {
    "padded": "missing attempt counts as not resolved; n is N_ROLLOUTS",
    "scored": "only attempts with reward.json; missing omitted from the denominator",
}


def pad_flags(flags: list[bool], k: int) -> list[bool]:
    out = list(flags[:k])
    while len(out) < k:
        out.append(False)
    return out


def attempt_is_scored(row: dict) -> bool:
    """True when the attempt has a verifier outcome (reward.json)."""
    notes = str(row.get("notes") or "")
    if "missing reward.json" in notes:
        return False
    if row.get("has_reward") is False:
        return False
    if row.get("has_reward") is True:
        return True
    if row.get("f2p") is not None:
        return True
    return "resolved" in row


def pass_at_ladder(per_task_flags: list[list[bool]], k: int) -> dict[str, float]:
    """Pass@i is the share of questions with a pass in the first i attempts (padded)."""
    n = len(per_task_flags)
    ladder: dict[str, float] = {}
    for i in range(1, k + 1):
        if n == 0:
            ladder[f"pass@{i}"] = 0.0
            continue
        hits = sum(1 for flags in per_task_flags if any(pad_flags(flags, k)[:i]))
        ladder[f"pass@{i}"] = hits / n
    return ladder


def pass_at_ladder_scored(per_task_scored_flags: list[list[bool]], k: int) -> dict[str, float]:
    """Pass@i over scored attempts only. Questions with zero scored flags are skipped."""
    usable = [flags for flags in per_task_scored_flags if flags]
    n = len(usable)
    ladder: dict[str, float] = {}
    for i in range(1, k + 1):
        if n == 0:
            ladder[f"pass@{i}_scored"] = 0.0
            continue
        hits = sum(1 for flags in usable if any(flags[:i]))
        ladder[f"pass@{i}_scored"] = hits / n
    return ladder


def mean_sd(vals: list[float]) -> tuple[float | None, float | None]:
    """Mean and sample standard deviation. One value has SD 0."""
    if not vals:
        return None, None
    mean = sum(vals) / len(vals)
    if len(vals) < 2:
        return mean, 0.0
    return mean, stdev(vals)


def _num(value) -> float | None:
    if isinstance(value, bool) or not isinstance(value, (int, float)):
        return None
    return float(value)


def _best_index(attempts: list[dict]) -> int | None:
    if not attempts:
        return None
    best_i = 0
    best_key = (-1, -1.0)
    for i, row in enumerate(attempts):
        resolved = 1 if row.get("resolved") else 0
        partial = _num(row.get("partial"))
        key = (resolved, partial if partial is not None else -1.0)
        if key > best_key:
            best_key = key
            best_i = i
    return best_i


def task_row(task_id: str, attempts: list[dict], n_rollouts: int) -> dict:
    flags = pad_flags([bool(row.get("resolved")) for row in attempts], n_rollouts)
    c = sum(1 for flag in flags if flag)
    n = n_rollouts
    pass_frac = (c / n) if n else 0.0
    first = bool(flags[0]) if flags else False
    best = any(flags)
    scored_flags = [bool(row.get("resolved")) for row in attempts if attempt_is_scored(row)]
    c_scored = sum(1 for flag in scored_flags if flag)
    n_scored = len(scored_flags)
    pass_frac_scored = (c_scored / n_scored) if n_scored else 0.0
    missing = max(0, n_rollouts - len(attempts))
    unscored_existing = sum(1 for row in attempts if not attempt_is_scored(row))
    unscored = unscored_existing + missing
    unscored_frac = (unscored / n) if n else 0.0
    chosen = _best_index(attempts)
    chosen_row = attempts[chosen] if chosen is not None else {}
    tok_in = tok_out = None
    if any(_num(row.get("tok_in")) is not None or _num(row.get("tok_out")) is not None for row in attempts):
        tok_in = sum(int(_num(row.get("tok_in")) or 0) for row in attempts)
        tok_out = sum(int(_num(row.get("tok_out")) or 0) for row in attempts)
    durs = [_num(row.get("dur_s")) for row in attempts]
    durs = [d for d in durs if d is not None]
    best_dur = _num(chosen_row.get("dur_s")) if chosen_row else None
    notes = [str(row.get("notes")) for row in attempts if row.get("notes")]
    return {
        "id": task_id,
        "c": c,
        "n": n,
        "pass_frac": pass_frac,
        "c_scored": c_scored,
        "n_scored": n_scored,
        "pass_frac_scored": pass_frac_scored,
        "unscored": unscored,
        "unscored_frac": unscored_frac,
        "first": first,
        "best": best,
        "reward": 1.0 if best else 0.0,
        "f2p": chosen_row.get("f2p"),
        "f2p_pass": chosen_row.get("f2p_pass"),
        "f2p_total": chosen_row.get("f2p_total"),
        "p2p": chosen_row.get("p2p"),
        "p2p_pass": chosen_row.get("p2p_pass"),
        "p2p_total": chosen_row.get("p2p_total"),
        "partial": chosen_row.get("partial"),
        "tok_in": tok_in,
        "tok_out": tok_out,
        "dur_s": best_dur,
        "notes": "; ".join(notes) if notes else "-",
        "rollouts": attempts,
        "flags": flags,
        "scored_flags": scored_flags,
        "rollout_durs": durs,
    }


def _mean(vals: list[float]) -> float | None:
    if not vals:
        return None
    return sum(vals) / len(vals)


def _timestamp(value) -> float | None:
    if isinstance(value, bool) or value is None:
        return None
    if isinstance(value, (int, float)):
        return float(value)
    if not isinstance(value, str) or not value.strip():
        return None
    text = value.strip()
    if text.endswith("Z"):
        text = text[:-1] + "+00:00"
    try:
        return datetime.fromisoformat(text).timestamp()
    except ValueError:
        return None


def _wall_seconds(rows: list[dict]) -> float | None:
    starts: list[float] = []
    finishes: list[float] = []
    for row in rows:
        for attempt in row.get("rollouts") or []:
            start = _timestamp(attempt.get("started_at"))
            finish = _timestamp(attempt.get("finished_at"))
            if start is None or finish is None or finish < start:
                continue
            starts.append(start)
            finishes.append(finish)
    if not starts:
        return None
    return max(finishes) - min(starts)


def _perfect_question(attempts: list[dict], n_rollouts: int) -> bool:
    """True when every requested attempt exists and each one is F2P 1 and P2P 1."""
    if n_rollouts < 1 or len(attempts) < n_rollouts:
        return False
    for row in attempts:
        f2p = _num(row.get("f2p"))
        p2p = _num(row.get("p2p"))
        if f2p is None or p2p is None:
            return False
        if abs(f2p - 1.0) > 1e-9 or abs(p2p - 1.0) > 1e-9:
            return False
    return True


def _median(vals: list[float]) -> float | None:
    if not vals:
        return None
    ordered = sorted(vals)
    mid = len(ordered) // 2
    if len(ordered) % 2:
        return float(ordered[mid])
    return (ordered[mid - 1] + ordered[mid]) / 2.0


def _micro(rows: list[dict], pass_key: str, total_key: str) -> dict:
    passes = [row.get(pass_key) for row in rows if isinstance(row.get(pass_key), int) and isinstance(row.get(total_key), int)]
    totals = [row.get(total_key) for row in rows if isinstance(row.get(pass_key), int) and isinstance(row.get(total_key), int)]
    if not totals or sum(totals) <= 0:
        return {"rate": None, "pass": None, "total": None}
    got = sum(passes)
    den = sum(totals)
    return {"rate": got / den, "pass": got, "total": den}


def summarize_arm(task_attempts: list[tuple[str, list[dict]]], n_rollouts: int, _concurrency: int) -> dict:
    rows = [task_row(tid, attempts, n_rollouts) for tid, attempts in task_attempts]
    flags = [row["flags"] for row in rows]
    scored_flags = [row["scored_flags"] for row in rows]
    ladder = pass_at_ladder(flags, n_rollouts)
    ladder_scored = pass_at_ladder_scored(scored_flags, n_rollouts)
    fracs = [row["pass_frac"] for row in rows]
    macro_p, macro_sd = mean_sd(fracs)
    scored_fracs = [row["pass_frac_scored"] for row in rows if row.get("n_scored")]
    macro_p_s, macro_sd_s = mean_sd(scored_fracs)
    if macro_p_s is None:
        macro_p_s, macro_sd_s = 0.0, 0.0
    any_hits = sum(1 for row in rows if row["best"])
    f2p_vals = [_num(row.get("f2p")) for row in rows]
    p2p_vals = [_num(row.get("p2p")) for row in rows]
    partial_vals = [_num(row.get("partial")) for row in rows]
    micro_f2p = _micro(rows, "f2p_pass", "f2p_total")
    micro_p2p = _micro(rows, "p2p_pass", "p2p_total")
    micro_partial_pass = None
    micro_partial_total = None
    if micro_f2p["pass"] is not None and micro_p2p["pass"] is not None:
        micro_partial_pass = micro_f2p["pass"] + micro_p2p["pass"]
        micro_partial_total = micro_f2p["total"] + micro_p2p["total"]
    micro_partial = None
    if micro_partial_total:
        micro_partial = micro_partial_pass / micro_partial_total
    all_durs = [d for row in rows for d in row["rollout_durs"]]
    best_durs = [_num(row.get("dur_s")) for row in rows]
    best_durs = [d for d in best_durs if d is not None]
    tok_rows = [row for row in rows if row.get("tok_in") is not None or row.get("tok_out") is not None]
    tok_in = sum(int(row.get("tok_in") or 0) for row in tok_rows) if tok_rows else None
    tok_out = sum(int(row.get("tok_out") or 0) for row in tok_rows) if tok_rows else None
    tok_total = None if tok_in is None and tok_out is None else int(tok_in or 0) + int(tok_out or 0)
    tok_avg = None if tok_total is None or not tok_rows else tok_total / len(tok_rows)
    scored = sum(len(row["rollouts"]) for row in rows)
    infra_excluded = sum(1 for _tid, attempts in task_attempts if not attempts)
    wall = _wall_seconds(rows)
    public_rows = []
    for row in rows:
        item = dict(row)
        item.pop("flags", None)
        item.pop("scored_flags", None)
        item.pop("rollout_durs", None)
        if _perfect_question(item.get("rollouts") or [], n_rollouts):
            item.pop("rollouts", None)
        public_rows.append(item)
    unscored_rollouts = sum(int(row.get("unscored") or 0) for row in rows)
    unscored_tasks = [
        {
            "id": row["id"],
            "unscored": row["unscored"],
            "n": row["n"],
            "unscored_frac": row["unscored_frac"],
            "notes": row["notes"],
        }
        for row in rows
        if int(row.get("unscored") or 0) > 0
    ]
    unscored_tasks.sort(key=lambda row: (-float(row["unscored_frac"]), str(row["id"])))
    arm = {
        **ladder,
        **ladder_scored,
        "macro_pass@1": macro_p,
        "macro_pass@1_sd": macro_sd,
        "macro_pass@1_scored": macro_p_s,
        "macro_pass@1_scored_sd": macro_sd_s,
        "any_pass": (any_hits / len(rows)) if rows else 0.0,
        "best_attempt_hits": any_hits,
        "infra_excluded": infra_excluded,
        "scored_rollouts": scored,
        "unscored_rollouts": unscored_rollouts,
        "unscored_tasks": unscored_tasks,
        "pass_methods": dict(PASS_METHODS),
        "macro": {
            "f2p": _mean([v for v in f2p_vals if v is not None]),
            "p2p": _mean([v for v in p2p_vals if v is not None]),
            "partial": _mean([v for v in partial_vals if v is not None]),
        },
        "micro": {
            "f2p": micro_f2p["rate"],
            "f2p_pass": micro_f2p["pass"],
            "f2p_total": micro_f2p["total"],
            "p2p": micro_p2p["rate"],
            "p2p_pass": micro_p2p["pass"],
            "p2p_total": micro_p2p["total"],
            "partial": micro_partial,
            "partial_pass": micro_partial_pass,
            "partial_total": micro_partial_total,
        },
        "timing": {
            "wall_seconds": wall,
            "median_best_attempt_s": _median(best_durs),
            "mean_best_attempt_s": _mean(best_durs),
            "mean_rollout_s": _mean(all_durs),
            "median_rollout_s": _median(all_durs),
        },
        "tokens": {
            "tasks_with_data": len(tok_rows),
            "in": tok_in,
            "out": tok_out,
            "total": tok_total,
            "avg_total_per_task": tok_avg,
        },
        "tasks": public_rows,
    }
    return arm
