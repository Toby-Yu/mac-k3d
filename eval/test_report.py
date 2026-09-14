#!/usr/bin/env python3
"""Unit tests for check_report and score_results (no network)."""

from __future__ import annotations

import json
import os
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
sys.path.insert(0, str(Path(__file__).resolve().parent))

from check_report import validate  # noqa: E402
from score_results import pass_at_1  # noqa: E402


FIXTURE = ROOT / "eval" / "testdata" / "report-min.json"


class PassAt1Tests(unittest.TestCase):
    def test_ratio(self):
        self.assertEqual(pass_at_1(1, 1), 1.0)
        self.assertEqual(pass_at_1(0, 1), 0.0)
        self.assertEqual(pass_at_1(1, 2), 0.5)
        self.assertEqual(pass_at_1(0, 0), 0.0)


class CheckReportTests(unittest.TestCase):
    def test_fixture_ok(self):
        doc = json.loads(FIXTURE.read_text(encoding="utf-8"))
        self.assertEqual(validate(doc), [])

    def test_missing_keys(self):
        errors = validate({"harness": "icode"})
        self.assertTrue(any("missing llm_model_id" in e for e in errors))
        self.assertTrue(any("missing totals" in e for e in errors))

    def test_cli_fixture(self):
        proc = subprocess.run(
            [sys.executable, str(ROOT / "eval" / "check_report.py"), str(FIXTURE)],
            check=False,
            capture_output=True,
            text=True,
        )
        self.assertEqual(proc.returncode, 0, proc.stdout + proc.stderr)
        self.assertIn("OK report schema", proc.stdout)


