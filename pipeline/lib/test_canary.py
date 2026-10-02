#!/usr/bin/env python3
"""Isolation canary tests (report P0.6): fixtures, spec, summary and the probe script."""

from __future__ import annotations

import contextlib
import io
import json
import os
import shutil
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path

LIB = Path(__file__).resolve().parent
sys.path.insert(0, str(LIB))

from canary_verdict import (  # noqa: E402
    CHECKS,
    FACT_FILES,
    build_spec,
    load_config,
    load_facts,
    main,
    render_spec,
    summarize,
    verdict,
)

TESTDATA = LIB / "testdata" / "canary"
PROBE = LIB / "canary_probe.sh"
CAPTURE = LIB / "icode_capture.sh"

GOLD_LINE = "return the secret gold answer forty two"
GOLD_PATCH = f"""diff --git a/src/pkg/new_mod.py b/src/pkg/new_mod.py
new file mode 100644
--- /dev/null
+++ b/src/pkg/new_mod.py
@@ -0,0 +1,2 @@
+def answer():
+    # {GOLD_LINE}
diff --git a/tests/test_new_mod.py b/tests/test_new_mod.py
new file mode 100644
--- /dev/null
+++ b/tests/test_new_mod.py
@@ -0,0 +1 @@
+assert True
diff --git a/README.md b/README.md
--- a/README.md
+++ b/README.md
@@ -1 +1 @@
-old
+new
"""


def facts_for(name: str) -> dict:
    """pass/ facts with the fixture's canary.json on top (leak fixtures hold only canary.json)."""
    facts = load_facts(TESTDATA / "pass")
    if name != "pass":
        facts.update({k: v for k, v in load_facts(TESTDATA / name).items() if v is not None})
    return facts


def statuses(result: dict) -> dict:
    return {name: check["status"] for name, check in result["checks"].items()}


def write_task(root: Path, toml: str = 'docker_image = "x"\n') -> Path:
    task = root / "alpha"
    (task / "solution").mkdir(parents=True)
    (task / "task.toml").write_text(toml, encoding="utf-8")
    (task / "solution" / "solution.patch").write_text(GOLD_PATCH, encoding="utf-8")
    return task


