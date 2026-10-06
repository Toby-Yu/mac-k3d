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


TOOLCHAIN_ENV = ROOT / "pipeline" / "config" / "toolchain.env"
COMMON = STAGES / "_common.sh"


def toolchain_pins() -> dict[str, str]:
    pins = {}
    for line in TOOLCHAIN_ENV.read_text(encoding="utf-8").splitlines():
        line = line.strip()
        if line and not line.startswith("#") and "=" in line:
            key, value = line.split("=", 1)
            pins[key.strip()] = value.strip()
    return pins


class ToolchainAlignmentTests(unittest.TestCase):
    """The env phase checks what `mac-k3d setup` installs, from one pin file."""

    def test_env_host_checks_git_and_python_against_the_pin(self):
        host = (STAGES / "env" / "host.sh").read_text(encoding="utf-8")
        self.assertIn("have git ||", host)
        self.assertIn("have python3 ||", host)
        self.assertIn('python3 - "$PYTHON_MIN"', host)
        self.assertIn("PYTHON_MIN", toolchain_pins())
        self.assertNotRegex(host, r"3\.11", "host.sh must take the python floor from toolchain.env")
        self.assertIn("launchctl", host, "host.sh must check the macOS LaunchAgent too")

    def test_common_reads_ram_and_disk_minimums_from_toolchain_env(self):
        common = COMMON.read_text(encoding="utf-8")
        self.assertIn('"${MAC_K3D_MIN_RAM_GB:-$MIN_RAM_GB}"', common)
        self.assertIn('"${MAC_K3D_MIN_DISK_GB:-$WORKER_MIN_DISK_GB}"', common)
        self.assertNotRegex(common, r"MIN_(RAM|DISK)_GB:-[0-9]")
        pins = toolchain_pins()
        for key in ("MIN_RAM_GB", "WORKER_MIN_DISK_GB"):
            self.assertRegex(pins.get(key, ""), r"^[0-9]+$", key)
        self.assertIn("sysctl -n hw.memsize", common, "RAM check needs a macOS branch")
        self.assertIn("route -n get", common, "MTU warning needs a macOS branch")

    def test_dependencies_doc_lists_every_pin(self):
        doc = (ROOT / "docs" / "dependencies.md").read_text(encoding="utf-8")
        pins = toolchain_pins()
        self.assertIn("JAVA_MAJOR", pins)
        common = COMMON.read_text(encoding="utf-8")
        for key in ("DEEPSWE_REF", "LOLBENCH_REF"):
            match = re.search(rf'^export {key}="\$\{{{key}:-([0-9a-f]+)\}}"', common, re.M)
            self.assertIsNotNone(match, f"{key} default not found in _common.sh")
            pins[key] = match.group(1)
        for key, value in pins.items():
            self.assertIn(f"`{key}={value}`", doc, f"docs/dependencies.md must list {key}={value}")


class MacPortabilityTests(unittest.TestCase):
    """macOS /bin/bash is 3.2: the guard must run before anything that needs 4.4."""

    BASH4 = re.compile(r"\bmapfile\b|\breadarray\b|\bwait -n\b|\bdeclare -A\b|\$\{[A-Za-z_]+(,,|\^\^)\}|;;&|\|&")

    def test_bash_guard_runs_before_any_bash4_syntax(self):
        run_all = (STAGES / "run_all.sh").read_text(encoding="utf-8")
        commands = [l for l in run_all.splitlines() if l.strip() and not l.startswith("#")]
        self.assertTrue(commands[0].startswith("set -"), commands[0])
        self.assertTrue(commands[1].startswith("DIR="), commands[1])
        self.assertEqual(commands[2], 'source "$DIR/_common.sh"')
        common = COMMON.read_text(encoding="utf-8")
        guard = common.index("\nrequire_bash_min\n")
        self.assertIn("BASH_VERSINFO", common[:guard])
        code = "\n".join(l for l in common[:guard].splitlines() if not l.lstrip().startswith("#"))
        self.assertIsNone(self.BASH4.search(code), "bash 4 syntax before the guard")
        host = (STAGES / "env" / "host.sh").read_text(encoding="utf-8")
        self.assertLess(host.index('/_common.sh"'), host.index("have python3"))

    def test_no_mapfile_or_wait_n_in_stages(self):
        for path in sorted(STAGES.rglob("*.sh")):
            for n, line in enumerate(path.read_text(encoding="utf-8").splitlines(), 1):
                if line.lstrip().startswith("#"):
                    continue
                self.assertNotRegex(line, r"\bmapfile\b|\bwait -n\b", f"{path.relative_to(ROOT)}:{n}")

    def test_old_bash_gets_the_clear_error(self):
        with tempfile.TemporaryDirectory() as tmp:
            env = {**os.environ, "BASH_MIN": "99.0", "HOME": tmp, "MAC_K3D_EVAL_WORKDIR": str(Path(tmp) / "eval")}
            proc = subprocess.run(
                ["bash", str(STAGES / "run_all.sh")], capture_output=True, text=True, check=False, env=env
            )
            self.assertEqual(proc.returncode, 1, proc.stdout + proc.stderr)
            self.assertIn("is older than 99.0 (BASH_MIN in pipeline/config/toolchain.env)", proc.stderr)
            self.assertIn("brew install bash", proc.stderr)
            self.assertFalse((Path(tmp) / "eval").exists(), "the guard must stop before WORKDIR is created")


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
