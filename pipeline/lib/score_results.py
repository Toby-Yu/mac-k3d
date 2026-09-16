#!/usr/bin/env python3
"""Score harness vs baseline into one JSON schema for DeepSWE and LoLBench."""

from __future__ import annotations

import argparse
import json
import os
import re
from datetime import datetime, timezone
from pathlib import Path

from icode_usage import find_icode_usage


def load_json(path: Path):
    if path.is_file():
        return json.loads(path.read_text(encoding="utf-8"))
    return None


def find_bool_resolved(blob: str) -> bool | None:
    for pat in (
        r'"resolved"\s*:\s*(true|false)',
        r"\bRESOLVED\b[:=]\s*(true|false|yes|no|1|0)",
        r"\b(pass|fail|success|failed)\b",
    ):
        m = re.search(pat, blob, re.I)
        if not m:
            continue
        v = m.group(1).lower()
        if v in ("true", "yes", "1", "pass", "success"):
            return True
        if v in ("false", "no", "0", "fail", "failed"):
            return False
    return None


def parse_reward_value(raw) -> bool | None:
    if isinstance(raw, bool):
        return raw
    if isinstance(raw, (int, float)):
        return float(raw) >= 1.0 - 1e-9
    if isinstance(raw, str):
        text = raw.strip().lower()
        if text in ("1", "1.0", "true", "pass", "success"):
            return True
        if text in ("0", "0.0", "false", "fail", "failed"):
            return False
        try:
            return float(text) >= 1.0 - 1e-9
        except ValueError:
            return None
    return None


def harbor_reward_resolved(d: Path) -> bool | None:
    """Harbor verifier writes reward.json (1.0 = resolved). Missing file is unknown."""
    if not d.is_dir():
        return None
    newest = None
    newest_mtime = -1.0
    for path in d.rglob("reward.json"):
        try:
            mtime = path.stat().st_mtime
        except OSError:
            continue
        if mtime >= newest_mtime:
            newest_mtime = mtime
            newest = path
    if newest is None:
        return None
    try:
        data = json.loads(newest.read_text(encoding="utf-8"))
    except (OSError, json.JSONDecodeError):
        return None
    if isinstance(data, dict):
        if "reward" in data:
            return parse_reward_value(data["reward"])
        if "resolved" in data:
            return parse_reward_value(data["resolved"])
    return parse_reward_value(data)


def find_harbor_task_dir(harness_dir: Path, tid: str) -> Path | None:
    """Newest Harbor jobs dir named <tid> under harness/harbor_runs/ (not harness/<tid>/)."""
    if not harness_dir.is_dir() or not tid:
        return None
    candidates: list[Path] = []
    for path in harness_dir.rglob("*"):
        if path.is_dir() and path.name == tid and "harbor_runs" in path.parts:
            candidates.append(path)
    if not candidates:
        return None

    def _mtime(path: Path) -> float:
        newest = -1.0
        for reward in path.rglob("reward.json"):
            try:
                newest = max(newest, reward.stat().st_mtime)
            except OSError:
                pass
        try:
            newest = max(newest, path.stat().st_mtime)
        except OSError:
            pass
        return newest

    return max(candidates, key=_mtime)


def find_pier_task_dir(harness_dir: Path, tid: str) -> Path | None:
    """Newest Pier trial dir named <tid>__<rand> under timestamped job folders."""
    if not harness_dir.is_dir() or not tid:
        return None
    prefix = f"{tid}__"
    candidates: list[Path] = []
    for path in harness_dir.rglob("*"):
        if path.is_dir() and path.name.startswith(prefix):
            candidates.append(path)
    if not candidates:
        return None

    def _mtime(path: Path) -> float:
        newest = -1.0
        for name in ("result.json", "reward.json"):
            for found in path.rglob(name):
                try:
                    newest = max(newest, found.stat().st_mtime)
                except OSError:
                    pass
        try:
            newest = max(newest, path.stat().st_mtime)
        except OSError:
            pass
        return newest

    return max(candidates, key=_mtime)


def _norm_key(key: object) -> str:
    return re.sub(r"[^a-z0-9]+", "_", str(key).lower()).strip("_")