class VerdictFixtureTests(unittest.TestCase):
    def setUp(self):
        self.config = load_config()

    def test_pass_fixture_passes_with_tool_list_skipped(self):
        result = verdict(facts_for("pass"), self.config)
        got = statuses(result)
        self.assertEqual(result["status"], "pass", result["checks"])
        self.assertEqual(got["tool_list"], "skip")
        self.assertEqual({k for k, v in got.items() if v != "pass"}, {"tool_list"})
        self.assertEqual(set(got), set(CHECKS))

    def test_each_leak_fixture_fails_only_its_check(self):
        for fixture, check in (
            ("network_leak", "network"),
            ("env_leak", "env_names"),
            ("history_leak", "history"),
        ):
            result = verdict(facts_for(fixture), self.config)
            got = statuses(result)
            self.assertEqual(result["status"], "fail", fixture)
            self.assertEqual({k for k, v in got.items() if v == "fail"}, {check}, (fixture, result["checks"]))

    def test_leak_details_name_the_leak_but_no_values(self):
        net = verdict(facts_for("network_leak"), self.config)["checks"]["network"]["detail"]
        self.assertIn("github.com", net)
        env = verdict(facts_for("env_leak"), self.config)["checks"]["env_names"]["detail"]
        self.assertIn("GITCODE_TOKEN", env)
        self.assertIn("HF_TOKEN", env)
        self.assertNotIn("DEEPSEEK_API_KEY", env)
        hist = verdict(facts_for("history_leak"), self.config)["checks"]["history"]["detail"]
        self.assertIn("56 commits reachable from refs but not from HEAD", hist)
        self.assertIn("remotes origin", hist)
        self.assertIn("2 unreachable objects", hist)

    def test_reachable_declared_host_warns_but_task_passes(self):
        facts = facts_for("pass")
        facts["probe"]["network"].append(
            {"kind": "declared", "host": "openrouter.ai", "curl_exit": 0, "http_code": "200", "error": None}
        )
        result = verdict(facts, self.config)
        self.assertEqual(result["status"], "pass")
        self.assertEqual(result["checks"]["declared_hosts"]["status"], "warn")
        self.assertIn("openrouter.ai", result["checks"]["declared_hosts"]["detail"])

    def test_unreachable_model_host_fails(self):
        facts = facts_for("pass")
        for entry in facts["probe"]["network"]:
            if entry["kind"] == "model":
                entry.update({"curl_exit": 7, "http_code": "000", "error": "curl: (7) Failed to connect"})
        result = verdict(facts, self.config)
        self.assertEqual(result["status"], "fail")
        self.assertEqual(result["checks"]["model_api"]["status"], "fail")
        self.assertEqual(result["checks"]["network"]["status"], "pass")

    def test_missing_curl_fails_network(self):
        facts = facts_for("pass")
        facts["probe"]["curl"] = False
        self.assertEqual(verdict(facts, self.config)["checks"]["network"]["status"], "fail")

    def test_missing_probe_fails_every_check(self):
        facts = facts_for("pass")
        facts["probe"] = None
        result = verdict(facts, self.config)
        self.assertEqual(result["status"], "fail")
        self.assertTrue(all(result["checks"][c]["status"] == "fail" for c in CHECKS if c != "tool_list"))

    def test_pythonpath_into_mount_fails(self):
        facts = facts_for("pass")
        facts["probe"]["python_env"]["PYTHONPATH"] = "/opt/icode-host/lib/python3.12/site-packages"
        result = verdict(facts, self.config)
        self.assertEqual(result["checks"]["python_env"]["status"], "fail")
        self.assertIn("PYTHONPATH", result["checks"]["python_env"]["detail"])

    def test_writable_mount_fails(self):
        facts = facts_for("pass")
        facts["probe"]["mount"].update({"options": "rw,relatime", "read_only": False, "touch_exit": 0})
        detail = verdict(facts, self.config)["checks"]["mount"]
        self.assertEqual(detail["status"], "fail")
        self.assertIn("not ro", detail["detail"])
        facts = facts_for("pass")
        facts["root"] = None
        self.assertEqual(verdict(facts, self.config)["checks"]["mount"]["status"], "fail")

    def test_mcp_config_and_research_subagent_fail(self):
        facts = facts_for("pass")
        facts["home_post"]["mcp_json"] = True
        self.assertEqual(verdict(facts, self.config)["checks"]["icode_home"]["status"], "fail")
        facts = facts_for("pass")
        facts["home_pre"]["research_subagent"] = True
        self.assertEqual(verdict(facts, self.config)["checks"]["icode_home"]["status"], "fail")
        facts = facts_for("pass")
        facts["host"]["task_mcp_servers"] = 1
        self.assertEqual(verdict(facts, self.config)["checks"]["icode_home"]["status"], "fail")

    def test_filesystem_hit_and_timeout_fail(self):
        facts = facts_for("pass")
        facts["probe"]["filesystem"]["hits"] = ["/tmp/solution.patch"]
        self.assertEqual(verdict(facts, self.config)["checks"]["filesystem"]["status"], "fail")
        facts = facts_for("pass")
        facts["probe"]["filesystem"]["timed_out"] = True
        self.assertEqual(verdict(facts, self.config)["checks"]["filesystem"]["status"], "fail")
        config = dict(self.config, filesystem_ignore=["/usr/share/*"])
        facts = facts_for("pass")
        facts["probe"]["filesystem"]["hits"] = ["/usr/share/doc/solution.patch"]
        self.assertEqual(verdict(facts, config)["checks"]["filesystem"]["status"], "pass")


class SpecTests(unittest.TestCase):
    def test_spec_lists_hosts_and_gold_file_tails_without_gold_content(self):
        toml = (
            'docker_image = "x"\n[agent]\nallowed_hosts = '
            '["api.deepseek.com", "openrouter.ai", "github.com", "*.example.com", "api.openai.com"]\n'
        )
        with tempfile.TemporaryDirectory() as tmp:
            spec = build_spec(write_task(Path(tmp), toml), "lolbench", load_config())
        kinds = {}
        for entry in spec["hosts"]:
            kinds.setdefault(entry["kind"], []).append(entry["host"])
        self.assertEqual(kinds["model"], ["api.deepseek.com"])
        self.assertEqual(kinds["declared"], ["openrouter.ai", "api.openai.com"])
        self.assertIn("github.com", kinds["source"])
        self.assertIn("gitcode.com", kinds["source"])
        self.assertIn("*/pkg/new_mod.py", spec["paths"])
        self.assertIn("*/tests/private", spec["paths"])
        self.assertNotIn("*/tests/test_new_mod.py", spec["paths"])
        self.assertIn("solution.patch", spec["names"])
        self.assertEqual(spec["task"], "alpha")
        text = render_spec(spec)
        self.assertIn("host declared openrouter.ai\n", text)
        self.assertIn("path */pkg/new_mod.py\n", text)
        self.assertIn("prune /logs/agent\n", text)
        self.assertIn("timeout curl 10\n", text)
        self.assertNotIn(GOLD_LINE, text)
        self.assertNotIn(GOLD_LINE, json.dumps(spec))
        self.assertNotIn("README.md", text)

    def test_spec_cli_writes_json(self):
        with tempfile.TemporaryDirectory() as tmp:
            task = write_task(Path(tmp))
            out = Path(tmp) / "out" / "spec.json"
            self.assertEqual(main(["spec", "--task-dir", str(task), "--benchmark", "deepswe", "--out", str(out)]), 0)
            spec = json.loads(out.read_text(encoding="utf-8"))
            self.assertNotIn("declared", {h["kind"] for h in spec["hosts"]})
            err = io.StringIO()
            with contextlib.redirect_stderr(err):
                self.assertEqual(main(["spec", "--task-dir", str(Path(tmp) / "nope"), "--out", str(out)]), 1)
            self.assertIn("missing task dir", err.getvalue())