class ScoreResultsTests(unittest.TestCase):
    def test_merges_meta_and_pass_at_1(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            tasks = root / "tasks" / "example-task"
            tasks.mkdir(parents=True)
            harness = root / "harness"
            harness.mkdir()
            (harness / "meta.json").write_text(
                json.dumps(
                    {
                        "access_date_utc": "2026-09-14T00:00:00Z",
                        "duration_seconds": 2,
                        "llm_model_id": "deepseek-v4-pro",
                        "llm_model_served": "deepseek-v4-pro",
                        "token_usage": {"prompt": 3, "completion": 1, "total": 4},
                    }
                ),
                encoding="utf-8",
            )
            (harness / "example-task").mkdir()
            (harness / "example-task" / "result.json").write_text(
                json.dumps(
                    {
                        "resolved": True,
                        "FAIL_TO_PASS": ["t::fix"],
                        "PASS_TO_PASS": ["t::ok"],
                    }
                ),
                encoding="utf-8",
            )
            baseline = root / "baseline"
            (baseline / "example-task").mkdir(parents=True)
            (baseline / "example-task" / "agent.patch").write_text("diff --git a b\n", encoding="utf-8")
            (baseline / "summary.json").write_text(
                json.dumps(
                    [
                        {
                            "id": "example-task",
                            "ok": True,
                            "patch_path": str(baseline / "example-task" / "agent.patch"),
                            "token_usage": {"prompt": 5, "completion": 2, "total": 7},
                            "duration_seconds": 1.5,
                        }
                    ]
                ),
                encoding="utf-8",
            )
            (baseline / "meta.json").write_text(
                json.dumps(
                    {
                        "access_date_utc": "2026-09-14T00:00:00Z",
                        "llm_model_id": "deepseek-v4-pro",
                        "llm_model_served": "deepseek-v4-flash",
                        "duration_seconds": 1.5,
                        "token_usage": {"prompt": 5, "completion": 2, "total": 7},
                    }
                ),
                encoding="utf-8",
            )
            out = root / "out.json"
            proc = subprocess.run(
                [
                    sys.executable,
                    str(ROOT / "eval" / "score_results.py"),
                    "--harness-dir",
                    str(harness),
                    "--baseline-dir",
                    str(baseline),
                    "--tasks-dir",
                    str(root / "tasks"),
                    "--n-tasks",
                    "1",
                    "--out",
                    str(out),
                ],
                check=False,
                capture_output=True,
                text=True,
                env={**os.environ, "DEEPSEEK_MODEL": "deepseek-v4-pro"},
            )
            self.assertEqual(proc.returncode, 0, proc.stdout + proc.stderr)
            doc = json.loads(out.read_text(encoding="utf-8"))
            self.assertEqual(validate(doc), [])
            self.assertEqual(doc["totals"]["pass_at_1_harness"], 1.0)
            self.assertEqual(doc["llm_model_id"], "deepseek-v4-pro")
            self.assertEqual(doc["llm_model_served"], "deepseek-v4-flash")
            self.assertEqual(doc["token_usage"]["total"], 11)
            self.assertEqual(doc["duration_seconds"]["total"], 3.5)
            self.assertEqual(doc["tasks"][0]["f2p"], ["t::fix"])


class LocalEnvAndPierAdapterTests(unittest.TestCase):
    def test_env_example_has_no_secret(self):
        text = (ROOT / ".env.example").read_text(encoding="utf-8")
        self.assertIn("DEEPSEEK_API_KEY=", text)
        self.assertNotRegex(text, r"DEEPSEEK_API_KEY=sk-")

    def test_gitignore_covers_dotenv(self):
        proc = subprocess.run(
            ["git", "check-ignore", "-v", ".env"],
            cwd=ROOT,
            check=False,
            capture_output=True,
            text=True,
        )
        self.assertEqual(proc.returncode, 0, proc.stderr)
        self.assertIn(".env", proc.stdout)

    def test_common_loads_env_file_without_overwrite(self):
        common = ROOT / "scripts" / "eval" / "_common.sh"
        with tempfile.TemporaryDirectory() as tmp:
            envf = Path(tmp) / "keys.env"
            envf.write_text(
                "DEEPSEEK_API_KEY=test-from-file\nDEEPSEEK_MODEL=deepseek-v4-pro\n",
                encoding="utf-8",
            )
            envf.chmod(0o600)
            script = f"""
unset DEEPSEEK_API_KEY
export MAC_K3D_ENV_FILE={envf}
# isolate workdir so tests do not touch the real eval-work tree
export MAC_K3D_EVAL_WORKDIR={tmp}/eval-work
source {common}
printf '%s' "$DEEPSEEK_API_KEY"
"""
            proc = subprocess.run(
                ["bash", "-c", script],
                check=False,
                capture_output=True,
                text=True,
            )
            self.assertEqual(proc.returncode, 0, proc.stdout + proc.stderr)
            self.assertTrue(proc.stdout.endswith("test-from-file"), proc.stdout)
            self.assertNotIn("test-from-file", proc.stderr)

            script_keep = f"""
export DEEPSEEK_API_KEY=already-set
export MAC_K3D_ENV_FILE={envf}
export MAC_K3D_EVAL_WORKDIR={tmp}/eval-work2
source {common}
printf '%s' "$DEEPSEEK_API_KEY"
"""
            keep = subprocess.run(
                ["bash", "-c", script_keep],
                check=False,
                capture_output=True,
                text=True,
            )
            self.assertEqual(keep.returncode, 0, keep.stdout + keep.stderr)
            self.assertTrue(keep.stdout.endswith("already-set"), keep.stdout)

    def test_p5_uses_import_path_not_bare_agent_icode(self):
        text = (ROOT / "scripts" / "eval" / "p5_harness.sh").read_text(encoding="utf-8")
        self.assertIn("icode_pier_agent:ICodeAgent", text)
        self.assertIn("No such option", text)
        self.assertNotIn("--agent icode", text)

    def test_icode_adapter_module_parses(self):
        path = ROOT / "eval" / "icode_pier_agent.py"
        src = path.read_text(encoding="utf-8")
        compile(src, str(path), "exec")
        self.assertIn("class ICodeAgent", src)

    def test_icode_adapter_imports_when_pier_installed(self):
        env = {**os.environ, "PYTHONPATH": str(ROOT / "eval")}
        proc = subprocess.run(
            [
                sys.executable,
                "-c",
                "from icode_pier_agent import ICodeAgent; assert ICodeAgent.name() == 'icode'",
            ],
            check=False,
            capture_output=True,
            text=True,
            env=env,
        )
        if proc.returncode != 0 and "ModuleNotFoundError" in (proc.stderr + proc.stdout):
            pier = subprocess.run(
                ["bash", "-lc", "command -v pier"],
                check=False,
                capture_output=True,
                text=True,
            )
            if pier.returncode != 0 or not pier.stdout.strip():
                self.skipTest("pier not installed")
            shebang = Path(pier.stdout.strip()).read_text(encoding="utf-8", errors="ignore").splitlines()[0]
            py = shebang[2:].strip() if shebang.startswith("#!") else ""
            if not py:
                self.skipTest("pier interpreter not found")
            proc = subprocess.run(
                [
                    py,
                    "-c",
                    "from icode_pier_agent import ICodeAgent; assert ICodeAgent.name() == 'icode'",
                ],
                check=False,
                capture_output=True,
                text=True,
                env=env,
            )
        self.assertEqual(proc.returncode, 0, proc.stdout + proc.stderr)


if __name__ == "__main__":
    raise SystemExit(unittest.main())

