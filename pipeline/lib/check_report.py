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
    "suite",
    "n_tasks",
    "n_rollouts",
    "tasks",
    "access_date_utc",
    "model",
    "model_served",
    "api_base",
    "wall_seconds",
    "wall_minutes",
    "pass_at_1",
    "macro",
    "tokens",
)
REQUIRED_MACRO = ("f2p", "p2p", "reward")
REQUIRED_TOKEN = ("in", "out", "total")
REQUIRED_TASK = (
    "id",
    "c",
    "n",
    "pass_frac",
    "first",
    "reward",
    "f2p",
    "p2p",
    "tok_in",
    "tok_out",
    "dur_s",
    "dur_min",
    "baseline",
)
REQUIRED_BASELINE = ("reward", "tok_in", "tok_out", "dur_s", "dur_min")


def _rate_error(loc: str, value: object) -> str | None:
    if value is None:
        return None
    if isinstance(value, list):
        return f"{loc} must be a number or null, not a list"
    if not isinstance(value, (int, float)) or not 0 <= float(value) <= 1:
        return f"{loc} must be null or a number in [0, 1]"
    return None


def _num_or_null_error(loc: str, value: object) -> str | None:
    if value is None:
        return None
    if not isinstance(value, (int, float)):
        return f"{loc} must be a number or null"
    return None


def validate(doc: object) -> list[str]:
    errors: list[str] = []
    if not isinstance(doc, dict):
        return ["report must be a JSON object"]
    for key in REQUIRED_TOP:
        if key not in doc:
            errors.append(f"missing {key}")
    if doc.get("harness") not in (None, "icode"):
        errors.append("harness should be icode")
    if doc.get("suite") not in (None, "deepswe", "lolbench", "swebenchpro"):
        errors.append("suite should be deepswe, lolbench, or swebenchpro")
    icode_git = doc.get("icode_git")
    if icode_git is not None:
        if not isinstance(icode_git, dict):
            errors.append("icode_git must be an object")
        else:
            for key in ("url", "kind", "ref", "sha"):
                if key not in icode_git:
                    errors.append(f"missing icode_git.{key}")
            kind = icode_git.get("kind")
            if kind is not None and kind not in ("branch", "tag", "commit"):
                errors.append("icode_git.kind should be branch, tag, or commit")
    n_rollouts = doc.get("n_rollouts")
    if n_rollouts is not None and (
        isinstance(n_rollouts, bool) or not isinstance(n_rollouts, int) or n_rollouts < 1
    ):
        errors.append("n_rollouts must be an integer >= 1")
    err = _rate_error("pass_at_1", doc.get("pass_at_1"))
    if err:
        errors.append(err)
    wall_s = doc.get("wall_seconds")
    wall_m = doc.get("wall_minutes")
    err = _num_or_null_error("wall_seconds", wall_s)
    if err:
        errors.append(err)
    err = _num_or_null_error("wall_minutes", wall_m)
    if err:
        errors.append(err)
    if isinstance(wall_s, (int, float)) and isinstance(wall_m, (int, float)):
        expected = round(float(wall_s) / 60.0, 3)
        if abs(float(wall_m) - expected) > 0.001:
            errors.append("wall_minutes must be wall_seconds / 60")
    macro = doc.get("macro")
    if not isinstance(macro, dict):
        errors.append("macro must be an object")
    else:
        for key in REQUIRED_MACRO:
            if key not in macro:
                errors.append(f"missing macro.{key}")
            else:
                err = _rate_error(f"macro.{key}", macro[key])
                if err:
                    errors.append(err)
        if "partial" in macro:
            err = _rate_error("macro.partial", macro["partial"])
            if err:
                errors.append(err)
    tok = doc.get("tokens")
    if not isinstance(tok, dict):
        errors.append("tokens must be an object")
    else:
        for key in REQUIRED_TOKEN:
            if key not in tok:
                errors.append(f"missing tokens.{key}")
            else:
                err = _num_or_null_error(f"tokens.{key}", tok[key])
                if err:
                    errors.append(err)
    tasks = doc.get("tasks")
    if not isinstance(tasks, list):
        errors.append("tasks must be an array")
        return errors
    for i, task in enumerate(tasks):
        if not isinstance(task, dict):
            errors.append(f"tasks[{i}] must be an object")
            continue
        for key in REQUIRED_TASK:
            if key not in task:
                errors.append(f"missing tasks[{i}].{key}")
        for key in ("f2p", "p2p", "partial", "pass_frac", "reward"):
            if key not in task:
                continue
            err = _rate_error(f"tasks[{i}].{key}", task[key])
            if err:
                errors.append(err)
        if "first" in task and task["first"] not in (True, False):
            errors.append(f"tasks[{i}].first must be a boolean")
        if (
            isinstance(n_rollouts, int)
            and not isinstance(n_rollouts, bool)
            and task.get("n") != n_rollouts
        ):
            errors.append(f"tasks[{i}].n must match n_rollouts")
        for key in ("tok_in", "tok_out", "dur_s", "dur_min"):
            if key not in task:
                continue
            err = _num_or_null_error(f"tasks[{i}].{key}", task[key])
            if err:
                errors.append(err)
        dur_s = task.get("dur_s")
        dur_m = task.get("dur_min")
        if isinstance(dur_s, (int, float)) and isinstance(dur_m, (int, float)):
            expected = round(float(dur_s) / 60.0, 3)
            if abs(float(dur_m) - expected) > 0.001:
                errors.append(f"tasks[{i}].dur_min must be dur_s / 60")
        baseline = task.get("baseline")
        if not isinstance(baseline, dict):
            errors.append(f"tasks[{i}].baseline must be an object")
        else:
            for key in REQUIRED_BASELINE:
                if key not in baseline:
                    errors.append(f"missing tasks[{i}].baseline.{key}")
            err = _rate_error(f"tasks[{i}].baseline.reward", baseline.get("reward"))
            if err:
                errors.append(err)
            for key in ("tok_in", "tok_out", "dur_s", "dur_min"):
                err = _num_or_null_error(f"tasks[{i}].baseline.{key}", baseline.get(key))
                if err:
                    errors.append(err)
    n_tasks = doc.get("n_tasks")
    if isinstance(n_tasks, int) and n_tasks < 0:
        errors.append("n_tasks must be >= 0")
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
