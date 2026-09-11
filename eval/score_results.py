#!/usr/bin/env python3
"""Score harness vs baseline into JSON (f2p / p2p best-effort from Pier/DeepSWE artifacts)."""

from __future__ import annotations

import argparse
import json
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
    # JSON-ish "FAIL_TO_PASS": ["a", "b"]
    m = re.search(rf'"{key}"\s*:\s*\[(.*?)\]', blob, re.S | re.I)
    if not m:
        return []
    return re.findall(r'"([^"]+)"', m.group(1))


def scan_dir(d: Path) -> dict:
    if not d.is_dir():
        return {"resolved": None, "f2p": [], "p2p": [], "patch_path": None}
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
    }


def task_ids(tasks_dir: Path, n: int) -> list[str]:
    return [p.name for p in sorted(tasks_dir.iterdir()) if p.is_dir()][:n]


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
    baseline_by_id = {x["id"]: x for x in baseline_summary if isinstance(x, dict) and "id" in x}

    per_task = []
    for tid in ids:
        h = scan_dir(harness_dir / tid)
        if h["resolved"] is None and h["patch_path"] is None:
            # Pier may nest differently — scan whole harness dir once for this id
            h = scan_dir(harness_dir)
        b = scan_dir(baseline_dir / tid)
        bmeta = baseline_by_id.get(tid, {})
        if b["patch_path"] is None and bmeta.get("patch_path"):
            b["patch_path"] = bmeta["patch_path"]
        if b["resolved"] is None and bmeta.get("ok") is True:
            # Baseline produced a reply; verifier may not have run yet
            b["resolved"] = None

        per_task.append(
            {
                "id": tid,
                "harness_resolved": h["resolved"],
                "baseline_resolved": b["resolved"],
                "f2p": h["f2p"] or b["f2p"],
                "p2p": h["p2p"] or b["p2p"],
                "harness_patch_path": h["patch_path"],
                "baseline_patch_path": b["patch_path"],
            }
        )

    harness_ok = sum(1 for t in per_task if t["harness_resolved"] is True)
    baseline_ok = sum(1 for t in per_task if t["baseline_resolved"] is True)

    doc = {
        "run_id": datetime.now(timezone.utc).strftime("%Y%m%dT%H%M%SZ"),
        "harness": "icode",
        "llm": "deepseek",
        "benchmark": "deepswe",
        "n_tasks": args.n_tasks,
        "tasks": per_task,
        "totals": {
            "harness_resolved": harness_ok,
            "baseline_resolved": baseline_ok,
            "n": len(per_task),
        },
        "notes": (
            "f2p/p2p lists are parsed from Pier/DeepSWE verifier artifacts when present; "
            "otherwise resolved may be null until verifiers are wired."
        ),
    }

    out = Path(args.out)
    out.parent.mkdir(parents=True, exist_ok=True)
    out.write_text(json.dumps(doc, indent=2), encoding="utf-8")
    print(f"wrote {out}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
