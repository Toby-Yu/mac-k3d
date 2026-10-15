#!/usr/bin/env python3
"""scripts/env_set_up/_common.sh: the helpers the bootstrap checks share.

A new Mac runs these before `mac-k3d setup` installs Homebrew bash, so on
macOS they also run under /bin/bash (3.2)."""

from __future__ import annotations

import os
import re
import shutil
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
COMMON = ROOT / "scripts" / "env_set_up" / "_common.sh"
SCRIPTS = sorted((ROOT / "scripts" / "env_set_up").glob("*.sh"))

FAKE_CURL = """#!/bin/sh
printf '%s\\n' "$*" >>"$FAKE_CURL_ARGS"
cat >>"$FAKE_CURL_STDIN"
[ "$FAKE_CURL_FAIL" = 1 ] && exit 7
printf '%s' "$FAKE_CURL_CODE"
"""


def shells() -> list[str]:
    found = ["bash"]
    if sys.platform == "darwin" and Path("/bin/bash").is_file():
        found.append("/bin/bash")
    return found


class CommonTest(unittest.TestCase):
    def setUp(self):
        self.tmp = Path(tempfile.mkdtemp())
        self.home = self.tmp / "home"
        (self.home / ".config" / "mac-k3d").mkdir(parents=True)
        self.bin = self.tmp / "bin"
        self.bin.mkdir()
        curl = self.bin / "curl"
        curl.write_text(FAKE_CURL)
        curl.chmod(0o755)

    def tearDown(self):
        shutil.rmtree(self.tmp, ignore_errors=True)

    def run_common(self, shell: str, script: str, **env: str) -> subprocess.CompletedProcess:
        base = {k: v for k, v in os.environ.items()
                if not k.startswith(("JENKINS_", "MAC_K3D_", "CONTROLLER_CONFIG", "WORKER_CONFIG"))}
        base.update({
            "HOME": str(self.home),
            "PATH": f"{self.bin}{os.pathsep}{os.environ.get('PATH', '')}",
            "FAKE_CURL_ARGS": str(self.tmp / "curl-args"),
            "FAKE_CURL_STDIN": str(self.tmp / "curl-stdin"),
        })
        base.update(env)
        return subprocess.run([shell, "-c", f'source "$1"; {script}', "_", str(COMMON)],
                              env=base, capture_output=True, text=True, check=False)

    def out(self, shell: str, script: str, **env: str) -> str:
        proc = self.run_common(shell, script, **env)
        self.assertEqual(proc.returncode, 0, proc.stderr)
        return proc.stdout.strip()

    def test_every_script_parses(self):
        for shell in shells():
            for script in SCRIPTS:
                proc = subprocess.run([shell, "-n", str(script)], capture_output=True, text=True, check=False)
                self.assertEqual(proc.returncode, 0, f"{shell} -n {script.name}: {proc.stderr}")

    def test_the_release_tag_is_this_checkouts_version(self):
        version = re.search(r'^version = "(.+)"$', (ROOT / "Cargo.toml").read_text(), re.M).group(1)
        for shell in shells():
            self.assertEqual(self.out(shell, 'echo "$MAC_K3D_RELEASE_TAG"'), f"v{version}")
            self.assertEqual(self.out(shell, 'echo "$MAC_K3D_RELEASE_TAG"', MAC_K3D_RELEASE_TAG="v9.9.9"), "v9.9.9")

    def test_a_worker_only_machine_checks_its_agents_controller(self):
        worker = self.home / ".config" / "mac-k3d" / "worker.yaml"
        worker.write_text(
            "jenkins_agent:\n"
            "  name: mac-mini-1\n"
            "  controller_url: \"http://10.0.0.5:17070\"  # the Linux controller\n"
            "docker:\n"
            "  controller_url: wrong\n"
        )
        for shell in shells():
            self.assertEqual(self.out(shell, 'echo "$JENKINS_URL"'), "http://10.0.0.5:17070")
            self.assertEqual(self.out(shell, 'echo "$JENKINS_URL"', JENKINS_URL="http://other:8080"), "http://other:8080")
        (self.home / ".config" / "mac-k3d" / "config.yaml").write_text("role: controller\n")
        for shell in shells():
            self.assertEqual(self.out(shell, 'echo "$JENKINS_URL"'), "http://localhost:17070")

    def test_version_at_least(self):
        cases = [("3.13.1", "3.11", True), ("3.11", "3.11", True), ("3.9.6", "3.11", False), ("4.0", "3.11", True),
                 ("5.2.37", "4.4", True), ("3.2.57", "4.4", False), ("", "3.11", False)]
        for shell in shells():
            for have, want, ok in cases:
                proc = self.run_common(shell, f'version_at_least "{have}" "{want}"')
                self.assertEqual(proc.returncode == 0, ok, f"{shell}: {have} vs {want}")

    def test_job_state_tells_present_absent_and_hidden_apart(self):
        for shell in shells():
            for code, fail, rc in (("200", "0", 0), ("404", "0", 1), ("403", "0", 2), ("401", "0", 2), ("", "1", 1)):
                proc = self.run_common(shell, "job_state deepswe-eval", FAKE_CURL_CODE=code, FAKE_CURL_FAIL=fail,
                                       JENKINS_URL="http://ctl:17070/")
                self.assertEqual(proc.returncode, rc, f"{shell}: HTTP {code or 'none'}")
        args = (self.tmp / "curl-args").read_text()
        self.assertIn("http://ctl:17070/job/deepswe-eval/api/json?tree=name", args)
        self.assertNotIn("-K", args)

    def test_the_api_token_reaches_curl_on_stdin_only(self):
        token = "11aa22bb33cc44dd55ee66ff77"
        env = {"JENKINS_USER": "admin", "JENKINS_API_TOKEN": token, "FAKE_CURL_CODE": "200", "FAKE_CURL_FAIL": "0"}
        for shell in shells():
            self.assertEqual(self.run_common(shell, "job_state deepswe-eval", **env).returncode, 0)
            self.assertEqual(self.run_common(shell, "jenkins_api /pluginManager/api/json", **env).returncode, 0)
        self.assertNotIn(token, (self.tmp / "curl-args").read_text())
        self.assertIn(f'user = "admin:{token}"', (self.tmp / "curl-stdin").read_text())

    def test_jenkins_login_code_is_three_digits(self):
        for shell in shells():
            self.assertEqual(self.out(shell, "jenkins_login_code", FAKE_CURL_CODE="200", FAKE_CURL_FAIL="0"), "200")
            self.assertEqual(self.out(shell, "jenkins_login_code", FAKE_CURL_CODE="", FAKE_CURL_FAIL="1"), "000")


if __name__ == "__main__":
    unittest.main()
