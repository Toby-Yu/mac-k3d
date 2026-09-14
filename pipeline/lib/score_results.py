#!/usr/bin/env python3
"""Score harness vs baseline into JSON (f2p / p2p, pass@1, tokens, time, model)."""

from __future__ import annotations

import argparse
import json
import os
import re
from datetime import datetime, timezone
from pathlib import Path


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


def extract_list(blob: str, key: str) -> list[str]:
    m = re.search(rf'"{key}"\s*:\s*\[(.*?)\]', blob, re.S | re.I)
    if not m:
        return []
    return re.findall(r'"([^"]+)"', m.group(1))


def extract_tokens(blob: str) -> dict:
    def first_int(*pats: str) -> int:
        for pat in pats:
            m = re.search(pat, blob, re.I)
            if m:
                return int(m.group(1))
        return 0

    prompt = first_int(
        r'"prompt_tokens"\s*:\s*(\d+)',
        r"\bprompt_tokens[=:\s]+(\d+)",
    )
    completion = first_int(
        r'"completion_tokens"\s*:\s*(\d+)',
        r"\bcompletion_tokens[=:\s]+(\d+)",
    )
    total = first_int(
        r'"total_tokens"\s*:\s*(\d+)',
        r"\btotal_tokens[=:\s]+(\d+)",
    )
    if total == 0:
        total = prompt + completion
    return {"prompt": prompt, "completion": completion, "total": total}


def empty_tokens() -> dict:
    return {"prompt": 0, "completion": 0, "total": 0}


def add_tokens(a: dict, b: dict) -> dict:
    return {
        "prompt": int(a.get("prompt") or 0) + int(b.get("prompt") or 0),
        "completion": int(a.get("completion") or 0) + int(b.get("completion") or 0),
        "total": int(a.get("total") or 0) + int(b.get("total") or 0),
    }


def scan_dir(d: Path) -> dict:
    if not d.is_dir():
        return {
            "resolved": None,
            "f2p": [],
            "p2p": [],
            "patch_path": None,
            "token_usage": empty_tokens(),
        }
    texts = []
    patch = None
    for p in d.rglob("*"):
        if not p.is_file():
            continue
        if p.name.endswith(".patch") or p.name == "agent.patch":
            patch = str(p)
        if p.suffix in {".json", ".jsonl", ".log", ".txt", ".md"} and p.stat().st_size < 2_000_000:
            try:
                texts.append(p.read_text(encoding="utf-8", errors="ignore"))
            except OSError:
                pass
    blob = "\n".join(texts)
    return {
        "resolved": find_bool_resolved(blob),
        "f2p": extract_list(blob, "FAIL_TO_PASS") or extract_list(blob, "fail_to_pass"),
        "p2p": extract_list(blob, "PASS_TO_PASS") or extract_list(blob, "pass_to_pass"),
        "patch_path": patch,
        "token_usage": extract_tokens(blob),
    }


def task_ids(tasks_dir: Path, n: int) -> list[str]:
    if not tasks_dir.is_dir():
        return []
    return [p.name for p in sorted(tasks_dir.iterdir()) if p.is_dir()][:n]


def pass_at_1(ok: int, n: int) -> float:
    if n <= 0:
        return 0.0
    return round(ok / n, 4)


