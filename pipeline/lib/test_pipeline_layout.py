#!/usr/bin/env python3
"""The phase layout under pipeline/stages: phases, steps, and where Harbor flags live."""

from __future__ import annotations

import os
import re
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent.parent
LIB = Path(__file__).resolve().parent
STAGES = ROOT / "pipeline" / "stages"
HARBOR_CMD = STAGES / "evaluate" / "harbor_cmd.sh"
# Flags that only make sense on a `harbor run` command line. (--mounts and
# --jobs-dir are also arguments of agent_mounts.py and canary_verdict.py.)
HARBOR_FLAGS = (
    "--allow-agent-host",
    "--env-file",
    "--agent-setup-timeout-multiplier",
    "--disable-verification",
    "--override-cpus",
    "--override-memory-mb",
)


def _array(text: str, name: str) -> list[str]:
    match = re.search(rf"^{name}=\(([^)]*)\)", text, re.M)
    return match.group(1).split() if match else []


def phases() -> list[str]:
    return _array((STAGES / "run_all.sh").read_text(encoding="utf-8"), "PHASES")


def steps(phase: str) -> list[str]:
    return _array((STAGES / f"{phase}.sh").read_text(encoding="utf-8"), "STEPS")


class PhaseLayoutTests(unittest.TestCase):
    def test_every_phase_and_step_exists(self):
        self.assertEqual(phases(), ["env", "tasks", "evaluate", "anticheat", "score", "report", "archive"])
        for phase in phases():
            listed = steps(phase)
            self.assertTrue(listed, f"{phase}.sh has no STEPS")
            for step in listed:
                self.assertTrue(step.startswith(f"{phase}/"), f"{phase}.sh runs {step} from another phase")
                self.assertTrue((STAGES / f"{step}.sh").is_file(), f"{phase}.sh lists missing {step}.sh")

    def test_every_step_file_is_run_by_its_phase(self):
        listed = {step for phase in phases() for step in steps(phase)}
        for path in sorted(STAGES.glob("*/*.sh")):
            rel = path.relative_to(STAGES).with_suffix("").as_posix()
            if path == HARBOR_CMD:
                continue
            self.assertIn(rel, listed, f"{rel}.sh is not in any phase's STEPS")

    def test_only_evaluate_steps_source_harbor_cmd(self):
        for path in sorted(STAGES.rglob("*.sh")):
            if "harbor_cmd.sh" not in path.read_text(encoding="utf-8") or path == HARBOR_CMD:
                continue
            self.assertEqual(path.parent.name, "evaluate", path)

    def test_no_numbered_stage_names_remain(self):
        pattern = re.compile(r"\bp[0-8]c?_[a-z_]+\.sh\b")
        roots = [ROOT / "pipeline", ROOT / "src", ROOT / "scripts", ROOT / "tests"]
        for root in roots:
            for path in root.rglob("*"):
                if not path.is_file() or path.suffix not in {".sh", ".py", ".rs", ".json", ".toml"}:
                    continue
                if path == Path(__file__).resolve():
                    continue
                text = path.read_text(encoding="utf-8", errors="ignore")
                self.assertIsNone(pattern.search(text), f"{path.relative_to(ROOT)} still names an old stage")

    def test_harbor_flags_only_in_harbor_cmd(self):
        for path in sorted(STAGES.rglob("*.sh")):
            if path == HARBOR_CMD:
                continue
            text = path.read_text(encoding="utf-8")
            rel = path.relative_to(ROOT)
            for flag in HARBOR_FLAGS:
                self.assertFalse(flag in text, f"{rel} builds Harbor flags ({flag})")
            self.assertFalse("cmd+=(" in text, f"{rel} builds a command line")
            self.assertIsNone(re.search(r"^\s*(cmd=\()?harbor run\b", text, re.M), f"{rel} runs harbor itself")
        cmd = HARBOR_CMD.read_text(encoding="utf-8")
        for flag in (*HARBOR_FLAGS, "--mounts", "--jobs-dir"):
            self.assertIn(flag, cmd)

    def test_agent_hosts_come_from_the_allowlist_file(self):
        literal = re.compile(r"--allow-agent-host\s+[\"']?[A-Za-z0-9-]+(\.[A-Za-z0-9-]+)+")
        for path in sorted((ROOT / "pipeline").rglob("*.sh")):
            text = path.read_text(encoding="utf-8")
            self.assertIsNone(literal.search(text), f"{path.relative_to(ROOT)} hard-codes an agent host")
        cmd = HARBOR_CMD.read_text(encoding="utf-8")
        self.assertIn('python3 "$PIPELINE_LIB/network_allowlist.py" hosts', cmd)


class RunAllTests(unittest.TestCase):
    def _run(self, tmp: str, **env: str) -> subprocess.CompletedProcess[str]:
        full = {**os.environ, "MAC_K3D_EVAL_WORKDIR": str(Path(tmp) / "eval"), "HOME": tmp, **env}
        for key in ("CANARY", "OFFICIAL", "BUILD_NUMBER"):
            if key not in env:
                full.pop(key, None)
        return subprocess.run(
            ["bash", str(STAGES / "run_all.sh")], capture_output=True, text=True, check=False, env=full
        )

    def test_unknown_phase_is_refused(self):
        with tempfile.TemporaryDirectory() as tmp:
            proc = self._run(tmp, MAC_K3D_PHASE="prepare")
            self.assertNotEqual(proc.returncode, 0)
            self.assertIn("MAC_K3D_PHASE must be all or one of: env tasks evaluate", proc.stderr)

    def test_canary_only_skips_every_phase_after_evaluate(self):
        for phase in ("anticheat", "score", "report", "archive"):
            with tempfile.TemporaryDirectory() as tmp:
                proc = self._run(tmp, MAC_K3D_PHASE=phase, CANARY="only")
                self.assertEqual(proc.returncode, 0, (phase, proc.stdout + proc.stderr))
                self.assertIn("canary only: no rollouts to score", proc.stdout)
                self.assertFalse((Path(tmp) / "eval" / "report_dir.txt").exists())


class ArchiveRunTests(unittest.TestCase):
    def test_archive_needs_the_report_first(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            tasks = root / "tasks.txt"
            tasks.write_text("alpha\n", encoding="utf-8")
            proc = subprocess.run(
                [
                    sys.executable,
                    str(LIB / "archive_run.py"),
                    "--report-dir", str(root / "run"),
                    "--harness-dir", str(root / "harness"),
                    "--task-file", str(tasks),
                    "--backup-root", str(root / "backup"),
                ],
                capture_output=True,
                text=True,
                check=False,
            )
            self.assertNotEqual(proc.returncode, 0)
            self.assertIn("run the report phase first", proc.stderr)
            self.assertFalse((root / "backup").exists())

    def test_backup_carries_the_cost_analysis(self):
        from archive_run import REPORT_FILES

        self.assertIn("cost-token-report.md", REPORT_FILES)
        archive = (STAGES / "archive.sh").read_text(encoding="utf-8")
        listed = steps("archive")
        self.assertLess(listed.index("archive/analysis"), listed.index("archive/backup"), archive)
        render = (LIB / "render_report.py").read_text(encoding="utf-8")
        self.assertNotIn("backup_run", render)
        self.assertNotIn("--backup-root", render)


if __name__ == "__main__":
    unittest.main()