class SummaryTests(unittest.TestCase):
    def _trial(self, jobs: Path, tid: str, fixture: str) -> None:
        agent = jobs / tid / f"{tid}_canary_7" / f"{tid}__abc" / "agent"
        agent.mkdir(parents=True)
        for key, name in FACT_FILES.items():
            data = facts_for(fixture)[key]
            if data is not None:
                (agent / name).write_text(json.dumps(data), encoding="utf-8")

    def _main(self, argv: list[str]) -> tuple[int, str]:
        buf = io.StringIO()
        with contextlib.redirect_stdout(buf):
            rc = main(argv)
        return rc, buf.getvalue()

    def test_all_pass_exits_zero(self):
        with tempfile.TemporaryDirectory() as tmp:
            jobs = Path(tmp) / "canary"
            self._trial(jobs, "cpython_5", "pass")
            self._trial(jobs, "flink_7", "pass")
            rc, out = self._main(["summarize", "--jobs-dir", str(jobs), "--out-dir", str(jobs)])
            self.assertEqual(rc, 0, out)
            summary = json.loads((jobs / "summary.json").read_text(encoding="utf-8"))
            self.assertEqual(summary["status"], "pass")
            self.assertEqual(summary["counts"], {"tasks": 2, "pass": 2, "fail": 0})
            self.assertIn("canary pass: tasks=2 pass=2 fail=0", out)
            self.assertIn("| cpython_5 | pass |", (jobs / "report.md").read_text(encoding="utf-8"))

    def test_one_leak_or_missing_trial_exits_two(self):
        with tempfile.TemporaryDirectory() as tmp:
            jobs = Path(tmp) / "canary"
            self._trial(jobs, "cpython_5", "pass")
            self._trial(jobs, "fastapi_1", "network_leak")
            argv = ["summarize", "--jobs-dir", str(jobs), "--out-dir", str(jobs)]
            rc, out = self._main(argv + ["--task", "cpython_5", "--task", "fastapi_1", "--task", "flink_7"])
            self.assertEqual(rc, 2, out)
            summary = json.loads((jobs / "summary.json").read_text(encoding="utf-8"))
            self.assertEqual(summary["counts"], {"tasks": 3, "pass": 1, "fail": 2})
            self.assertEqual(summary["failed_tasks"], ["fastapi_1", "flink_7"])
            self.assertIn("CANARY FAIL task=fastapi_1 check=network: github.com reached (HTTP 200)", out)
            self.assertIn("no canary trial", summary["tasks"]["flink_7"]["checks"]["network"]["detail"])
            self.assertIn("## Failures and warnings", (jobs / "report.md").read_text(encoding="utf-8"))

    def test_empty_jobs_dir_fails(self):
        with tempfile.TemporaryDirectory() as tmp:
            self.assertEqual(summarize(Path(tmp) / "none", load_config())["status"], "fail")