def _as_rate(raw) -> float | None:
    if isinstance(raw, bool):
        return 1.0 if raw else 0.0
    if isinstance(raw, (int, float)):
        return float(raw)
    if isinstance(raw, str):
        text = raw.strip()
        if "/" in text:
            left, _, right = text.partition("/")
            try:
                num = float(left.strip())
                den = float(right.strip())
            except ValueError:
                try:
                    return float(text)
                except ValueError:
                    return None
            if den == 0:
                return None
            return num / den
        try:
            return float(text)
        except ValueError:
            return None
    if isinstance(raw, dict):
        for key in ("mean", "avg", "value", "rate", "reward"):
            if key in raw:
                got = _as_rate(raw[key])
                if got is not None:
                    return got
        passed = _as_count(raw.get("passed") if "passed" in raw else raw.get("pass"))
        total = _as_count(raw.get("total") if "total" in raw else raw.get("n"))
        if passed is not None and total:
            return passed / total
    return None


def _as_count(raw) -> int | None:
    if isinstance(raw, bool) or raw is None:
        return None
    if isinstance(raw, int):
        return raw
    if isinstance(raw, float):
        if raw.is_integer() and 0 <= raw < 1_000_000:
            return int(raw)
        return None
    if isinstance(raw, str):
        text = raw.strip()
        if "/" in text:
            left, _, _right = text.partition("/")
            return _as_count(left)
        try:
            val = float(text)
        except ValueError:
            return None
        return _as_count(val)
    return None


F2P_RATE_KEYS = {
    "fail_to_pass",
    "fail_to_pass_rate",
    "f2p",
    "f2p_rate",
    "f2p_p",
    "f2p_pass_rate",
}
P2P_RATE_KEYS = {
    "pass_to_pass",
    "pass_to_pass_rate",
    "p2p",
    "p2p_rate",
    "p2p_p",
    "p2p_pass_rate",
}
PARTIAL_RATE_KEYS = {"partial", "partial_rate"}
F2P_PASS_KEYS = {"f2p_pass", "f2p_passed", "fail_to_pass_pass", "n_fail_to_pass_passed"}
F2P_TOTAL_KEYS = {"f2p_total", "fail_to_pass_total", "n_fail_to_pass"}
P2P_PASS_KEYS = {"p2p_pass", "p2p_passed", "pass_to_pass_pass", "n_pass_to_pass_passed"}
P2P_TOTAL_KEYS = {"p2p_total", "pass_to_pass_total", "n_pass_to_pass"}


def _metric_key(node: dict) -> str:
    return _norm_key(node.get("key") or node.get("name") or node.get("metric") or "")


def collect_named_rate(obj, aliases: set[str]) -> float | None:
    found: list[float] = []

    def walk(node) -> None:
        if isinstance(node, dict):
            named = _metric_key(node)
            if named in aliases:
                rate = _as_rate(node.get("value") if "value" in node else node)
                if rate is not None and 0 <= rate <= 1:
                    found.append(rate)
            for key, val in node.items():
                if _norm_key(key) in aliases:
                    rate = _as_rate(val)
                    if rate is not None and 0 <= rate <= 1:
                        found.append(rate)
                walk(val)
        elif isinstance(node, list):
            for item in node:
                walk(item)

    walk(obj)
    return found[0] if found else None


def collect_named_count(obj, aliases: set[str]) -> int | None:
    found: list[int] = []

    def walk(node) -> None:
        if isinstance(node, dict):
            named = _metric_key(node)
            if named in aliases:
                got = _as_count(node.get("value") if "value" in node else node)
                if got is not None:
                    found.append(got)
            for key, val in node.items():
                if _norm_key(key) in aliases:
                    got = _as_count(val)
                    if got is not None:
                        found.append(got)
                walk(val)
        elif isinstance(node, list):
            for item in node:
                walk(item)

    walk(obj)
    return found[0] if found else None


def collect_fraction_counts(obj, rate_aliases: set[str]) -> tuple[int | None, int | None]:
    found: list[tuple[int, int]] = []

    def consider(raw) -> None:
        if isinstance(raw, str) and "/" in raw:
            left, _, right = raw.partition("/")
            num = _as_count(left)
            den = _as_count(right)
            if num is not None and den:
                found.append((num, den))
        if isinstance(raw, dict):
            num = _as_count(raw.get("passed") if "passed" in raw else raw.get("pass"))
            den = _as_count(raw.get("total") if "total" in raw else raw.get("n"))
            if num is not None and den:
                found.append((num, den))

    def walk(node) -> None:
        if isinstance(node, dict):
            named = _metric_key(node)
            if named in rate_aliases:
                consider(node.get("value") if "value" in node else node)
            for key, val in node.items():
                if _norm_key(key) in rate_aliases:
                    consider(val)
                walk(val)
        elif isinstance(node, list):
            for item in node:
                walk(item)

    walk(obj)
    return found[0] if found else (None, None)


