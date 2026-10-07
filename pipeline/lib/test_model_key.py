#!/usr/bin/env python3
"""The model key reaches iCode (and the canary) only through a private file.

A fake Harbor base class and a fake environment record every upload and exec,
so the tests can check that the key is never in an exec's env or command line,
that the uploaded file is 0600 and owned by the agent user, and that the run
command loads the file and deletes it before anything else starts.
"""

from __future__ import annotations

import asyncio
import json
import os
import shutil
import stat
import subprocess
import sys
import tempfile
import textwrap
import unittest
from pathlib import Path
from types import SimpleNamespace
from unittest import mock

LIB = Path(__file__).resolve().parent
sys.path.insert(0, str(LIB))

SENTINEL = "sk-model-sentinel 'quoted' $HOME;x"

FAKE_HARBOR = {
    "harbor/__init__.py": "",
    "harbor/agents/__init__.py": "",
    "harbor/agents/installed/__init__.py": "",
    "harbor/agents/installed/base.py": '''
        class BaseInstalledAgent:
            """Harbor 0.22's exec helpers: `set -o pipefail; ` + command, per-exec env."""

            def __init__(self, logs_dir, *args, extra_env=None, **kwargs):
                self.logs_dir = logs_dir
                self.mcp_servers = []
                self._extra_env = dict(extra_env) if extra_env else {}

            @property
            def extra_env(self):
                return dict(self._extra_env)

            async def exec_as_root(self, environment, command, env=None, cwd=None, timeout_sec=None):
                return await environment.exec(command=f"set -o pipefail; {command}", user="root", env=env)

            async def exec_as_agent(self, environment, command, env=None, cwd=None, timeout_sec=None):
                return await environment.exec(command=f"set -o pipefail; {command}", user=None, env=env)
    ''',
    "harbor/environments/__init__.py": "",
    "harbor/environments/base.py": "class BaseEnvironment:\n    default_user = None\n",
    "harbor/models/__init__.py": "",
    "harbor/models/agent/__init__.py": "",
    "harbor/models/agent/context.py": "class AgentContext:\n    pass\n",
}


class FakeEnvironment:
    """A trial container rooted at `root`; records what the agent sends it."""

    def __init__(self, root: Path, default_user: str | None = "agent"):
        self.root = root
        self.default_user = default_user
        self.execs: list[tuple[str | None, dict, str]] = []
        self.uploads: list[dict] = []

    def path(self, target: str) -> Path:
        return self.root / target.lstrip("/")

    async def upload_file(self, source, target: str) -> None:
        src = Path(source)
        self.uploads.append({
            "source": src, "target": target,
            "mode": stat.S_IMODE(src.stat().st_mode), "text": src.read_text(encoding="utf-8"),
        })
        dest = self.path(target)
        dest.parent.mkdir(parents=True, exist_ok=True)
        shutil.copy2(src, dest)

    async def exec(self, command: str, user=None, env=None, cwd=None, timeout_sec=None):
        self.execs.append((user, dict(env or {}), command))
        return SimpleNamespace(return_code=0, stdout="", stderr="")


class ModelKeyTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls._tmp = tempfile.TemporaryDirectory()
        site = Path(cls._tmp.name) / "site"
        for rel, text in FAKE_HARBOR.items():
            path = site / rel
            path.parent.mkdir(parents=True, exist_ok=True)
            path.write_text(textwrap.dedent(text), encoding="utf-8")
        mine = ("harbor", "icode_harbor_agent", "canary_harbor_agent")
        cls._saved = {k: v for k, v in sys.modules.items() if k.split(".")[0] in mine}
        for key in cls._saved:
            sys.modules.pop(key)
        sys.path.insert(0, str(site))
        cls._site = site
        with mock.patch.dict(os.environ, clear=False):
            os.environ.pop("MAC_K3D_EGRESS_PROBE_IMAGE", None)
            import canary_harbor_agent
            import icode_harbor_agent
        cls.icode = icode_harbor_agent
        cls.canary = canary_harbor_agent

    @classmethod
    def tearDownClass(cls):
        sys.path.remove(str(cls._site))
        for key in [k for k in sys.modules if k.split(".")[0] in ("harbor", "icode_harbor_agent", "canary_harbor_agent")]:
            sys.modules.pop(key)
        sys.modules.update(cls._saved)
        cls._tmp.cleanup()

    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory()
        self.root = Path(self.tmp.name)
        self.logs = self.root / "logs"

    def tearDown(self):
        self.tmp.cleanup()

    def environment(self, default_user: str | None = "agent") -> FakeEnvironment:
        return FakeEnvironment(self.root / "container", default_user)

    def run_agent(self, agent, environment: FakeEnvironment, key: str | None = SENTINEL) -> None:
        env = {"DEEPSEEK_API_KEY": key} if key is not None else {}
        with mock.patch.dict(os.environ, env, clear=False):
            if key is None:
                os.environ.pop("DEEPSEEK_API_KEY", None)
            asyncio.run(agent.run("fix the bug", environment, None))

    def assert_key_never_in_an_exec(self, environment: FakeEnvironment) -> None:
        for user, env, command in environment.execs:
            self.assertNotIn("DEEPSEEK_API_KEY", env, command[:80])
            self.assertNotIn(SENTINEL, command)
            self.assertNotIn("sk-model-sentinel", command)

    def run_loader(self, environment: FakeEnvironment, then: str) -> subprocess.CompletedProcess[str]:
        """The loader as the container's shell would run it, with the key file at its fake path."""
        target = str(environment.path(self.icode.MODEL_KEY_FILE))
        script = self.icode.MODEL_KEY_LOADER.replace(self.icode.MODEL_KEY_FILE, target) + then
        env = {k: v for k, v in os.environ.items() if k != "DEEPSEEK_API_KEY"}
        return subprocess.run(["bash", "-c", script], capture_output=True, text=True, check=False, env=env)

    def test_icode_gets_the_key_from_a_private_file_only(self):
        agent = self.icode.ICodeAgent(logs_dir=self.logs, extra_env={"ICODE_MODEL": "m"})
        environment = self.environment()
        self.run_agent(agent, environment)
        self.assert_key_never_in_an_exec(environment)

        [upload] = environment.uploads
        self.assertEqual(upload["target"], self.icode.MODEL_KEY_FILE)
        self.assertEqual(upload["mode"], 0o600)
        self.assertFalse(upload["source"].exists(), "the host copy is removed right after the upload")
        target = self.icode.MODEL_KEY_FILE
        self.assertIn(("root", f"set -o pipefail; chown agent {target} && chmod 600 {target}"),
                      [(user, command) for user, _, command in environment.execs])

        user, env, command = environment.execs[-1]
        self.assertIsNone(user)
        self.assertEqual(env["ICODE_MODEL"], "m")
        self.assertTrue(command.startswith("set -o pipefail; " + self.icode.MODEL_KEY_LOADER), command[:120])
        self.assertLess(command.index(f"rm -f {target}"), command.index("icode -p"))

        out = self.root / "seen"
        proc = self.run_loader(environment, f'printf "%s" "$DEEPSEEK_API_KEY" > {out}; '
                                            f'[ ! -e {environment.path(target)} ] && echo gone')
        self.assertEqual(proc.returncode, 0, proc.stderr)
        self.assertEqual(proc.stdout.strip(), "gone")
        self.assertEqual(out.read_text(encoding="utf-8"), SENTINEL)

    def test_a_key_passed_with_ae_is_dropped_from_every_exec(self):
        agent = self.icode.ICodeAgent(
            logs_dir=self.logs, extra_env={"DEEPSEEK_API_KEY": SENTINEL, "ICODE_MODEL": "m"},
        )
        self.assertNotIn("DEEPSEEK_API_KEY", agent.extra_env)
        self.assertNotIn("DEEPSEEK_API_KEY", agent.run_env())
        environment = self.environment()
        self.run_agent(agent, environment)
        self.assert_key_never_in_an_exec(environment)

    def test_a_root_container_still_gets_a_0600_file(self):
        agent = self.icode.ICodeAgent(logs_dir=self.logs)
        environment = self.environment(default_user=None)
        self.run_agent(agent, environment)
        target = self.icode.MODEL_KEY_FILE
        commands = [command for _, _, command in environment.execs]
        self.assertIn(f"set -o pipefail; chmod 600 {target}", commands)
        self.assertFalse(any("chown" in c for c in commands))

    def test_a_missing_key_fails_the_trial_before_iCode_starts(self):
        agent = self.icode.ICodeAgent(logs_dir=self.logs)
        environment = self.environment()
        with self.assertRaisesRegex(RuntimeError, "DEEPSEEK_API_KEY is not in Harbor's environment"):
            self.run_agent(agent, environment, key=None)
        self.assertEqual(environment.uploads, [])
        self.assertFalse(any("icode -p" in command for _, _, command in environment.execs))

    def test_the_loader_stops_when_the_file_is_missing(self):
        proc = self.run_loader(self.environment(), "echo started")
        self.assertEqual(proc.returncode, 3)
        self.assertIn("model key file missing", proc.stderr)
        self.assertNotIn("started", proc.stdout)

    def test_the_canary_loads_the_key_exactly_like_icode(self):
        spec = self.root / "canary_spec.json"
        spec.write_text(json.dumps({"version": "v1", "task": "alpha", "allow_host": None}), encoding="utf-8")
        agent = self.canary.CanaryAgent(logs_dir=self.logs, spec=str(spec), extra_env={"ICODE_MODEL": "m"})
        environment = self.environment()
        self.run_agent(agent, environment)
        self.assert_key_never_in_an_exec(environment)
        [upload] = environment.uploads
        self.assertEqual((upload["target"], upload["mode"]), (self.icode.MODEL_KEY_FILE, 0o600))
        probe = next(command for _, _, command in environment.execs if "canary_probe.sh probe" in command)
        self.assertTrue(probe.startswith("set -o pipefail; " + self.icode.MODEL_KEY_LOADER), probe[:120])
        self.assertTrue((self.logs / "canary_host.json").is_file())


if __name__ == "__main__":
    unittest.main()
