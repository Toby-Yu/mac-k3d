#!/usr/bin/env python3
"""Validate eval JSON has required report fields (no network)."""

from __future__ import annotations

import argparse
import json
import os
import sys
from pathlib import Path

REQUIRED_TOP = (
    "harness",
    "benchmark",
    "n_tasks",
    "tasks",
    "access_date_utc",
    "llm_name",
    "llm_model_id",
    "llm_model_served",
    "llm_version",
    "duration_seconds",
    "token_usage",
    "totals",
)
REQUIRED_DURATION = ("harness", "baseline", "total")
REQUIRED_TOKEN = ("prompt", "completion", "total")
REQUIRED_TOTALS = (
    "harness_resolved",
    "baseline_resolved",
    "n",
    "pass_at_1_harness",
    "pass_at_1_baseline",
)
REQUIRED_TASK = ("id", "f2p", "p2p", "harness_resolved", "baseline_resolved")


def validate(doc: object) -> list[str]:
    errors: list[str] = []
    if not isinstance(doc, dict):
        return ["report must be a JSON object"]
    for key in REQUIRED_TOP:
        if key not in doc:
            errors.append(f"missing {key}")
    dur = doc.get("duration_seconds")
    if not isinstance(dur, dict):
        errors.append("duration_seconds must be an object")
    else:
        for key in REQUIRED_DURATION:
            if key not in dur:
                errors.append(f"missing duration_seconds.{key}")
    tok = doc.get("token_usage")
    if not isinstance(tok, dict):
        errors.append("token_usage must be an object")
    else:
        for key in REQUIRED_TOKEN:
            if key not in tok:
                errors.append(f"missing token_usage.{key}")
            elif not isinstance(tok[key], (int, float)):
                errors.append(f"token_usage.{key} must be a number")
    totals = doc.get("totals")
    if not isinstance(totals, dict):
        errors.append("totals must be an object")
    else:
        for key in REQUIRED_TOTALS:
            if key not in totals:
                errors.append(f"missing totals.{key}")
        n = totals.get("n")
        if isinstance(n, int) and n > 0:
            for key in ("pass_at_1_harness", "pass_at_1_baseline"):
                v = totals.get(key)
                if isinstance(v, (int, float)) and not 0 <= float(v) <= 1:
                    errors.append(f"totals.{key} must be in [0, 1]")
    tasks = doc.get("tasks")
    if not isinstance(tasks, list):
        errors.append("tasks must be an array")
    else:
        for i, task in enumerate(tasks):
            if not isinstance(task, dict):
                errors.append(f"tasks[{i}] must be an object")
                continue
            for key in REQUIRED_TASK:
                if key not in task:
                    errors.append(f"missing tasks[{i}].{key}")
    n_tasks = doc.get("n_tasks")
    if isinstance(n_tasks, int) and isinstance(tasks, list) and n_tasks < 0:
        errors.append("n_tasks must be >= 0")
    if doc.get("harness") not in (None, "icode"):
        errors.append("harness should be icode")
    if doc.get("benchmark") not in (None, "deepswe"):
        errors.append("benchmark should be deepswe")
    return errors


def resolve_path(explicit: str | None) -> Path:
    if explicit:
        return Path(explicit)
    workdir = Path(os.environ.get("MAC_K3D_EVAL_WORKDIR") or "eval-runs")
    last = workdir / "last_output.txt"
    if last.is_file():
        p = Path(last.read_text(encoding="utf-8").strip())
        if p.is_file():
            return p
    out_dir = workdir / "reports"
    jsons = sorted(out_dir.glob("eval-*.json")) if out_dir.is_dir() else []
    if jsons:
        return jsons[-1]
    raise FileNotFoundError(
        "no report path given and no $MAC_K3D_EVAL_WORKDIR/last_output.txt or eval-runs/reports/eval-*.json"
    )


def main() -> int:
    ap = argparse.ArgumentParser(description="Validate eval report JSON schema")
    ap.add_argument("path", nargs="?", help="JSON file (default: last eval output)")
    args = ap.parse_args()
    try:
        path = resolve_path(args.path)
    except FileNotFoundError as e:
        print(f"ERROR: {e}", file=sys.stderr)
        return 2
    if not path.is_file():
        print(f"ERROR: not a file: {path}", file=sys.stderr)
        return 2
    try:
        doc = json.loads(path.read_text(encoding="utf-8"))
    except json.JSONDecodeError as e:
        print(f"ERROR: invalid JSON: {e}", file=sys.stderr)
        return 1
    errors = validate(doc)
    if errors:
        print(f"FAIL report schema ({path})")
        for err in errors:
            print(f"  - {err}")
        return 1
    print(f"OK report schema ({path})")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