def verifier_rates(d: Path) -> dict:
    """F2P / P2P rates and optional counts from Pier or Harbor JSON (not test-name lists)."""
    empty = {
        "f2p": None,
        "p2p": None,
        "partial": None,
        "f2p_pass": None,
        "f2p_total": None,
        "p2p_pass": None,
        "p2p_total": None,
    }
    if not d.is_dir():
        return empty
    files: list[Path] = []
    for path in d.rglob("*.json"):
        if path.name in {"events.json", "events.jsonl"}:
            continue
        try:
            if path.stat().st_size >= 2_000_000:
                continue
        except OSError:
            continue
        files.append(path)
    files.sort(key=lambda p: (p.name != "result.json", p.name != "reward.json", str(p)))
    out = dict(empty)
    for path in files:
        try:
            data = json.loads(path.read_text(encoding="utf-8"))
        except (OSError, json.JSONDecodeError):
            continue
        if out["f2p"] is None:
            out["f2p"] = collect_named_rate(data, F2P_RATE_KEYS)
        if out["p2p"] is None:
            out["p2p"] = collect_named_rate(data, P2P_RATE_KEYS)
        if out["partial"] is None:
            out["partial"] = collect_named_rate(data, PARTIAL_RATE_KEYS)
        if out["f2p_pass"] is None:
            out["f2p_pass"] = collect_named_count(data, F2P_PASS_KEYS)
        if out["f2p_total"] is None:
            out["f2p_total"] = collect_named_count(data, F2P_TOTAL_KEYS)
        if out["p2p_pass"] is None:
            out["p2p_pass"] = collect_named_count(data, P2P_PASS_KEYS)
        if out["p2p_total"] is None:
            out["p2p_total"] = collect_named_count(data, P2P_TOTAL_KEYS)
        if out["f2p_pass"] is None or out["f2p_total"] is None:
            num, den = collect_fraction_counts(data, F2P_RATE_KEYS)
            if out["f2p_pass"] is None:
                out["f2p_pass"] = num
            if out["f2p_total"] is None:
                out["f2p_total"] = den
        if out["p2p_pass"] is None or out["p2p_total"] is None:
            num, den = collect_fraction_counts(data, P2P_RATE_KEYS)
            if out["p2p_pass"] is None:
                out["p2p_pass"] = num
            if out["p2p_total"] is None:
                out["p2p_total"] = den
        if out["f2p"] is not None and out["p2p"] is not None:
            break
    return out


def scan_dir(d: Path) -> dict:
    if not d.is_dir():
        return {"resolved": None, "patch_path": None}
    patch = None
    texts = []
    for p in d.rglob("*"):
        if not p.is_file():
            continue
        if p.name.endswith(".patch") or p.name == "agent.patch":
            patch = str(p)
        if p.suffix in {".json", ".log", ".txt", ".md"} and p.stat().st_size < 2_000_000:
            try:
                texts.append(p.read_text(encoding="utf-8", errors="ignore"))
            except OSError:
                pass
    blob = "\n".join(texts)
    reward = harbor_reward_resolved(d)
    resolved = reward if reward is not None else find_bool_resolved(blob)
    return {"resolved": resolved, "patch_path": patch}


def task_ids(tasks_dir: Path, n: int) -> list[str]:
    if not tasks_dir.is_dir():
        return []
    return [p.name for p in sorted(tasks_dir.iterdir()) if p.is_dir()][:n]


def pass_at_1(ok: int, n: int) -> float:
    if n <= 0:
        return 0.0
    return round(ok / n, 4)


def as_minutes(seconds) -> float | None:
    if not isinstance(seconds, (int, float)):
        return None
    return round(float(seconds) / 60.0, 3)


def reward_float(resolved: bool | None) -> float | None:
    if resolved is True:
        return 1.0
    if resolved is False:
        return 0.0
    return None


