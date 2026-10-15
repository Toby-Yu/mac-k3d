#!/usr/bin/env python3
"""The Linux platforms a harness input or bundle can run on.

    python3 harness_platforms.py PATH     prints e.g. "linux/amd64 linux/arm64"

PATH is what a build prepares its harness from:

  a harness store entry or bundle   its platform; an iCode release bundle
                                    ("any") holds every drop of the upload
  a workswarm wheel                 its WHEEL tags; a pure wheel runs anywhere
  a .tar.gz of wheels               the platforms every distribution has a wheel for
  an iCode drop, folder or .tar.gz  each file named icode, by its header; a
                                    script runs anywhere, a macOS build nowhere

A git input builds for any platform and is never asked about. Exit 1 when the
input names no Linux platform: the build then fails as it always has.
"""

from __future__ import annotations

import json
import os
import sys
import tarfile
import zipfile
from pathlib import Path, PurePosixPath

sys.path.insert(0, str(Path(__file__).resolve().parent))
from harness_store import META, SCHEMA, TREE  # noqa: E402

AMD64 = "linux/amd64"
ARM64 = "linux/arm64"
ALL = frozenset({AMD64, ARM64})
NONE: frozenset[str] = frozenset()
ELF_MACHINE = {62: AMD64, 183: ARM64}
MACHO = {b"\xcf\xfa\xed\xfe", b"\xce\xfa\xed\xfe", b"\xca\xfe\xba\xbe", b"\xbe\xba\xfe\xca"}


def header_platforms(head: bytes) -> frozenset[str]:
    """What an executable named icode runs on, from its first 20 bytes."""
    if head[:4] == b"\x7fELF" and len(head) >= 20:
        plat = ELF_MACHINE.get(int.from_bytes(head[18:20], "little"))
        return frozenset({plat}) if plat else NONE
    if head[:4] in MACHO:
        return NONE
    return ALL


def tag_platforms(tag: str) -> frozenset[str]:
    """The Linux platforms of one wheel platform tag (manylinux_2_28_aarch64, any, ...)."""
    if tag == "any":
        return ALL
    if not tag.startswith(("linux_", "manylinux", "musllinux_")):
        return NONE
    if tag.endswith(("_x86_64", "_amd64")):
        return frozenset({AMD64})
    if tag.endswith(("_aarch64", "_arm64")):
        return frozenset({ARM64})
    return NONE


def _tags_platforms(tags: list[str]) -> frozenset[str]:
    out: set[str] = set()
    for tag in tags:
        out |= tag_platforms(tag)
    return frozenset(out)


def wheel_name_platforms(name: str) -> tuple[str, frozenset[str]]:
    """(distribution, platforms) from a wheel file name; tags may be dot-joined."""
    parts = PurePosixPath(name).name[: -len(".whl")].split("-")
    if len(parts) < 5:
        return "", NONE
    return parts[0].lower().replace(".", "_"), _tags_platforms(parts[-1].split("."))


def wheel_platforms(path: Path) -> frozenset[str]:
    with zipfile.ZipFile(path) as zf:
        info = next(
            (n for n in zf.namelist() if PurePosixPath(n).parts[0].endswith(".dist-info") and n.endswith("/WHEEL")),
            None,
        )
        if info is None:
            return NONE
        lines = zf.read(info).decode("utf-8", "replace").splitlines()
    tags = [ln.split(":", 1)[1].strip().rsplit("-", 1)[-1] for ln in lines if ln.startswith("Tag:")]
    return _tags_platforms(tags) if tags else ALL


def _intersect(dists: dict[str, set[str]]) -> frozenset[str]:
    if not dists:
        return NONE
    out = set(ALL)
    for plats in dists.values():
        out &= plats
    return frozenset(out)


def _icode_member(name: str) -> bool:
    return PurePosixPath(name).name == "icode"


def tar_platforms(path: Path, prefix: str = "") -> frozenset[str]:
    """A bundle's platform, or what the wheels / icode files inside a .tar.gz run on."""
    dists: dict[str, set[str]] = {}
    found: set[str] = set()
    seen_icode = False
    with tarfile.open(path, mode="r:*") as tar:
        for member in tar:
            if member.name == META:
                fh = tar.extractfile(member)
                doc = json.loads(fh.read().decode("utf-8")) if fh else {}
                if isinstance(doc, dict) and doc.get("schema") == SCHEMA:
                    platform = str(doc.get("platform") or "")
                    if platform != "any":
                        return frozenset({platform}) & ALL
                    prefix = TREE + "/"
                continue
            if not member.isfile() or not member.name.startswith(prefix):
                continue
            if member.name.endswith(".whl"):
                dist, plats = wheel_name_platforms(member.name)
                if dist:
                    dists.setdefault(dist, set()).update(plats)
            elif _icode_member(member.name):
                fh = tar.extractfile(member)
                seen_icode = True
                found |= header_platforms(fh.read(20) if fh else b"")
    if dists:
        return _intersect(dists)
    return frozenset(found) if seen_icode else NONE


def _is_archive(path: Path) -> bool:
    with path.open("rb") as fh:
        head = fh.read(2)
    return head == b"\x1f\x8b" or path.name.endswith((".tar.gz", ".tgz", ".tar"))


def dir_platforms(root: Path) -> frozenset[str]:
    """A store entry's platform, or each icode (loose or in a drop archive) under a folder."""
    meta = root / META
    if meta.is_file():
        try:
            doc = json.loads(meta.read_text(encoding="utf-8"))
        except (OSError, json.JSONDecodeError):
            doc = {}
        platform = str(doc.get("platform") or "") if isinstance(doc, dict) else ""
        if platform != "any":
            return frozenset({platform}) & ALL
        root = root / TREE
    found: set[str] = set()
    for dirpath, _dirs, files in os.walk(root):
        for name in files:
            path = Path(dirpath) / name
            if path.is_symlink():
                continue
            if name == "icode":
                with path.open("rb") as fh:
                    found |= header_platforms(fh.read(20))
            elif name.endswith((".tar.gz", ".tgz")):
                found |= tar_platforms(path)
    return frozenset(found)


def platforms(path: Path) -> frozenset[str]:
    if path.is_dir():
        return dir_platforms(path)
    with path.open("rb") as fh:
        head = fh.read(20)
    if head[:4] == b"PK\x03\x04":
        return wheel_platforms(path)
    if _is_archive(path):
        return tar_platforms(path)
    return header_platforms(head)


def main(argv: list[str] | None = None) -> int:
    args = sys.argv[1:] if argv is None else argv
    if len(args) != 1:
        print("usage: harness_platforms.py PATH", file=sys.stderr)
        return 2
    try:
        found = platforms(Path(args[0]))
    except (OSError, tarfile.TarError, zipfile.BadZipFile, ValueError) as exc:
        print(f"harness_platforms: {args[0]}: {exc}", file=sys.stderr)
        return 1
    if not found:
        return 1
    print(" ".join(sorted(found)))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
