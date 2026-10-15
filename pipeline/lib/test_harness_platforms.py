#!/usr/bin/env python3
"""harness_platforms.py: the Linux platforms a harness release or bundle runs on,
which decide whether an arm64 worker can run DeepSWE in native containers."""

from __future__ import annotations

import io
import json
import subprocess
import sys
import tarfile
import tempfile
import unittest
import zipfile
from pathlib import Path

LIB = Path(__file__).resolve().parent
sys.path.insert(0, str(LIB))

import harness_platforms as hp  # noqa: E402
import harness_store as hs  # noqa: E402

AMD64 = frozenset({hp.AMD64})
ARM64 = frozenset({hp.ARM64})


def elf(machine: int) -> bytes:
    return b"\x7fELF\x02\x01\x01" + b"\0" * 11 + machine.to_bytes(2, "little") + b"\0" * 44


X86_ELF = elf(62)
ARM_ELF = elf(183)


def add(tar: tarfile.TarFile, name: str, body: bytes, mode: int = 0o644) -> None:
    info = tarfile.TarInfo(name)
    info.size, info.mode = len(body), mode
    tar.addfile(info, io.BytesIO(body))


def wheel(path: Path, dist: str, *tags: str) -> Path:
    with zipfile.ZipFile(path, "w") as zf:
        lines = "".join(f"Tag: {tag}\n" for tag in tags)
        zf.writestr(f"{dist}.dist-info/WHEEL", f"Wheel-Version: 1.0\n{lines}")
        zf.writestr(f"{dist}.dist-info/METADATA", "Name: x\n")
    return path


class HeaderAndTagTest(unittest.TestCase):
    def test_headers(self):
        self.assertEqual(hp.header_platforms(X86_ELF[:20]), AMD64)
        self.assertEqual(hp.header_platforms(ARM_ELF[:20]), ARM64)
        self.assertEqual(hp.header_platforms(elf(40)[:20]), hp.NONE, "32-bit ARM runs in neither")
        self.assertEqual(hp.header_platforms(b"\xcf\xfa\xed\xfe" + b"\0" * 16), hp.NONE, "a macOS build")
        self.assertEqual(hp.header_platforms(b"#!/bin/sh\nexec x\n"), hp.ALL, "a script")

    def test_wheel_tags(self):
        self.assertEqual(hp.tag_platforms("any"), hp.ALL)
        self.assertEqual(hp.tag_platforms("manylinux_2_28_aarch64"), ARM64)
        self.assertEqual(hp.tag_platforms("manylinux2014_x86_64"), AMD64)
        self.assertEqual(hp.tag_platforms("musllinux_1_2_x86_64"), AMD64)
        self.assertEqual(hp.tag_platforms("macosx_11_0_arm64"), hp.NONE)
        self.assertEqual(hp.tag_platforms("win_amd64"), hp.NONE)

    def test_wheel_file_names(self):
        self.assertEqual(
            hp.wheel_name_platforms("pydantic_core-2.27.2-cp311-cp311-manylinux_2_17_x86_64.manylinux2014_x86_64.whl"),
            ("pydantic_core", AMD64),
        )
        self.assertEqual(hp.wheel_name_platforms("wheelhouse/Workswarm-0.2.5b1-py3-none-any.whl"), ("workswarm", hp.ALL))
        self.assertEqual(hp.wheel_name_platforms("broken.whl"), ("", hp.NONE))