def usage_pair(usage) -> tuple[int | None, int | None]:
    if not isinstance(usage, dict):
        return None, None
    prompt = usage.get("prompt")
    if prompt is None:
        prompt = usage.get("prompt_tokens", usage.get("input_tokens"))
    completion = usage.get("completion")
    if completion is None:
        completion = usage.get("completion_tokens", usage.get("output_tokens"))
    if prompt is None and completion is None:
        return None, None
    try:
        tok_in = int(prompt or 0)
        tok_out = int(completion or 0)
    except (TypeError, ValueError):
        return None, None
    return tok_in, tok_out


def mean_or_none(vals: list) -> float | None:
    nums = [float(v) for v in vals if isinstance(v, (int, float))]
    if not nums:
        return None
    return round(sum(nums) / len(nums), 4)


def add_known(a: int | None, b: int | None) -> int | None:
    if a is None and b is None:
        return None
    return int(a or 0) + int(b or 0)


def baseline_object(bmeta: dict, b_resolved: bool | None) -> dict:
    tok_in, tok_out = usage_pair(bmeta.get("token_usage"))
    dur = bmeta.get("duration_seconds")
    return {
        "reward": reward_float(b_resolved),
        "tok_in": tok_in,
        "tok_out": tok_out,
        "dur_s": dur if isinstance(dur, (int, float)) else None,
        "dur_min": as_minutes(dur),
    }


