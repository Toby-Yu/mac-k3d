"""Parse iCode `--json` token usage (same shape Harbor tees to icode.txt)."""

from __future__ import annotations

import json
from pathlib import Path
from typing import Any


def _first_present(usage: dict, keys: tuple[str, ...], extra: Any = None) -> Any:
    """First defined value, including 0. A missing key is not the same as zero."""
    for key in keys:
        if key in usage and usage[key] is not None:
            return usage[key]
    return extra


def usage_from_obj(obj: Any) -> dict[str, Any] | None:
    if not isinstance(obj, dict):
        return None
    usage = obj.get("usage") if isinstance(obj.get("usage"), dict) else None
    if usage is None and any(
        k in obj for k in ("input_tokens", "prompt_tokens", "prompt", "token_usage")
    ):
        usage = obj.get("token_usage") if isinstance(obj.get("token_usage"), dict) else obj
    if not isinstance(usage, dict):
        return None
    inp = usage.get("input_tokens")
    if inp is None:
        inp = usage.get("prompt_tokens", usage.get("prompt"))
    out = usage.get("output_tokens")
    if out is None:
        out = usage.get("completion_tokens", usage.get("completion"))
    tot = usage.get("total_tokens", usage.get("total"))
    if inp is None and out is None:
        return None
    try:
        prompt = int(inp or 0)
        completion = int(out or 0)
    except (TypeError, ValueError):
        return None
    try:
        total = int(tot) if tot is not None else prompt + completion
    except (TypeError, ValueError):
        total = prompt + completion
    model = obj.get("model") or obj.get("llm_model_id")
    details = usage.get("prompt_tokens_details")
    nested_cached = details.get("cached_tokens") if isinstance(details, dict) else None
    cache = _first_present(
        usage,
        ("prompt_cache_hit_tokens", "cache_hit_tokens", "cached_tokens", "cache"),
        nested_cached,
    )
    try:
        cache_hit = int(cache) if cache is not None else None
    except (TypeError, ValueError):
        cache_hit = None
    return {
        "prompt": prompt,
        "completion": completion,
        "total": total,
        "model": model if isinstance(model, str) else None,
        "cache_hit": cache_hit,
    }


def parse_icode_usage_text(text: str) -> dict[str, Any] | None:
    last: dict[str, Any] | None = None
    decoder = json.JSONDecoder()
    i = 0
    n = len(text)
    while i < n:
        start = text.find("{", i)
        if start < 0:
            break
        try:
            obj, end = decoder.raw_decode(text, start)
        except json.JSONDecodeError:
            i = start + 1
            continue
        got = usage_from_obj(obj)
        if got is not None:
            last = got
        i = end if end > start else start + 1
    return last


def parse_icode_usage_file(path: Path, tail_bytes: int = 262144) -> dict[str, Any] | None:
    if not path.is_file():
        return None
    try:
        raw = path.read_bytes()
    except OSError:
        return None
    if len(raw) > tail_bytes:
        raw = raw[-tail_bytes:]
    return parse_icode_usage_text(raw.decode("utf-8", errors="replace"))


def find_icode_usage(root: Path) -> dict[str, Any] | None:
    """Newest icode-usage.json, else last usage object in icode.txt/json."""
    if not root.is_dir():
        return None
    newest_usage: dict[str, Any] | None = None
    newest_mtime = -1.0
    for path in root.rglob("icode-usage.json"):
        try:
            mtime = path.stat().st_mtime
        except OSError:
            continue
        got = parse_icode_usage_file(path)
        if got is None:
            continue
        if mtime >= newest_mtime:
            newest_mtime = mtime
            newest_usage = got
    if newest_usage is not None:
        return newest_usage
    last: dict[str, Any] | None = None
    last_mtime = -1.0
    for name in ("icode.json", "icode.txt", "timing.json"):
        for path in root.rglob(name):
            try:
                mtime = path.stat().st_mtime
            except OSError:
                continue
            got = parse_icode_usage_file(path)
            if got is None:
                continue
            if mtime >= last_mtime:
                last_mtime = mtime
                last = got
    return last
