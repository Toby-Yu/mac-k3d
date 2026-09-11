#!/usr/bin/env python3
"""Baseline arm: send DeepSWE instruction.md to DeepSeek chat API (no iCode)."""

from __future__ import annotations

import argparse
import json
import os
import sys
import urllib.error
import urllib.request
from pathlib import Path


def list_tasks(tasks_dir: Path, n: int) -> list[Path]:
    dirs = sorted([p for p in tasks_dir.iterdir() if p.is_dir()])
    return dirs[:n]


def read_instruction(task_dir: Path) -> str:
    for name in ("instruction.md", "Instruction.md"):
        p = task_dir / name
        if p.is_file():
            return p.read_text(encoding="utf-8")
    raise FileNotFoundError(f"no instruction.md under {task_dir}")


def chat_deepseek(prompt: str, api_key: str, model: str) -> str:
    url = "https://api.deepseek.com/chat/completions"
    body = {
        "model": model,
        "messages": [
            {
                "role": "system",
                "content": (
                    "You are a coding agent. Solve the software engineering task. "
                    "Reply with a unified diff patch only when possible, otherwise explain briefly."
                ),
            },
            {"role": "user", "content": prompt},
        ],
        "temperature": 0.2,
    }
    req = urllib.request.Request(
        url,
        data=json.dumps(body).encode("utf-8"),
        headers={
            "Content-Type": "application/json",
            "Authorization": f"Bearer {api_key}",
        },
        method="POST",
    )
    with urllib.request.urlopen(req, timeout=600) as resp:
        data = json.loads(resp.read().decode("utf-8"))
    return data["choices"][0]["message"]["content"]


def extract_patch(text: str) -> str:
    if "```" in text:
        parts = text.split("```")
        for i, part in enumerate(parts):
            chunk = part.strip()
            if chunk.startswith("diff") or chunk.startswith("---") or "\n@@" in chunk:
                if chunk.startswith("diff") or chunk.startswith("---"):
                    return chunk
                # strip language tag
                lines = chunk.splitlines()
                if lines and not lines[0].startswith("---") and not lines[0].startswith("diff"):
                    return "\n".join(lines[1:])
                return chunk
    if "diff --git" in text or text.lstrip().startswith("--- "):
        return text
    return text


def main() -> int:
    ap = argparse.ArgumentParser()
    ap.add_argument("--tasks-dir", required=True)
    ap.add_argument("--n-tasks", type=int, default=1)
    ap.add_argument("--out-dir", required=True)
    ap.add_argument("--model", default=os.environ.get("DEEPSEEK_MODEL", "deepseek-chat"))
    args = ap.parse_args()

    api_key = os.environ.get("DEEPSEEK_API_KEY", "")
    if not api_key:
        print("DEEPSEEK_API_KEY missing", file=sys.stderr)
        return 1

    tasks_dir = Path(args.tasks_dir)
    out_dir = Path(args.out_dir)
    out_dir.mkdir(parents=True, exist_ok=True)

    tasks = list_tasks(tasks_dir, args.n_tasks)
    summary = []
    for task in tasks:
        tid = task.name
        print(f"baseline task={tid}")
        prompt = read_instruction(task)
        try:
            reply = chat_deepseek(prompt, api_key, args.model)
            patch = extract_patch(reply)
            task_out = out_dir / tid
            task_out.mkdir(parents=True, exist_ok=True)
            (task_out / "response.txt").write_text(reply, encoding="utf-8")
            (task_out / "agent.patch").write_text(patch, encoding="utf-8")
            summary.append({"id": tid, "ok": True, "patch_path": str(task_out / "agent.patch")})
            print(f"  wrote {task_out / 'agent.patch'}")
        except (urllib.error.URLError, urllib.error.HTTPError, KeyError, TimeoutError) as e:
            summary.append({"id": tid, "ok": False, "error": str(e)})
            print(f"  ERROR {e}", file=sys.stderr)

    (out_dir / "summary.json").write_text(json.dumps(summary, indent=2), encoding="utf-8")
    return 0 if all(x.get("ok") for x in summary) else 1


if __name__ == "__main__":
    raise SystemExit(main())