class InputTest(unittest.TestCase):
    def setUp(self):
        self.tmp = Path(tempfile.mkdtemp())

    def tearDown(self):
        hs._remove(self.tmp)

    def test_a_wheel_by_its_tags(self):
        self.assertEqual(hp.platforms(wheel(self.tmp / "a.whl", "workswarm-0.2.5b1", "py3-none-any")), hp.ALL)
        x86 = wheel(self.tmp / "b.whl", "workswarm-0.2.5b1", "cp311-cp311-manylinux_2_28_x86_64")
        self.assertEqual(hp.platforms(x86), AMD64)
        both = wheel(self.tmp / "c.whl", "workswarm-0.2.5b1",
                     "cp311-cp311-manylinux_2_28_x86_64", "cp311-cp311-manylinux_2_28_aarch64")
        self.assertEqual(hp.platforms(both), hp.ALL)

    def test_a_wheelhouse_runs_where_every_distribution_has_a_wheel(self):
        house = self.tmp / "wheelhouse.tar.gz"
        with tarfile.open(house, "w:gz") as tar:
            add(tar, "wheelhouse/workswarm-0.2.5b1-py3-none-any.whl", b"PK")
            add(tar, "wheelhouse/pydantic_core-2.27.2-cp311-cp311-manylinux_2_17_x86_64.whl", b"PK")
            add(tar, "wheelhouse/pydantic_core-2.27.2-cp311-cp311-manylinux_2_17_aarch64.whl", b"PK")
            add(tar, "wheelhouse/orjson-3.10.0-cp311-cp311-manylinux_2_17_x86_64.whl", b"PK")
        self.assertEqual(hp.platforms(house), AMD64, "orjson has no aarch64 wheel")
        with tarfile.open(self.tmp / "both.tar.gz", "w:gz") as tar:
            add(tar, "wheelhouse/workswarm-0.2.5b1-py3-none-any.whl", b"PK")
            add(tar, "wheelhouse/orjson-3.10.0-cp311-cp311-manylinux_2_17_x86_64.whl", b"PK")
            add(tar, "wheelhouse/orjson-3.10.0-cp311-cp311-manylinux_2_17_aarch64.whl", b"PK")
        self.assertEqual(hp.platforms(self.tmp / "both.tar.gz"), hp.ALL)

    def test_icode_drops_by_the_header_of_each_icode(self):
        x86 = self.tmp / "icode-linux-x86_64-full-v1.0.0.tar.gz"
        with tarfile.open(x86, "w:gz") as tar:
            add(tar, "icode-linux-x86_64-full-v1.0.0/icode", X86_ELF, 0o755)
            add(tar, "icode-linux-x86_64-full-v1.0.0/README.md", b"hi\n")
        self.assertEqual(hp.platforms(x86), AMD64)
        both = self.tmp / "both.tar.gz"
        with tarfile.open(both, "w:gz") as tar:
            add(tar, "icode-linux-x86_64-full-v1.0.0/icode", X86_ELF, 0o755)
            add(tar, "icode-linux-aarch64-full-v1.0.0/icode", ARM_ELF, 0o755)
        self.assertEqual(hp.platforms(both), hp.ALL)
        folder = self.tmp / "drops"
        (folder / "icode-linux-aarch64-full-v1.0.0").mkdir(parents=True)
        (folder / "icode-linux-aarch64-full-v1.0.0" / "icode").write_bytes(ARM_ELF)
        self.assertEqual(hp.platforms(folder), ARM64)
        (folder / "icode-linux-x86_64-full-v1.0.0.tar.gz").write_bytes(x86.read_bytes())
        self.assertEqual(hp.platforms(folder), hp.ALL, "a loose drop and a drop archive in one folder")
        mac = self.tmp / "icode"
        mac.write_bytes(b"\xcf\xfa\xed\xfe" + b"\0" * 40)
        self.assertEqual(hp.platforms(mac), hp.NONE)

    def test_a_bundle_and_a_store_entry_by_their_platform(self):
        tree = self.tmp / "tree"
        (tree / "bin").mkdir(parents=True)
        (tree / "bin" / "run").write_text("#!/bin/sh\n")
        meta = {"harness": "jiuwenswarm", "identity": {"mode": "git", "sha": "a" * 40},
                "platform": "linux/arm64", "recipe": "jiuwenswarm-v3", "schema": hs.SCHEMA}
        bundle = self.tmp / "bundle.tar.gz"
        hs.pack(tree, meta, bundle)
        self.assertEqual(hp.platforms(bundle), ARM64)
        entry = hs.put(self.tmp / "store", "jiuwenswarm", bundle)
        self.assertEqual(hp.platforms(entry), ARM64)

    def test_an_icode_release_bundle_any_by_the_drops_it_holds(self):
        tree = self.tmp / "tree"
        (tree / "icode-linux-x86_64-full-v1.0.0").mkdir(parents=True)
        (tree / "icode-linux-x86_64-full-v1.0.0" / "icode").write_bytes(X86_ELF)
        meta = {"harness": "icode", "identity": {"mode": "release", "sha256": "b" * 64},
                "platform": "any", "recipe": "icode-v1", "schema": hs.SCHEMA}
        bundle = self.tmp / "bundle.tar.gz"
        hs.pack(tree, meta, bundle)
        self.assertEqual(hp.platforms(bundle), AMD64)
        entry = hs.put(self.tmp / "store", "icode", bundle)
        self.assertEqual(hp.platforms(entry), AMD64)

    def test_cli(self):
        script = str(LIB / "harness_platforms.py")
        x86 = wheel(self.tmp / "x.whl", "workswarm-0.2.5b1", "cp311-cp311-manylinux_2_28_x86_64")
        ok = subprocess.run([sys.executable, script, str(x86)], capture_output=True, text=True)
        self.assertEqual((ok.returncode, ok.stdout), (0, "linux/amd64\n"))
        pure = wheel(self.tmp / "p.whl", "workswarm-0.2.5b1", "py3-none-any")
        both = subprocess.run([sys.executable, script, str(pure)], capture_output=True, text=True)
        self.assertEqual(both.stdout, "linux/amd64 linux/arm64\n")
        (self.tmp / "empty").mkdir()
        none = subprocess.run([sys.executable, script, str(self.tmp / "empty")], capture_output=True, text=True)
        self.assertEqual((none.returncode, none.stdout), (1, ""))
        bad = self.tmp / "bad.tar.gz"
        bad.write_bytes(b"\x1f\x8bnot gzip")
        broken = subprocess.run([sys.executable, script, str(bad)], capture_output=True, text=True)
        self.assertEqual(broken.returncode, 1)
        self.assertIn("harness_platforms:", broken.stderr)


if __name__ == "__main__":
    unittest.main()