def main() -> int:
    ap = argparse.ArgumentParser()
    ap.add_argument("--harness-dir", required=True)
    ap.add_argument("--baseline-dir", required=True)
    ap.add_argument("--tasks-dir", required=True)
    ap.add_argument("--n-tasks", type=int, default=1)
    ap.add_argument("--out", required=True)
    ap.add_argument("--finalize", action="store_true")
    ap.add_argument("--task-file", default="", help="newline-separated task dir names")
    args = ap.parse_args()

    harness_dir = Path(args.harness_dir)
    baseline_dir = Path(args.baseline_dir)
    tasks_dir = Path(args.tasks_dir)
    if args.task_file:
        raw = Path(args.task_file).read_text(encoding="utf-8")
        names = [ln.strip() for ln in raw.splitlines() if ln.strip()]
        ids = [n for n in names if (tasks_dir / n).is_dir()]
        if not ids:
            ids = task_ids(tasks_dir, args.n_tasks)
    else:
        ids = task_ids(tasks_dir, args.n_tasks)

    baseline_summary = load_json(baseline_dir / "summary.json") or []
    if not isinstance(baseline_summary, list):
        baseline_summary = []
    baseline_by_id = {x["id"]: x for x in baseline_summary if isinstance(x, dict) and "id" in x}
    baseline_meta = load_json(baseline_dir / "meta.json") or {}
    harness_meta = load_json(harness_dir / "meta.json") or {}
    lolbench = (os.environ.get("BENCHMARK") or "deepswe") == "lolbench"
    n_rollouts = 1
    d_h = harness_meta.get("duration_seconds")
    meta_h_in, meta_h_out = usage_pair(harness_meta.get("token_usage"))
    if lolbench:
        meta_h_in = meta_h_out = None
    elif (not meta_h_in and not meta_h_out):
        scraped = find_icode_usage(harness_dir)
        if scraped:
            meta_h_in = int(scraped.get("prompt") or 0)
            meta_h_out = int(scraped.get("completion") or 0)

    per_task = []
    for tid in ids:
        if lolbench:
            hdir = find_harbor_task_dir(harness_dir, tid)
            h = scan_dir(hdir) if hdir is not None else scan_dir(harness_dir / tid)
            if h["resolved"] is None and h["patch_path"] is None:
                h = scan_dir(harness_dir)
            rates_root = hdir if hdir is not None else harness_dir
        else:
            hdir = find_pier_task_dir(harness_dir, tid)
            h = scan_dir(hdir) if hdir is not None else scan_dir(harness_dir / tid)
            if h["resolved"] is None and h["patch_path"] is None:
                h = scan_dir(harness_dir)
            rates_root = hdir if hdir is not None else (
                harness_dir / tid if (harness_dir / tid).is_dir() else harness_dir
            )
        rates = verifier_rates(rates_root)
        if rates["f2p"] is None and rates["p2p"] is None:
            rates = verifier_rates(harness_dir)
        b = scan_dir(baseline_dir / tid)
        bmeta = baseline_by_id.get(tid, {})
        if b["resolved"] is None and bmeta.get("ok") is True:
            b["resolved"] = None
        reward = reward_float(h["resolved"])
        c = 1 if reward == 1.0 else 0
        tok_in = tok_out = None
        if not lolbench:
            tok_in, tok_out = meta_h_in, meta_h_out
        dur_s = d_h if isinstance(d_h, (int, float)) else None
        per_task.append(
            {
                "id": tid,
                "c": c,
                "n": n_rollouts,
                "pass_frac": pass_at_1(c, n_rollouts),
                "first": reward == 1.0,
                "reward": reward,
                "f2p": rates["f2p"],
                "f2p_pass": rates["f2p_pass"],
                "f2p_total": rates["f2p_total"],
                "p2p": rates["p2p"],
                "p2p_pass": rates["p2p_pass"],
                "p2p_total": rates["p2p_total"],
                "partial": rates["partial"],
                "tok_in": tok_in,
                "tok_out": tok_out,
                "dur_s": dur_s,
                "dur_min": as_minutes(dur_s),
                "baseline": baseline_object(bmeta, b["resolved"]),
            }
        )

    n = len(per_task)
    harness_ok = sum(1 for t in per_task if t["reward"] == 1.0)
    d_b = baseline_meta.get("duration_seconds")
    wall_seconds = None
    if isinstance(d_h, (int, float)) or isinstance(d_b, (int, float)):
        wall_seconds = round(float(d_h or 0) + float(d_b or 0), 3)

    tok_in_h = tok_out_h = None
    if not lolbench:
        tok_in_h, tok_out_h = meta_h_in, meta_h_out
    tok_in_b = tok_out_b = None
    if baseline_meta.get("token_usage"):
        tok_in_b, tok_out_b = usage_pair(baseline_meta.get("token_usage"))
    if tok_in_b is None:
        for t in per_task:
            tok_in_b = add_known(tok_in_b, t["baseline"].get("tok_in"))
            tok_out_b = add_known(tok_out_b, t["baseline"].get("tok_out"))
    if tok_in_h is not None or tok_out_h is not None:
        tokens_in, tokens_out = tok_in_h, tok_out_h
    else:
        tokens_in, tokens_out = tok_in_b, tok_out_b
    tokens_total = add_known(tokens_in, tokens_out)

    model_id = (
        os.environ.get("DEEPSEEK_MODEL")
        or harness_meta.get("llm_model_id")
        or baseline_meta.get("llm_model_id")
        or "deepseek-v4-pro"
    )
    served = baseline_meta.get("llm_model_served") or harness_meta.get("llm_model_served")
    access = (
        harness_meta.get("access_date_utc")
        or baseline_meta.get("access_date_utc")
        or datetime.now(timezone.utc).strftime("%Y-%m-%dT%H:%M:%SZ")
    )
    api_base = (
        os.environ.get("DEEPSEEK_API_BASE")
        or os.environ.get("ICODE_API_BASE")
        or "https://api.deepseek.com"
    )

    doc = {
        "suite": os.environ.get("BENCHMARK") or "deepswe",
        "harness": "icode",
        "model": model_id,
        "model_served": served,
        "api_base": api_base,
        "n_tasks": args.n_tasks,
        "n_rollouts": n_rollouts,
        "access_date_utc": access,
        "wall_seconds": wall_seconds,
        "wall_minutes": as_minutes(wall_seconds),
        "pass_at_1": pass_at_1(harness_ok, n),
        "macro": {
            "f2p": mean_or_none([t["f2p"] for t in per_task]),
            "p2p": mean_or_none([t["p2p"] for t in per_task]),
            "partial": mean_or_none([t["partial"] for t in per_task]),
            "reward": mean_or_none([t["reward"] for t in per_task]),
        },
        "tokens": {"in": tokens_in, "out": tokens_out, "total": tokens_total},
        "tasks": per_task,
    }

    out = Path(args.out)
    out.parent.mkdir(parents=True, exist_ok=True)
    out.write_text(json.dumps(doc, indent=2), encoding="utf-8")
    print(f"wrote {out}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
