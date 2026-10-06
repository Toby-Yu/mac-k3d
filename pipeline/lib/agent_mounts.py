#!/usr/bin/env python3
"""Harbor --mounts for the iCode agent, and the allowlist tasks/isolation checks before any rollout.

The only mount is the iCode host tree, read-only, at /opt/icode-host. It must not
be, contain, or sit inside a benchmark checkout (solution/ and tests/private).
"""

from __future__ import annotations

import argparse
import json
import sys
from pathlib import Path

ICODE_TARGET = "/opt/icode-host"


def build_mounts(icode_root: Path) -> list[dict]:
    return [{"type": "bind", "source": str(icode_root), "target": ICODE_TARGET, "read_only": True}]


def _overlaps(a: Path, b: Path) -> bool:
    return a == b or a in b.parents or b in a.parents


def check_mounts(mounts: object, icode_root: Path, forbidden: list[Path]) -> list[str]:
    errors: list[str] = []
    if not isinstance(mounts, list):
        return ["mounts must be a JSON list"]
    if len(mounts) != 1:
        errors.append(f"expected exactly one mount (the iCode tree), got {len(mounts)}")
    root = icode_root.resolve()
    for index, mount in enumerate(mounts):
        if not isinstance(mount, dict):
            errors.append(f"mount {index} is not an object")
            continue
        if mount.get("type") != "bind":
            errors.append(f"mount {index} type must be bind")
        if mount.get("target") != ICODE_TARGET:
            errors.append(f"mount {index} target must be {ICODE_TARGET}")
        if mount.get("read_only") is not True:
            errors.append(f"mount {index} must set read_only: true")
        source = str(mount.get("source") or "")
        if not source or Path(source).resolve() != root:
            errors.append(f"mount {index} source {source or '<empty>'} is not the iCode tree {root}")
    for path in forbidden:
        if not str(path):
            continue
        bench = path.resolve()
        if _overlaps(root, bench):
            errors.append(f"iCode tree {root} overlaps benchmark path {bench}")
    return errors


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description="Agent mounts for the evaluate phase")
    sub = parser.add_subparsers(dest="cmd", required=True)
    build = sub.add_parser("build")
    build.add_argument("--icode-root", required=True)
    check = sub.add_parser("check")
    check.add_argument("--mounts", required=True, help="JSON list passed to harbor --mounts")
    check.add_argument("--icode-root", required=True)
    check.add_argument("--forbid", action="append", default=[], help="benchmark dir the tree must not overlap")
    args = parser.parse_args(argv)
    if args.cmd == "build":
        print(json.dumps(build_mounts(Path(args.icode_root))))
        return 0
    try:
        mounts = json.loads(args.mounts)
    except json.JSONDecodeError as exc:
        print(f"ERROR: --mounts is not JSON: {exc}", file=sys.stderr)
        return 1
    errors = check_mounts(mounts, Path(args.icode_root), [Path(p) for p in args.forbid if p])
    for err in errors:
        print(f"ERROR: agent mounts: {err}", file=sys.stderr)
    if errors:
        return 1
    print(f"OK agent mounts: one read-only bind of {Path(args.icode_root).resolve()} at {ICODE_TARGET}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