def main() -> int:
    ap = argparse.ArgumentParser()
    ap.add_argument("--harness-dir", required=True)
    ap.add_argument("--baseline-dir", required=True)
    ap.add_argument("--tasks-dir", required=True)
    ap.add_argument("--n-tasks", type=int, default=1)
    ap.add_argument("--out", required=True)
    ap.add_argument("--finalize", action="store_true")
    args = ap.parse_args()

    harness_dir = Path(args.harness_dir)
    baseline_dir = Path(args.baseline_dir)
    tasks_dir = Path(args.tasks_dir)
    ids = task_ids(tasks_dir, args.n_tasks)

    baseline_summary = load_json(baseline_dir / "summary.json") or []
    if not isinstance(baseline_summary, list):
        baseline_summary = []
    baseline_by_id = {x["id"]: x for x in baseline_summary if isinstance(x, dict) and "id" in x}
    baseline_meta = load_json(baseline_dir / "meta.json") or {}
    harness_meta = load_json(harness_dir / "meta.json") or {}

    per_task = []
    for tid in ids:
        h = scan_dir(harness_dir / tid)
        if h["resolved"] is None and h["patch_path"] is None:
            h = scan_dir(harness_dir)
        b = scan_dir(baseline_dir / tid)
        bmeta = baseline_by_id.get(tid, {})
        if b["patch_path"] is None and bmeta.get("patch_path"):
            b["patch_path"] = bmeta["patch_path"]
        if b["resolved"] is None and bmeta.get("ok") is True:
            b["resolved"] = None
        b_tokens = bmeta.get("token_usage") or b["token_usage"]
        h_tokens = h["token_usage"]

        per_task.append(
            {
                "id": tid,
                "harness_resolved": h["resolved"],
                "baseline_resolved": b["resolved"],
                "f2p": h["f2p"] or b["f2p"],
                "p2p": h["p2p"] or b["p2p"],
                "harness_patch_path": h["patch_path"],
                "baseline_patch_path": b["patch_path"],
                "token_usage": {
                    "harness": h_tokens,
                    "baseline": b_tokens,
                },
                "duration_seconds": {
                    "harness": harness_meta.get("duration_seconds"),
                    "baseline": bmeta.get("duration_seconds"),
                },
            }
        )

    n = len(per_task)
    harness_ok = sum(1 for t in per_task if t["harness_resolved"] is True)
    baseline_ok = sum(1 for t in per_task if t["baseline_resolved"] is True)

    token_harness = empty_tokens()
    token_baseline = empty_tokens()
    for t in per_task:
        tu = t.get("token_usage") or {}
        token_harness = add_tokens(token_harness, tu.get("harness") or empty_tokens())
        token_baseline = add_tokens(token_baseline, tu.get("baseline") or empty_tokens())
    if token_baseline["total"] == 0 and baseline_meta.get("token_usage"):
        token_baseline = baseline_meta["token_usage"]
    if token_harness["total"] == 0 and harness_meta.get("token_usage"):
        token_harness = harness_meta["token_usage"]
    token_total = add_tokens(token_harness, token_baseline)

    model_id = (
        os.environ.get("DEEPSEEK_MODEL")
        or harness_meta.get("llm_model_id")
        or baseline_meta.get("llm_model_id")
        or "deepseek-v4-pro"
    )
    llm_name = os.environ.get("LLM_NAME") or "DeepSeek V4 Pro"
    served = baseline_meta.get("llm_model_served") or harness_meta.get("llm_model_served")
    version = baseline_meta.get("llm_version") or harness_meta.get("llm_version")
    access = (
        harness_meta.get("access_date_utc")
        or baseline_meta.get("access_date_utc")
        or datetime.now(timezone.utc).strftime("%Y-%m-%dT%H:%M:%SZ")
    )
    d_h = harness_meta.get("duration_seconds")
    d_b = baseline_meta.get("duration_seconds")
    d_total = None
    if isinstance(d_h, (int, float)) or isinstance(d_b, (int, float)):
        d_total = round(float(d_h or 0) + float(d_b or 0), 3)

    doc = {
        "run_id": datetime.now(timezone.utc).strftime("%Y%m%dT%H%M%SZ"),
        "access_date_utc": access,
        "harness": "icode",
        "llm": "deepseek",
        "llm_name": llm_name,
        "llm_model_id": model_id,
        "llm_model_served": served,
        "llm_version": version,
        "benchmark": "deepswe",
        "n_tasks": args.n_tasks,
        "tasks": per_task,
        "duration_seconds": {
            "harness": d_h,
            "baseline": d_b,
            "total": d_total,
        },
        "token_usage": {
            "prompt": token_total["prompt"],
            "completion": token_total["completion"],
            "total": token_total["total"],
            "harness": token_harness,
            "baseline": token_baseline,
        },
        "totals": {
            "harness_resolved": harness_ok,
            "baseline_resolved": baseline_ok,
            "n": n,
            "pass_at_1_harness": pass_at_1(harness_ok, n),
            "pass_at_1_baseline": pass_at_1(baseline_ok, n),
        },
        "notes": (
            "f2p/p2p lists are parsed from Pier/DeepSWE verifier artifacts when present; "
            "pass@1 is resolved_true / n for a single attempt per task; "
            "token_usage and duration come from the DeepSeek API (baseline) and logs (harness) when present."
        ),
    }

    out = Path(args.out)
    out.parent.mkdir(parents=True, exist_ok=True)
    out.write_text(json.dumps(doc, indent=2), encoding="utf-8")
    print(f"wrote {out}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
