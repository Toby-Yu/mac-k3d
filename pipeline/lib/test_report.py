#!/usr/bin/env python3
"""Unit tests for check_report and score_results (no network)."""

from __future__ import annotations

import json
import os
import subprocess
import sys
import tempfile
import time
import unittest
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent.parent
LIB = Path(__file__).resolve().parent
sys.path.insert(0, str(LIB))

from check_report import validate  # noqa: E402
from openai_compat import missing_model_message, model_in_ids, parse_model_ids  # noqa: E402
from pier_result import hollow_job_reason  # noqa: E402
from score_results import (  # noqa: E402
    harbor_reward_resolved,
    pass_at_1,
    scale_eval_resolved,
    verifier_rates,
)


FIXTURE = LIB / "testdata" / "report-min.json"


class OpenAICompatTests(unittest.TestCase):
    def test_parse_model_ids(self):
        ids = parse_model_ids(
            {"object": "list", "data": [{"id": "deepseek-flash"}, {"id": "deepseek-v4-pro"}]}
        )
        self.assertEqual(ids, ["deepseek-flash", "deepseek-v4-pro"])
        self.assertEqual(parse_model_ids({}), [])
        self.assertEqual(parse_model_ids({"data": "nope"}), [])

    def test_product_name_is_not_an_id(self):
        ids = ["deepseek-flash", "deepseek-v4-pro"]
        self.assertTrue(model_in_ids("deepseek-flash", ids))
        self.assertFalse(model_in_ids("deepseek-v4.1-flash", ids))
        msg = missing_model_message("deepseek-v4.1-flash", ids)
        self.assertIn("is not returned by GET /models", msg)
        self.assertIn("deepseek-flash", msg)


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

    def test_swebenchpro_suite_ok(self):
        doc = json.loads(FIXTURE.read_text(encoding="utf-8"))
        doc["suite"] = "swebenchpro"
        self.assertEqual(validate(doc), [])

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

    def test_icode_git_copied_into_report(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            tasks = root / "tasks" / "example-task"
            tasks.mkdir(parents=True)
            harness = root / "harness"
            harness.mkdir()
            (harness / "meta.json").write_text(
                json.dumps({"duration_seconds": 2, "token_usage": {"prompt": 1, "completion": 1, "total": 2}}),
                encoding="utf-8",
            )
            baseline = root / "baseline"
            baseline.mkdir()
            (baseline / "meta.json").write_text("{}", encoding="utf-8")
            work = root / "work"
            work.mkdir()
            (work / "icode_git.json").write_text(
                json.dumps(
                    {
                        "url": "https://github.com/org/icode.git",
                        "kind": "commit",
                        "ref": "abcdef1",
                        "sha": "abcdef1234567890abcdef1234567890abcdef12",
                        "subject": "fix harness for PR",
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
                env={
                    **os.environ,
                    "DEEPSEEK_MODEL": "deepseek-v4-pro",
                    "BENCHMARK": "deepswe",
                    "WORKDIR": str(work),
                },
            )
            self.assertEqual(proc.returncode, 0, proc.stdout + proc.stderr)
            doc = json.loads(out.read_text(encoding="utf-8"))
            self.assertEqual(validate(doc), [])
            self.assertEqual(doc["icode_git"]["kind"], "commit")
            self.assertEqual(doc["icode_git"]["ref"], "abcdef1")
            self.assertTrue(doc["icode_git"]["sha"].startswith("abcdef12"))
            self.assertEqual(doc["icode_git"]["subject"], "fix harness for PR")

    def test_harbor_scores_newest_job_not_older_fail(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            tasks = root / "tasks" / "abs-module-cache-flags"
            tasks.mkdir(parents=True)
            harness = root / "harness"
            old = harness / "harbor_runs" / "2026-09-16__09-54-24" / "abs-module-cache-flags"
            new = harness / "harbor_runs" / "2026-09-16__11-22-34" / "abs-module-cache-flags"
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

    def test_two_rollouts_count_resolved_attempts(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            tasks = root / "tasks" / "example-task"
            tasks.mkdir(parents=True)
            job = root / "harness" / "harbor_runs" / "jenkins-2" / "example-task"
            older = job / "attempt-a" / "verifier"
            newer = job / "attempt-b" / "verifier"
            older.mkdir(parents=True)
            newer.mkdir(parents=True)
            (older / "reward.json").write_text(
                '{"reward": 1.0, "f2p": 1.0, "p2p": 1.0, "f2p_pass": 1, "f2p_total": 1, "p2p_pass": 1, "p2p_total": 1}\n',
                encoding="utf-8",
            )
            (newer / "reward.json").write_text(
                '{"reward": 0.0, "f2p": 0.0, "p2p": 1.0, "f2p_pass": 0, "f2p_total": 1, "p2p_pass": 1, "p2p_total": 1}\n',
                encoding="utf-8",
            )
            (job / "attempt-a" / "agent").mkdir()
            (job / "attempt-b" / "agent").mkdir()
            (job / "attempt-a" / "agent" / "icode-usage.json").write_text(
                '{"input_tokens": 10, "output_tokens": 1}\n', encoding="utf-8"
            )
            (job / "attempt-b" / "agent" / "icode-usage.json").write_text(
                '{"input_tokens": 20, "output_tokens": 2}\n', encoding="utf-8"
            )
            now = time.time()
            os.utime(older / "reward.json", (now - 20, now - 20))
            os.utime(newer / "reward.json", (now, now))
            harness = root / "harness"
            (harness / "meta.json").write_text(
                json.dumps({"duration_seconds": 4, "llm_model_id": "deepseek-flash"}),
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
                env={
                    **os.environ,
                    "BENCHMARK": "deepswe",
                    "DEEPSEEK_MODEL": "deepseek-flash",
                    "N_ROLLOUTS": "2",
                },
            )
            self.assertEqual(proc.returncode, 0, proc.stdout + proc.stderr)
            doc = json.loads(out.read_text(encoding="utf-8"))
            self.assertEqual(validate(doc), [])
            task = doc["tasks"][0]
            self.assertEqual(doc["n_rollouts"], 2)
            self.assertEqual(task["c"], 1)
            self.assertEqual(task["n"], 2)
            self.assertEqual(task["pass_frac"], 0.5)
            self.assertIs(task["first"], True)
            self.assertEqual(task["reward"], 0.5)
            self.assertEqual(doc["pass_at_1"], 0.5)
            self.assertEqual(task["f2p"], 0.5)
            self.assertEqual(task["p2p"], 1.0)
            self.assertEqual(task["f2p_pass"], 1)
            self.assertEqual(task["f2p_total"], 2)
            self.assertEqual(task["tok_in"], 30)
            self.assertEqual(task["tok_out"], 3)

    def test_current_run_ignores_leftover_and_fills_partial(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            tid = "instance_nodebb"
            tasks = root / "tasks" / tid
            tasks.mkdir(parents=True)
            harness = root / "harness"
            trial = harness / "harbor_runs" / "jenkins-16" / tid
            (trial / "verifier").mkdir(parents=True)
            (trial / "verifier" / "reward.json").write_text(
                json.dumps(
                    {
                        "reward": 1.0,
                        "resolved": True,
                        "f2p": 1.0,
                        "f2p_pass": 3,
                        "f2p_total": 3,
                        "p2p": 1.0,
                        "p2p_pass": 288,
                        "p2p_total": 288,
                    }
                ),
                encoding="utf-8",
            )
            (trial / "icode-usage.json").write_text(
                '{"input_tokens": 12743597, "output_tokens": 68171, "total_tokens": 12811768}\n',
                encoding="utf-8",
            )
            (harness / "meta.json").write_text(
                json.dumps(
                    {
                        "duration_seconds": 1785,
                        "llm_model_id": "deepseek-flash",
                        "token_usage": {"prompt": 15581690, "completion": 75133, "total": 15656823},
                    }
                ),
                encoding="utf-8",
            )
            baseline = root / "baseline"
            task_base = baseline / tid
            task_base.mkdir(parents=True)
            (task_base / "eval.json").write_text('{"resolved": false}\n', encoding="utf-8")
            (task_base / "response.txt").write_text("the tests fail\n", encoding="utf-8")
            (task_base / "agent.patch").write_text("diff --git a b\n", encoding="utf-8")
            (baseline / "summary.json").write_text(
                json.dumps(
                    [
                        {
                            "id": tid,
                            "ok": True,
                            "patch_path": str(task_base / "agent.patch"),
                            "token_usage": {"prompt": 1411, "completion": 33828, "total": 35239},
                            "duration_seconds": 108.497,
                        }
                    ]
                ),
                encoding="utf-8",
            )
            (baseline / "meta.json").write_text(
                json.dumps({"duration_seconds": 108.497, "token_usage": {"prompt": 1411, "completion": 33828}}),
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
                env={**os.environ, "BENCHMARK": "swebenchpro", "DEEPSEEK_MODEL": "deepseek-flash"},
            )
            self.assertEqual(proc.returncode, 0, proc.stdout + proc.stderr)
            doc = json.loads(out.read_text(encoding="utf-8"))
            self.assertEqual(validate(doc), [])
            self.assertEqual(doc["tasks"][0]["reward"], 1.0)
            self.assertEqual(doc["tasks"][0]["partial"], 1.0)
            self.assertEqual(doc["macro"]["partial"], 1.0)
            self.assertIsNone(doc["tasks"][0]["baseline"]["reward"])
            self.assertEqual(doc["tokens"]["in"], 12743597)
            self.assertEqual(doc["tokens"]["out"], 68171)
            self.assertEqual(doc["tasks"][0]["baseline"]["tok_in"], 1411)

    def test_clear_stale_baseline_grades(self):
        from baseline_deepseek import clear_stale_grades

        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            task = root / "task"
            task.mkdir()
            (task / "eval.json").write_text('{"resolved": false}\n', encoding="utf-8")
            (task / "reward.json").write_text('{"reward": 0}\n', encoding="utf-8")
            (root / "scale_summary.json").write_text("{}\n", encoding="utf-8")
            (root / "scale_eval").mkdir()
            (root / "scale_eval" / "eval_results.json").write_text("{}\n", encoding="utf-8")
            (task / "agent.patch").write_text("diff\n", encoding="utf-8")
            clear_stale_grades(root, task)
            self.assertFalse((task / "eval.json").exists())
            self.assertFalse((task / "reward.json").exists())
            self.assertFalse((root / "scale_summary.json").exists())
            self.assertFalse((root / "scale_eval").exists())
            self.assertTrue((task / "agent.patch").is_file())

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

    def test_lolbench_agent_report_counts_and_trial_tokens(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            tasks = root / "tasks" / "ruff_1"
            tasks.mkdir(parents=True)
            trial = (
                root
                / "harness"
                / "harbor_runs"
                / "jenkins-15"
                / "ruff_1"
                / "ruff_1_icode_union_15"
                / "ruff_1__qbeDcAQ"
            )
            verifier = trial / "verifier"
            verifier.mkdir(parents=True)
            (verifier / "reward.json").write_text(
                json.dumps(
                    {
                        "reward": 0.0,
                        "resolved": 0.0,
                        "f2p_pass_rate": 0.6842105263157895,
                        "p2p_pass_rate": 1.0,
                    }
                ),
                encoding="utf-8",
            )
            (verifier / "agent_report.json").write_text(
                json.dumps(
                    {
                        "f2p": {"passed": 13, "failed": 6, "total": 19},
                        "p2p": {"passed": 51, "failed": 0, "total": 51},
                        "resolved": False,
                    }
                ),
                encoding="utf-8",
            )
            (trial / "agent").mkdir()
            (trial / "agent" / "icode.txt").write_text(
                '{"usage": {"input_tokens": 10092649, "output_tokens": 73292, "total_tokens": 10165941}}\n',
                encoding="utf-8",
            )
            harness = root / "harness"
            (harness / "meta.json").write_text(
                json.dumps({"duration_seconds": 954, "token_usage": {"prompt": 1, "completion": 1}}),
                encoding="utf-8",
            )
            baseline = root / "baseline" / "ruff_1"
            baseline.mkdir(parents=True)
            (root / "baseline" / "summary.json").write_text(
                json.dumps(
                    [
                        {
                            "id": "ruff_1",
                            "ok": True,
                            "token_usage": {"prompt": 663, "completion": 112, "total": 775},
                            "duration_seconds": 1.087,
                        }
                    ]
                ),
                encoding="utf-8",
            )
            (root / "baseline" / "meta.json").write_text(
                json.dumps(
                    {
                        "duration_seconds": 1.087,
                        "token_usage": {"prompt": 663, "completion": 112, "total": 775},
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
                    str(root / "baseline"),
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
                env={**os.environ, "BENCHMARK": "lolbench", "DEEPSEEK_MODEL": "deepseek-flash"},
            )
            self.assertEqual(proc.returncode, 0, proc.stdout + proc.stderr)
            doc = json.loads(out.read_text(encoding="utf-8"))
            self.assertEqual(validate(doc), [])
            task = doc["tasks"][0]
            self.assertEqual(task["f2p_pass"], 13)
            self.assertEqual(task["f2p_total"], 19)
            self.assertEqual(task["p2p_pass"], 51)
            self.assertEqual(task["p2p_total"], 51)
            self.assertEqual(task["partial"], 0.9143)
            self.assertEqual(doc["macro"]["partial"], 0.9143)
            self.assertEqual(task["tok_in"], 10092649)
            self.assertEqual(task["tok_out"], 73292)
            self.assertEqual(doc["tokens"]["in"], 10092649)
            self.assertEqual(doc["tokens"]["out"], 73292)
            self.assertEqual(task["baseline"]["tok_in"], 663)
            self.assertIsNone(task["baseline"]["reward"])

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
        self.assertIn("GITCODE_TOKEN=", text)
        self.assertIn("GITHUB_TOKEN=", text)
        self.assertNotRegex(text, r"DEEPSEEK_API_KEY=sk-")
        self.assertNotRegex(text, r"GITCODE_TOKEN=\S")
        self.assertNotRegex(text, r"GITHUB_TOKEN=\S")

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

            envf.write_text(
                "DEEPSEEK_API_KEY=test-from-file\n"
                "GITCODE_TOKEN=gc-from-file\n"
                "MAC_K3D_GITHUB_PAT=gh-alias\n",
                encoding="utf-8",
            )
            envf.chmod(0o600)
            script_git = f"""
unset DEEPSEEK_API_KEY GITCODE_TOKEN GITHUB_TOKEN MAC_K3D_GITHUB_PAT MAC_K3D_GITCODE_PAT
export MAC_K3D_ENV_FILE={envf}
export MAC_K3D_EVAL_WORKDIR={tmp}/eval-runs3
source {common}
printf 'gc:%s gh:%s' "$GITCODE_TOKEN" "$GITHUB_TOKEN"
"""
            gitp = subprocess.run(
                ["bash", "-c", script_git],
                check=False,
                capture_output=True,
                text=True,
            )
            self.assertEqual(gitp.returncode, 0, gitp.stdout + gitp.stderr)
            self.assertTrue(gitp.stdout.endswith("gc:gc-from-file gh:gh-alias"), gitp.stdout)
            self.assertNotIn("gc-from-file", gitp.stderr)
            self.assertNotIn("gh-alias", gitp.stderr)

    def test_p5_uses_import_path_not_bare_agent_icode(self):
        text = (ROOT / "pipeline" / "stages" / "p5_harness.sh").read_text(encoding="utf-8")
        self.assertIn("icode_harbor_agent:ICodeAgent", text)
        self.assertIn("harbor run", text)
        self.assertIn("CMD+=(-n 1)", text)
        self.assertIn('CMD+=(-k "$N_ROLLOUTS")', text)
        self.assertIn("No such option", text)
        self.assertIn("selected_tasks", text)
        self.assertNotIn("pier run", text)
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

    def test_p0_checks_openai_models_when_key_set(self):
        p0 = (ROOT / "pipeline" / "stages" / "p0_prereqs.sh").read_text(encoding="utf-8")
        self.assertIn("openai_compat.py", p0)
        self.assertIn("--check-model", p0)
        self.assertIn("GET /models", p0)

    def test_p0_installs_buildx_for_harbor_sidecar(self):
        p0 = (ROOT / "pipeline" / "stages" / "p0_prereqs.sh").read_text(encoding="utf-8")
        compose = (ROOT / "pipeline" / "stages" / "ensure_compose.sh").read_text(encoding="utf-8")
        self.assertIn("ensure_docker_buildx", p0)
        self.assertIn("docker-buildx", compose)
        self.assertIn("docker buildx", compose)

    def test_p1_requires_agent_import_path(self):
        text = (ROOT / "pipeline" / "stages" / "p1_pier.sh").read_text(encoding="utf-8")
        self.assertIn("uv tool install harbor", text)
        self.assertIn("lolbench", text)
        self.assertIn("swebenchpro", text)
        self.assertIn("ensuring harbor", text)
        self.assertNotIn("datacurve-pier", text)

    def test_run_all_runs_p4_for_every_benchmark(self):
        text = (ROOT / "pipeline" / "stages" / "run_all.sh").read_text(encoding="utf-8")
        self.assertNotIn("P4 skipped", text)
        self.assertIn("p3_icode.sh", text)
        self.assertIn("p5_harness.sh", text)
        self.assertIn("p4_agent.sh", text)
        self.assertNotIn("Scale Docker eval", text)

    def test_p5_swebenchpro_uses_harbor(self):
        text = (ROOT / "pipeline" / "stages" / "p5_harness.sh").read_text(encoding="utf-8")
        self.assertNotIn("swebenchpro_run.py", text)
        self.assertNotIn("pier run", text)
        self.assertIn("harbor run", text)
        self.assertIn("icode_harbor_agent:ICodeAgent", text)
        p2 = (ROOT / "pipeline" / "stages" / "p2_deepswe.sh").read_text(encoding="utf-8")
        self.assertIn("swebenchpro_tasks.py", p2)
        self.assertIn("swebenchpro", p2)

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
            pro = root / "swebenchpro" / "tasks"
            (pro / "django__forms-1234").mkdir(parents=True)
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
export BENCHMARK=swebenchpro
export TASK=django__forms-1234
write_selected_tasks
printf 'swebenchpro:%s\\n' "$(tr '\\n' ',' <"$MAC_K3D_EVAL_WORKDIR/selected_tasks.txt")"
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
            self.assertIn("swebenchpro:django__forms-1234,", proc.stdout)

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

    def test_icode_input_url_ref_validation(self):
        script = ROOT / "pipeline" / "stages" / "test_icode_input.sh"
        proc = subprocess.run(
            ["bash", str(script)],
            check=False,
            capture_output=True,
            text=True,
        )
        self.assertEqual(proc.returncode, 0, proc.stdout + proc.stderr)
        self.assertIn("OK test_icode_input.sh", proc.stdout)


class SwebenchproTests(unittest.TestCase):
    def test_dockerhub_image_matches_scale_helper(self):
        from swebenchpro_run import dockerhub_image

        meta = {
            "instance_id": "instance_NodeBB__NodeBB-04998908ba6721d64eba79ae3b65a351dcfbc5b5-vnan",
            "repo": "NodeBB/NodeBB",
            "dockerhub_tag": "NodeBB_NodeBB",
            "dockerhub_username": "jefzda",
        }
        self.assertEqual(
            dockerhub_image(meta),
            "jefzda/sweap-images:nodebb.nodebb-NodeBB__NodeBB-04998908ba6721d64eba79ae3b65a351dcfbc5b5",
        )
        hub = {
            **meta,
            "dockerhub_tag": "nodebb.nodebb-NodeBB__NodeBB-04998908ba6721d64eba79ae3b65a351dcfbc5b5",
        }
        self.assertEqual(
            dockerhub_image(hub),
            "jefzda/sweap-images:nodebb.nodebb-NodeBB__NodeBB-04998908ba6721d64eba79ae3b65a351dcfbc5b5",
        )

    def test_row_from_obj_maps_uppercase_fail_to_pass(self):
        from swebenchpro_tasks import row_from_obj

        row = row_from_obj(
            {
                "instance_id": "instance_demo",
                "repo": "NodeBB/NodeBB",
                "FAIL_TO_PASS": ["test/a.js | new"],
                "PASS_TO_PASS": '["test/a.js | old"]',
                "selected_test_files_to_run": ["test/a.js"],
            }
        )
        self.assertIsNotNone(row)
        self.assertEqual(row["fail_to_pass"], '["test/a.js | new"]')
        self.assertEqual(row["pass_to_pass"], '["test/a.js | old"]')
        self.assertEqual(row["selected_test_files_to_run"], '["test/a.js"]')
        self.assertEqual(set(eval(row["fail_to_pass"])), {"test/a.js | new"})
        self.assertEqual(set(eval(row["pass_to_pass"])), {"test/a.js | old"})

    def test_suite_reward_matches_scale_rule(self):
        from swebenchpro_tasks import suite_reward

        got = suite_reward({"new", "old"}, ["new"], ["old"])
        self.assertEqual(got["reward"], 1.0)
        self.assertTrue(got["resolved"])
        self.assertEqual(got["f2p"], 1.0)
        self.assertEqual(got["p2p"], 1.0)
        missed = suite_reward({"old"}, ["new"], ["old"])
        self.assertEqual(missed["reward"], 0.0)
        self.assertFalse(missed["resolved"])
        self.assertEqual(missed["f2p_pass"], 0)
        self.assertEqual(missed["f2p_total"], 1)

    def test_scale_suite_rates_from_passed_names(self):
        from swebenchpro_run import scale_suite_rates

        rates = scale_suite_rates(
            {
                "fail_to_pass": '["new-test"]',
                "pass_to_pass": '["old-test"]',
            },
            {"new-test", "old-test"},
        )
        self.assertEqual(rates["f2p"], 1.0)
        self.assertEqual(rates["f2p_pass"], 1)
        self.assertEqual(rates["f2p_total"], 1)
        self.assertEqual(rates["p2p"], 1.0)
        self.assertEqual(rates["p2p_pass"], 1)
        self.assertEqual(rates["p2p_total"], 1)

    def test_parse_eval_results_bool_map(self):
        from swebenchpro_run import parse_eval_output

        with tempfile.TemporaryDirectory() as tmp:
            out = Path(tmp)
            (out / "eval_results.json").write_text(
                json.dumps({"instance_demo": True, "instance_other": False}),
                encoding="utf-8",
            )
            got = parse_eval_output(out)
            self.assertEqual(got["instance_demo"], True)
            self.assertEqual(got["instance_other"], False)

    def test_write_patches_json_is_scale_list(self):
        from swebenchpro_run import write_patches_json

        with tempfile.TemporaryDirectory() as tmp:
            path = Path(tmp) / "patches.json"
            write_patches_json(path, {"inst-1": "diff --git a b\n"})
            data = json.loads(path.read_text(encoding="utf-8"))
            self.assertIsInstance(data, list)
            self.assertEqual(data[0]["instance_id"], "inst-1")
            self.assertIn("diff --git", data[0]["patch"])

    def test_materialize_jsonl_and_skip_docker(self):
        from swebenchpro_tasks import materialize

        fixture = LIB / "testdata" / "swebenchpro-mini.jsonl"
        with tempfile.TemporaryDirectory() as tmp:
            src = Path(tmp) / "src"
            src.mkdir()
            (src / "helper_code").mkdir()
            (src / "helper_code" / "sweap_eval.jsonl").write_text(
                fixture.read_text(encoding="utf-8"), encoding="utf-8"
            )
            out = Path(tmp) / "tasks"
            ids = materialize(src, out, n_tasks=1)
            self.assertEqual(ids, ["django__forms-1234"])
            self.assertTrue((out / "django__forms-1234" / "instruction.md").is_file())
            meta = json.loads((out / "django__forms-1234" / "meta.json").read_text(encoding="utf-8"))
            self.assertEqual(meta["dockerhub_tag"], "django_django")
            toml = (out / "django__forms-1234" / "task.toml").read_text(encoding="utf-8")
            self.assertIn("docker_image", toml)
            self.assertIn('environment_mode = "shared"', toml)
            self.assertNotIn('environment_mode = "separate"', toml)
            self.assertIn("jefzda/sweap-images:", toml)
            self.assertTrue((out / "django__forms-1234" / "tests" / "test.sh").is_file())
            self.assertTrue((out / "django__forms-1234" / "tests" / "grade.py").is_file())
            dockerfile = (out / "django__forms-1234" / "environment" / "Dockerfile").read_text(
                encoding="utf-8"
            )
            self.assertIn("ENTRYPOINT []", dockerfile)
            compose = (
                out / "django__forms-1234" / "environment" / "docker-compose.yaml"
            ).read_text(encoding="utf-8")
            self.assertIn('entrypoint: ["sh", "-c", "sleep infinity"]', compose)
            verifier_compose = (
                out / "django__forms-1234" / "tests" / "docker-compose.yaml"
            ).read_text(encoding="utf-8")
            self.assertIn('entrypoint: ["sh", "-c", "sleep infinity"]', verifier_compose)
            ids2 = materialize(src, out, task="flask__views-9")
            self.assertEqual(ids2, ["flask__views-9"])

            host = Path(tmp) / "icode-host"
            host.mkdir()
            (host / "icode").write_text("#!/bin/sh\nexit 0\n", encoding="utf-8")
            harness = Path(tmp) / "harness"
            proc = subprocess.run(
                [
                    sys.executable,
                    str(LIB / "swebenchpro_run.py"),
                    "--mode",
                    "harness",
                    "--tasks-dir",
                    str(out),
                    "--out-dir",
                    str(harness),
                    "--src-dir",
                    str(src),
                    "--n-tasks",
                    "1",
                    "--icode-host",
                    str(host),
                    "--skip-docker",
                    "--skip-eval",
                ],
                check=False,
                capture_output=True,
                text=True,
            )
            self.assertEqual(proc.returncode, 0, proc.stdout + proc.stderr)
            eval_doc = json.loads(
                (harness / "django__forms-1234" / "eval.json").read_text(encoding="utf-8")
            )
            self.assertEqual(eval_doc["resolved"], False)
            self.assertIs(scale_eval_resolved(harness / "django__forms-1234"), False)

    def test_unknown_benchmark_dies_in_p2(self):
        text = (ROOT / "pipeline" / "stages" / "p2_deepswe.sh").read_text(encoding="utf-8")
        self.assertIn("use deepswe, lolbench, or swebenchpro", text)
        common = (ROOT / "pipeline" / "stages" / "_common.sh").read_text(encoding="utf-8")
        self.assertIn("swebenchpro) echo \"$SWEBENCHPRO_DIR/tasks\"", common)


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

    def test_check_docs_script_passes_on_tracked_tree(self):
        script = ROOT / "scripts" / "check_docs.sh"
        proc = subprocess.run(
            ["bash", str(script)],
            cwd=ROOT,
            check=False,
            capture_output=True,
            text=True,
        )
        self.assertEqual(proc.returncode, 0, proc.stdout + proc.stderr)
        self.assertIn("OK docs layout", proc.stdout)

    def test_gitignore_covers_runtime_env_and_eval_workdirs(self):
        gi = (ROOT / ".gitignore").read_text(encoding="utf-8")
        self.assertIn("**/.pier-env", gi)
        self.assertIn("**/.harbor-env", gi)
        self.assertIn("eval-runs-*/", gi)
        self.assertIn(".cursor/debug-*.log", gi)


if __name__ == "__main__":
    raise SystemExit(unittest.main())

