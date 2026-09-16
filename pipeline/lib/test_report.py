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

ROOT = Path(__file__).resolve().parent.parent.parent
LIB = Path(__file__).resolve().parent
sys.path.insert(0, str(LIB))

from check_report import validate  # noqa: E402
from pier_result import hollow_job_reason  # noqa: E402
from score_results import harbor_reward_resolved, pass_at_1, verifier_rates  # noqa: E402


FIXTURE = LIB / "testdata" / "report-min.json"


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
        self.assertTrue(any("missing model" in e for e in errors))
        self.assertTrue(any("missing macro" in e for e in errors))

    def test_lolbench_suite_ok(self):
        doc = json.loads(FIXTURE.read_text(encoding="utf-8"))
        doc["suite"] = "lolbench"
        self.assertEqual(validate(doc), [])

    def test_unknown_suite_rejected(self):
        doc = json.loads(FIXTURE.read_text(encoding="utf-8"))
        doc["suite"] = "humaneval"
        errors = validate(doc)
        self.assertTrue(any("suite should be" in e for e in errors))

    def test_f2p_list_rejected(self):
        doc = json.loads(FIXTURE.read_text(encoding="utf-8"))
        doc["tasks"][0]["f2p"] = ["test_example.py::test_fix"]
        errors = validate(doc)
        self.assertTrue(any("tasks[0].f2p" in e for e in errors))

    def test_cli_fixture(self):
        proc = subprocess.run(
            [sys.executable, str(LIB / "check_report.py"), str(FIXTURE)],
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
                    str(LIB / "score_results.py"),
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
                env={**os.environ, "DEEPSEEK_MODEL": "deepseek-v4-pro", "BENCHMARK": "deepswe"},
            )
            self.assertEqual(proc.returncode, 0, proc.stdout + proc.stderr)
            doc = json.loads(out.read_text(encoding="utf-8"))
            self.assertEqual(validate(doc), [])
            self.assertEqual(doc["suite"], "deepswe")
            self.assertEqual(doc["pass_at_1"], 1.0)
            self.assertEqual(doc["model"], "deepseek-v4-pro")
            self.assertEqual(doc["model_served"], "deepseek-v4-flash")
            self.assertEqual(doc["tokens"]["in"], 3)
            self.assertEqual(doc["tokens"]["out"], 1)
            self.assertEqual(doc["tokens"]["total"], 4)
            self.assertEqual(doc["wall_seconds"], 3.5)
            self.assertEqual(doc["wall_minutes"], 0.058)
            self.assertIs(doc["tasks"][0]["first"], True)
            self.assertEqual(doc["tasks"][0]["reward"], 1.0)
            self.assertIsNone(doc["tasks"][0]["f2p"])
            self.assertNotIn("f2p_rate", doc["tasks"][0])
            self.assertNotIn("llm_name", doc)
            self.assertNotIn("notes", doc)
            self.assertEqual(doc["tasks"][0]["baseline"]["tok_in"], 5)
            self.assertEqual(doc["tasks"][0]["dur_min"], 0.033)

    def test_pier_scores_newest_trial_not_older_fail(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            tasks = root / "tasks" / "abs-module-cache-flags"
            tasks.mkdir(parents=True)
            harness = root / "harness"
            old = harness / "2026-09-16__09-54-24" / "abs-module-cache-flags__old"
            new = harness / "2026-09-16__11-22-34" / "abs-module-cache-flags__new"
            (old / "verifier").mkdir(parents=True)
            (new / "verifier").mkdir(parents=True)
            (old / "verifier" / "reward.json").write_text(
                '{"reward": 0, "f2p": 0.0, "f2p_passed": 0, "f2p_total": 20, "p2p": 1.0, "p2p_passed": 3, "p2p_total": 3}\n',
                encoding="utf-8",
            )
            (new / "verifier" / "reward.json").write_text(
                '{"reward": 1, "f2p": 1.0, "f2p_passed": 20, "f2p_total": 20, "p2p": 1.0, "p2p_passed": 3, "p2p_total": 3}\n',
                encoding="utf-8",
            )
            (old / "result.json").write_text("{}", encoding="utf-8")
            (new / "result.json").write_text("{}", encoding="utf-8")
            import time

            time.sleep(0.05)
            (new / "verifier" / "reward.json").write_text(
                '{"reward": 1, "f2p": 1.0, "f2p_passed": 20, "f2p_total": 20, "p2p": 1.0, "p2p_passed": 3, "p2p_total": 3}\n',
                encoding="utf-8",
            )
            (harness / "meta.json").write_text(
                json.dumps(
                    {
                        "duration_seconds": 958,
                        "llm_model_id": "deepseek-v4-pro",
                        "token_usage": {"prompt": 0, "completion": 0, "total": 0},
                    }
                ),
                encoding="utf-8",
            )
            (new / "artifacts").mkdir()
            (new / "artifacts" / "icode-usage.json").write_text(
                '{"input_tokens": 111, "output_tokens": 22, "total_tokens": 133}\n',
                encoding="utf-8",
            )
            baseline = root / "baseline"
            baseline.mkdir()
            (baseline / "meta.json").write_text(
                json.dumps({"duration_seconds": 10, "token_usage": {"prompt": 1, "completion": 1, "total": 2}}),
                encoding="utf-8",
            )
            out = root / "out.json"
            proc = subprocess.run(
                [
                    sys.executable,
                    str(LIB / "score_results.py"),
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
                env={**os.environ, "BENCHMARK": "deepswe", "DEEPSEEK_MODEL": "deepseek-v4-pro"},
            )
            self.assertEqual(proc.returncode, 0, proc.stdout + proc.stderr)
            doc = json.loads(out.read_text(encoding="utf-8"))
            self.assertEqual(validate(doc), [])
            self.assertEqual(doc["pass_at_1"], 1.0)
            self.assertEqual(doc["macro"]["f2p"], 1.0)
            self.assertEqual(doc["macro"]["reward"], 1.0)
            self.assertEqual(doc["tasks"][0]["f2p_pass"], 20)
            self.assertEqual(doc["tokens"]["in"], 111)
            self.assertEqual(doc["tokens"]["out"], 22)

    def test_icode_usage_parses_harbor_json_line(self):
        from icode_usage import parse_icode_usage_text

        blob = (
            "warning: skip\n"
            '{"session_id": "cli-x", "usage": {"input_tokens": 50, "output_tokens": 7, "total_tokens": 57}}\n'
        )
        got = parse_icode_usage_text(blob)
        self.assertEqual(got["prompt"], 50)
        self.assertEqual(got["completion"], 7)
        self.assertEqual(got["total"], 57)

    def test_harbor_reward_json_sets_resolved(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            tasks = root / "tasks" / "ruff_1"
            tasks.mkdir(parents=True)
            harness = root / "harness"
            (harness / "harbor_runs" / "jenkins-1" / "ruff_1").mkdir(parents=True)
            (harness / "harbor_runs" / "jenkins-1" / "ruff_1" / "reward.json").write_text(
                '{"reward": 1.0}\n', encoding="utf-8"
            )
            (harness / "meta.json").write_text(
                json.dumps({"duration_seconds": 9, "llm_model_id": "deepseek-v4-pro"}),
                encoding="utf-8",
            )
            baseline = root / "baseline"
            baseline.mkdir()
            (baseline / "meta.json").write_text("{}", encoding="utf-8")
            out = root / "out.json"
            proc = subprocess.run(
                [
                    sys.executable,
                    str(LIB / "score_results.py"),
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
                env={**os.environ, "BENCHMARK": "lolbench", "DEEPSEEK_MODEL": "deepseek-v4-pro"},
            )
            self.assertEqual(proc.returncode, 0, proc.stdout + proc.stderr)
            doc = json.loads(out.read_text(encoding="utf-8"))
            self.assertEqual(validate(doc), [])
            self.assertEqual(doc["suite"], "lolbench")
            self.assertEqual(doc["tasks"][0]["reward"], 1.0)
            self.assertIs(doc["tasks"][0]["first"], True)
            self.assertEqual(doc["pass_at_1"], 1.0)
            self.assertIsNone(doc["tasks"][0]["f2p"])
            self.assertNotIn("notes", doc)

            zero = root / "zero"
            zero.mkdir()
            (zero / "reward.json").write_text('{"reward": 0.0}\n', encoding="utf-8")
            self.assertIs(harbor_reward_resolved(zero), False)
            self.assertIs(harbor_reward_resolved(harness), True)

    def test_harbor_nested_job_dir_rates_not_resolved(self):
        """Layout from lolbench_one_task SUCCESS: reward 0.0, F2P ~0.368, P2P 1.0."""
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            tasks = root / "tasks" / "ruff_1"
            tasks.mkdir(parents=True)
            job = (
                root
                / "harness"
                / "harbor_runs"
                / "jenkins-10"
                / "ruff_1"
                / "ruff_1_icode_union_10"
            )
            verifier = job / "ruff_1__g3wWhDM" / "verifier"
            verifier.mkdir(parents=True)
            (verifier / "reward.json").write_text('{"reward": 0.0}\n', encoding="utf-8")
            (job / "result.json").write_text(
                json.dumps(
                    {
                        "metrics": [
                            {
                                "f2p_pass_rate": 0.3684210526315789,
                                "p2p_pass_rate": 1.0,
                                "reward": 0.0,
                                "resolved": 0.0,
                            }
                        ]
                    }
                ),
                encoding="utf-8",
            )
            harness = root / "harness"
            (harness / "meta.json").write_text(
                json.dumps({"duration_seconds": 1576, "llm_model_id": "deepseek-v4-pro"}),
                encoding="utf-8",
            )
            baseline = root / "baseline"
            baseline.mkdir()
            (baseline / "meta.json").write_text("{}", encoding="utf-8")
            out = root / "out.json"
            proc = subprocess.run(
                [
                    sys.executable,
                    str(LIB / "score_results.py"),
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
                env={**os.environ, "BENCHMARK": "lolbench", "DEEPSEEK_MODEL": "deepseek-v4-pro"},
            )
            self.assertEqual(proc.returncode, 0, proc.stdout + proc.stderr)
            doc = json.loads(out.read_text(encoding="utf-8"))
            self.assertEqual(validate(doc), [])
            self.assertEqual(doc["suite"], "lolbench")
            self.assertEqual(doc["tasks"][0]["reward"], 0.0)
            self.assertEqual(doc["pass_at_1"], 0.0)
            self.assertAlmostEqual(doc["tasks"][0]["f2p"], 0.3684210526315789)
            self.assertEqual(doc["tasks"][0]["p2p"], 1.0)
            self.assertAlmostEqual(doc["macro"]["f2p"], 0.3684)
            self.assertEqual(doc["wall_seconds"], 1576)
            self.assertEqual(doc["wall_minutes"], round(1576 / 60.0, 3))
            self.assertIsNone(doc["tasks"][0]["tok_in"])
            self.assertNotIn("f2p_rate", doc["tasks"][0])
            self.assertNotIn("notes", doc)

    def test_verifier_rates_nested_mean_and_list_metrics(self):
        with tempfile.TemporaryDirectory() as tmp:
            nested = Path(tmp) / "nested"
            nested.mkdir()
            (nested / "result.json").write_text(
                json.dumps(
                    {
                        "stats": {
                            "evals": {
                                "icode": {
                                    "metrics": {
                                        "fail_to_pass": {"mean": 0.3684210526315789},
                                        "pass_to_pass": {"mean": 1.0},
                                    }
                                }
                            }
                        }
                    }
                ),
                encoding="utf-8",
            )
            got = verifier_rates(nested)
            self.assertAlmostEqual(got["f2p"], 0.3684210526315789)
            self.assertEqual(got["p2p"], 1.0)


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
        common = ROOT / "pipeline" / "stages" / "_common.sh"
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
# isolate workdir so tests do not touch the real eval-runs tree
export MAC_K3D_EVAL_WORKDIR={tmp}/eval-runs
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
export MAC_K3D_EVAL_WORKDIR={tmp}/eval-runs2
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
        text = (ROOT / "pipeline" / "stages" / "p5_harness.sh").read_text(encoding="utf-8")
        self.assertIn("icode_pier_agent:ICodeAgent", text)
        self.assertIn("No such option", text)
        self.assertIn("selected_tasks", text)
        self.assertIn("hollow", text)
        self.assertNotIn("--agent icode", text)
        self.assertIn('CMD+=(--ae "ICODE_API_BASE=https://api.deepseek.com")', text)
        self.assertIn('CMD+=(--ae "ICODE_PROVIDER=DeepSeek")', text)
        run_sh = (ROOT / "pipeline" / "lib" / "pier-agent-icode" / "run.sh").read_text(
            encoding="utf-8"
        )
        self.assertIn("run -t", run_sh)
        self.assertIn("-a code", run_sh)
        self.assertIn("icode-usage.json", run_sh)
        self.assertNotIn('icode" run "$PROMPT"', run_sh)
        adapter = (ROOT / "pipeline" / "lib" / "icode_pier_agent.py").read_text(
            encoding="utf-8"
        )
        self.assertIn("https://api.deepseek.com", adapter)
        self.assertIn("DeepSeek", adapter)

    def test_p5_lolbench_uses_harbor_agent(self):
        text = (ROOT / "pipeline" / "stages" / "p5_harness.sh").read_text(encoding="utf-8")
        self.assertIn("harbor run", text)
        self.assertIn("icode_harbor_agent:ICodeAgent", text)
        self.assertIn("api.deepseek.com", text)
        self.assertIn("ICODE_MODEL", text)
        self.assertIn("reward.json", text)
        self.assertIn("docker build --progress=plain", text)
        self.assertIn("--mounts", text)
        self.assertIn("P5 harbor heartbeat", text)
        self.assertNotIn("CMD+=(--force-build)", text)
        src = (ROOT / "pipeline" / "lib" / "icode_harbor_agent.py").read_text(encoding="utf-8")
        compile(src, "icode_harbor_agent.py", "exec")
        self.assertIn("/opt/icode-host", src)
        self.assertNotIn("gitcode.com", src)

    def test_p0_installs_buildx_for_harbor_sidecar(self):
        p0 = (ROOT / "pipeline" / "stages" / "p0_prereqs.sh").read_text(encoding="utf-8")
        compose = (ROOT / "pipeline" / "stages" / "ensure_compose.sh").read_text(encoding="utf-8")
        self.assertIn("ensure_docker_buildx", p0)
        self.assertIn("docker-buildx", compose)
        self.assertIn("docker buildx", compose)

    def test_p1_requires_agent_import_path(self):
        text = (ROOT / "pipeline" / "stages" / "p1_pier.sh").read_text(encoding="utf-8")
        self.assertIn("--agent-import-path", text)
        self.assertIn("uv tool install harbor", text)
        self.assertIn("uv tool install datacurve-pier", text)
        self.assertIn("lolbench", text)

    def test_run_all_skips_p4_for_lolbench(self):
        text = (ROOT / "pipeline" / "stages" / "run_all.sh").read_text(encoding="utf-8")
        self.assertIn("P4 skipped", text)
        self.assertIn("p3_icode.sh", text)
        self.assertIn("p5_harness.sh", text)

    def test_hollow_pier_result(self):
        hollow = {
            "n_total_trials": 1,
            "stats": {
                "n_completed_trials": 1,
                "n_errored_trials": 1,
                "evals": {
                    "icode__deepseek-v4-pro__tasks": {
                        "n_trials": 0,
                        "n_errors": 1,
                    }
                },
            },
        }
        self.assertIsNotNone(hollow_job_reason(hollow))
        ran = {
            "stats": {
                "n_errored_trials": 1,
                "evals": {"x": {"n_trials": 1, "n_errors": 1}},
            }
        }
        self.assertIsNone(hollow_job_reason(ran))

    def test_icode_adapter_module_parses(self):
        path = LIB / "icode_pier_agent.py"
        src = path.read_text(encoding="utf-8")
        compile(src, str(path), "exec")
        self.assertIn("class ICodeAgent", src)

    def test_icode_adapter_imports_when_pier_installed(self):
        env = {**os.environ, "PYTHONPATH": str(LIB)}
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


class TaskSelectTests(unittest.TestCase):
    def test_write_selected_tasks_honors_task_and_lolbench(self):
        common = ROOT / "pipeline" / "stages" / "_common.sh"
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            swe = root / "deep-swe" / "tasks"
            (swe / "aaa-first").mkdir(parents=True)
            (swe / "zzz-last").mkdir(parents=True)
            lb = root / "lolbench" / "harbor_tasks"
            (lb / "ruff_1").mkdir(parents=True)
            (lb / "fastapi_1").mkdir(parents=True)
            script = f"""
export MAC_K3D_EVAL_WORKDIR={root}
export BENCHMARK=deepswe
export TASK=zzz-last
source {common}
write_selected_tasks
printf 'deepswe:%s\\n' "$(tr '\\n' ',' <"$MAC_K3D_EVAL_WORKDIR/selected_tasks.txt")"
export BENCHMARK=lolbench
export TASK=ruff_1
write_selected_tasks
printf 'lolbench:%s\\n' "$(tr '\\n' ',' <"$MAC_K3D_EVAL_WORKDIR/selected_tasks.txt")"
unset TASK
export N_TASKS=1
write_selected_tasks
printf 'lolbench-first:%s\\n' "$(tr '\\n' ',' <"$MAC_K3D_EVAL_WORKDIR/selected_tasks.txt")"
"""
            proc = subprocess.run(
                ["bash", "-c", script],
                check=False,
                capture_output=True,
                text=True,
            )
            self.assertEqual(proc.returncode, 0, proc.stdout + proc.stderr)
            self.assertIn("deepswe:zzz-last,", proc.stdout)
            self.assertIn("lolbench:ruff_1,", proc.stdout)
            self.assertIn("lolbench-first:fastapi_1,", proc.stdout)

    def test_write_selected_tasks_honors_comma_list(self):
        common = ROOT / "pipeline" / "stages" / "_common.sh"
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            swe = root / "deep-swe" / "tasks"
            (swe / "aaa-first").mkdir(parents=True)
            (swe / "zzz-last").mkdir(parents=True)
            script = f"""
export MAC_K3D_EVAL_WORKDIR={root}
export BENCHMARK=deepswe
unset TASK
export TASKS='zzz-last, aaa-first'
source {common}
write_selected_tasks
printf 'tasks:%s n:%s\\n' "$(tr '\\n' ',' <"$MAC_K3D_EVAL_WORKDIR/selected_tasks.txt")" "$N_TASKS"
unset TASKS
export TASK='zzz-last,aaa-first'
write_selected_tasks
printf 'task-csv:%s n:%s\\n' "$(tr '\\n' ',' <"$MAC_K3D_EVAL_WORKDIR/selected_tasks.txt")" "$N_TASKS"
unset TASK
export N_TASKS=2
write_selected_tasks
printf 'first2:%s\\n' "$(tr '\\n' ',' <"$MAC_K3D_EVAL_WORKDIR/selected_tasks.txt")"
"""
            proc = subprocess.run(
                ["bash", "-c", script],
                check=False,
                capture_output=True,
                text=True,
            )
            self.assertEqual(proc.returncode, 0, proc.stdout + proc.stderr)
            self.assertIn("tasks:zzz-last,aaa-first, n:2", proc.stdout)
            self.assertIn("task-csv:zzz-last,aaa-first, n:2", proc.stdout)
            self.assertIn("first2:aaa-first,zzz-last,", proc.stdout)

    def test_common_canonicalizes_relative_workdir(self):
        common = ROOT / "pipeline" / "stages" / "_common.sh"
        with tempfile.TemporaryDirectory() as tmp:
            script = f"""
cd {tmp}
export MAC_K3D_EVAL_WORKDIR=rel-eval
source {common}
printf '%s' "$WORKDIR"
"""
            proc = subprocess.run(
                ["bash", "-c", script],
                check=False,
                capture_output=True,
                text=True,
            )
            self.assertEqual(proc.returncode, 0, proc.stdout + proc.stderr)
            workdir = proc.stdout.strip().splitlines()[-1]
            self.assertTrue(workdir.startswith("/"), workdir)
            self.assertTrue(workdir.endswith("rel-eval"), workdir)


class P3IcodeBinaryTests(unittest.TestCase):
    def test_official_full_release_shapes(self):
        script = ROOT / "pipeline" / "stages" / "test_p3_icode.sh"
        proc = subprocess.run(
            ["bash", str(script)],
            check=False,
            capture_output=True,
            text=True,
        )
        self.assertEqual(proc.returncode, 0, proc.stdout + proc.stderr)
        self.assertIn("OK test_p3_icode.sh", proc.stdout)


class SecretGuardTests(unittest.TestCase):
    def test_check_no_secrets_script_passes_on_tracked_tree(self):
        script = ROOT / "scripts" / "check_no_secrets.sh"
        proc = subprocess.run(
            ["bash", str(script)],
            cwd=ROOT,
            check=False,
            capture_output=True,
            text=True,
        )
        self.assertEqual(proc.returncode, 0, proc.stdout + proc.stderr)
        self.assertIn("OK no tracked secret files", proc.stdout)

    def test_gitignore_covers_runtime_env_and_eval_workdirs(self):
        gi = (ROOT / ".gitignore").read_text(encoding="utf-8")
        self.assertIn("**/.pier-env", gi)
        self.assertIn("**/.harbor-env", gi)
        self.assertIn("eval-runs-*/", gi)
        self.assertIn(".cursor/debug-*.log", gi)


if __name__ == "__main__":
    raise SystemExit(unittest.main())