class ProbeScriptTests(unittest.TestCase):
    """canary_probe.sh on the host: stub curl, stub read-only touch, temp repo and search root."""

    MODEL = "api.deepseek.com"

    def setUp(self):
        self.tmp = Path(tempfile.mkdtemp())
        self.addCleanup(shutil.rmtree, self.tmp, True)
        self.root = self.tmp / "root"
        self.repo = self.root / "app"
        self.mount = self.root / "opt" / "icode-host"
        self.logs = self.tmp / "logs"
        self.home = self.tmp / "home"
        for path in (self.repo, self.mount, self.logs, self.home):
            path.mkdir(parents=True)
        (self.repo / "src" / "pkg").mkdir(parents=True)
        (self.repo / "src" / "pkg" / "new_mod.py").write_text("in the repo, so pruned\n", encoding="utf-8")
        git_env = {**os.environ, "GIT_AUTHOR_NAME": "t", "GIT_AUTHOR_EMAIL": "t@t", "GIT_COMMITTER_NAME": "t",
                   "GIT_COMMITTER_EMAIL": "t@t"}
        for args in (["init", "-q"], ["add", "-A"], ["commit", "-q", "-m", "base"]):
            subprocess.run(["git", "-C", str(self.repo), *args], check=True, env=git_env, capture_output=True)
        self.spec = self.tmp / "canary_spec.txt"
        self.spec.write_text(
            render_spec(build_spec(write_task(self.tmp / "task"), "deepswe", load_config())), encoding="utf-8"
        )
        self.mounts_file = self.tmp / "mounts"
        self.mounts_file.write_text(f"/dev/vda1 {self.mount} ext4 ro,relatime 0 0\n", encoding="utf-8")
        self.bin = self.tmp / "bin"
        self.bin.mkdir()
        self._stub(
            "curl",
            f"""#!/bin/sh
for a in "$@"; do url=$a; done
case "$url" in
  https://{self.MODEL}/) printf 401; exit 0 ;;
esac
echo "curl: (7) Failed to connect to ${{url}}" >&2
printf 000
exit 7
""",
        )
        self._stub(
            "touch",
            f"""#!/bin/sh
case "$1" in
  {self.mount}/*) echo "touch: cannot touch '$1': Read-only file system" >&2; exit 1 ;;
esac
exec {shutil.which("touch")} "$@"
""",
        )

    def _stub(self, name: str, body: str) -> None:
        (self.bin / name).write_text(body, encoding="utf-8")
        (self.bin / name).chmod(0o755)

    def _probe(self, *args: str) -> None:
        env = {
            "PATH": f"{self.bin}{os.pathsep}{os.environ.get('PATH', '')}",
            "HOME": str(self.home),
            "CANARY_LOG_DIR": str(self.logs),
            "CANARY_SPEC": str(self.spec),
            "CANARY_SEARCH_ROOT": str(self.root),
            "CANARY_MOUNT": str(self.mount),
            "CANARY_MOUNTS_FILE": str(self.mounts_file),
            "CANARY_CAPTURE": str(CAPTURE),
            "CAPTURE_LOG_DIR": str(self.tmp / "capture"),
            "MAC_K3D_REPO": str(self.repo),
            "DEEPSEEK_API_KEY": "sk-canary-sentinel-0000",
        }
        proc = subprocess.run(["bash", str(PROBE), *args], env=env, capture_output=True, text=True, check=False)
        self.assertEqual(proc.returncode, 0, proc.stdout + proc.stderr)

    def _run_all(self) -> dict:
        self._probe("home", "pre")
        self._probe("probe")
        self._probe("home", "post")
        self._probe("mount")
        shutil.copy(TESTDATA / "pass" / "canary_host.json", self.logs / "canary_host.json")
        for name in FACT_FILES.values():
            self.assertIsInstance(json.loads((self.logs / name).read_text(encoding="utf-8")), dict, name)
        return verdict(load_facts(self.logs), load_config())

    def test_probe_facts_parse_and_pass(self):
        result = self._run_all()
        self.assertEqual(result["status"], "pass", result["checks"])
        probe = json.loads((self.logs / "canary.json").read_text(encoding="utf-8"))
        self.assertEqual(probe["repo"]["path"], str(self.repo))
        self.assertEqual(probe["repo"]["all_count"], 1)
        self.assertEqual(probe["filesystem"]["hits"], [])
        self.assertIn(str(self.repo), probe["filesystem"]["pruned"])
        self.assertTrue(probe["mount"]["read_only"])
        self.assertIn("DEEPSEEK_API_KEY", probe["env_names"])
        self.assertNotIn("sk-canary-sentinel-0000", (self.logs / "canary.json").read_text(encoding="utf-8"))
        self.assertTrue(json.loads((self.logs / "canary_root.json").read_text(encoding="utf-8"))["read_only_error"])

    def test_planted_gold_patch_fails_filesystem(self):
        (self.root / "tmp").mkdir()
        (self.root / "tmp" / "solution.patch").write_text("planted\n", encoding="utf-8")
        result = self._run_all()
        self.assertEqual(result["status"], "fail")
        self.assertEqual(result["checks"]["filesystem"]["status"], "fail")
        self.assertIn(str(self.root / "tmp" / "solution.patch"), result["checks"]["filesystem"]["detail"])

    def test_writable_mount_and_extra_ref_fail(self):
        self.mounts_file.write_text(f"/dev/vda1 {self.mount} ext4 rw,relatime 0 0\n", encoding="utf-8")
        (self.bin / "touch").unlink()
        subprocess.run(["git", "-C", str(self.repo), "branch", "-q", "other"], check=True)
        subprocess.run(
            ["git", "-C", str(self.repo), "remote", "add", "origin", "https://gitcode.com/x/y.git"], check=True
        )
        result = self._run_all()
        self.assertEqual(result["checks"]["mount"]["status"], "fail")
        self.assertEqual(result["checks"]["history"]["status"], "fail")
        self.assertIn("remotes origin", result["checks"]["history"]["detail"])
        self.assertEqual(sorted(p.name for p in self.mount.iterdir()), [])


if __name__ == "__main__":
    raise SystemExit(unittest.main())
