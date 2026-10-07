#!/usr/bin/env python3
"""env/egress: Harbor's egress probe, the fallback image, and how the run uses it.

A stub `docker` and a stub `harbor` package stand in for the real ones. The
stub DockerEnvironment probes exactly like Harbor 0.22 (`docker container run`
of the class attribute), so Harbor's own check runs through
harbor_probe_override.py as it does in a trial.
"""

from __future__ import annotations

import json
import os
import subprocess
import sys
import tempfile
import textwrap
import unittest
from pathlib import Path

LIB = Path(__file__).resolve().parent
ROOT = LIB.parent.parent
STAGES = ROOT / "pipeline" / "stages"
sys.path.insert(0, str(LIB))

from harbor_egress import FALLBACK_PROBE_IMAGE  # noqa: E402

DEFAULT = "alpine:3.23.4@sha256:5b10"
SIDECAR = "harbor-prebuilt:harbor-docker-egress-control-sidecar--abc123"
ALREADY_EXISTS = (
    "failed to register layer: unable to prepare extraction snapshot: "
    'AlreadyExists: target snapshot "sha256:29df493b": already exists'
)

DOCKER_STUB = r'''#!/usr/bin/env python3
import os, sys
args = sys.argv[1:]
with open(os.environ["STUB_LOG"], "a") as log:
    log.write(" ".join(args) + "\n")
broken = set(filter(None, os.environ.get("STUB_BROKEN", "").split(",")))
ERR = os.environ["STUB_ERR"]
if args[:2] == ["image", "inspect"]:
    ref = args[2]
    if ref in broken or ("sidecar" in ref and os.environ.get("STUB_NO_SIDECAR")):
        sys.exit(1)
    sys.exit(0)
if args[:1] == ["pull"]:
    if args[1] in broken:
        sys.stderr.write(ERR + "\n")
        sys.exit(1)
    sys.exit(0)
if args[:2] == ["container", "run"]:
    if args[3] in broken:
        sys.stderr.write("docker: Error response from daemon: " + ERR + "\n")
        sys.exit(125)
    sys.exit(1 if os.environ.get("STUB_KERNEL_FAIL") else 0)
if args[:1] == ["version"]:
    print("linux/amd64")
sys.exit(0)
'''

HARBOR_DOCKER = f'''
import functools, subprocess
from pathlib import Path


class DockerEnvironment:
    _EGRESS_CONTROL_SIDECAR_CONTEXT_PATH = Path(__file__).parent
    _EGRESS_CONTROL_SIDECAR_DOCKER_NAME = "harbor-prebuilt:harbor-docker-egress-control-sidecar"
    _EGRESS_CONTROL_KERNEL_PROBE_IMAGE = "{DEFAULT}"
    _EGRESS_CONTROL_KERNEL_PROBE_SCRIPT = "if [ ! -f /proc/config.gz ]; then exit 0; fi"

    @classmethod
    def _egress_control_sidecar_dockerfile_path(cls):
        return cls._EGRESS_CONTROL_SIDECAR_CONTEXT_PATH / "Dockerfile"

    @staticmethod
    @functools.cache
    def _egress_control_kernel_support():
        try:
            result = subprocess.run(
                ["docker", "container", "run", "--rm", DockerEnvironment._EGRESS_CONTROL_KERNEL_PROBE_IMAGE,
                 "sh", "-c", DockerEnvironment._EGRESS_CONTROL_KERNEL_PROBE_SCRIPT],
                capture_output=True, text=True, timeout=30,
            )
        except Exception:
            return False
        return result.returncode == 0
'''

HARBOR_UTILS = '''
def _compute_image_name(docker_name, hash_key):
    return f"{docker_name}--{hash_key}"


async def default_docker_platform():
    return "linux/amd64"
'''

HARBOR_CACHE = '''
def docker_build_context_hash(*, context, dockerfile_path=None, build_args=None, platform=None):
    return "abc123"
'''


def fake_harbor(root: Path, docker_py: str = HARBOR_DOCKER) -> Path:
    pkg = root / "site"
    files = {
        "harbor/__init__.py": "",
        "harbor/environments/__init__.py": "",
        "harbor/environments/docker/__init__.py": "",
        "harbor/environments/docker/docker.py": docker_py,
        "harbor/environments/docker/utils.py": HARBOR_UTILS,
        "harbor/utils/__init__.py": "",
        "harbor/utils/container_cache.py": HARBOR_CACHE,
    }
    for rel, text in files.items():
        path = pkg / rel
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_text(textwrap.dedent(text), encoding="utf-8")
    return pkg


class HarborEgressCheckTests(unittest.TestCase):
    def setUp(self):
        self._tmp = tempfile.TemporaryDirectory()
        self.root = Path(self._tmp.name)
        bindir = self.root / "bin"
        bindir.mkdir()
        (bindir / "docker").write_text(DOCKER_STUB, encoding="utf-8")
        (bindir / "docker").chmod(0o755)
        self.log = self.root / "docker.log"
        self.out = self.root / "work" / "egress_probe.json"
        self.env = dict(
            os.environ,
            PATH=f"{bindir}{os.pathsep}{os.environ.get('PATH', '')}",
            PYTHONPATH=str(fake_harbor(self.root)),
            MAC_K3D_HARBOR_PYTHON=sys.executable,
            STUB_LOG=str(self.log),
            STUB_ERR=ALREADY_EXISTS,
        )
        self.env.pop("MAC_K3D_EGRESS_PROBE_IMAGE", None)

    def tearDown(self):
        self._tmp.cleanup()

    def check(self, **stub: str) -> subprocess.CompletedProcess[str]:
        env = dict(self.env, **stub)
        return subprocess.run(
            [sys.executable, str(LIB / "harbor_egress.py"), "check", "--out", str(self.out)],
            capture_output=True, text=True, check=False, env=env,
        )

    def probe(self) -> dict:
        return json.loads(self.out.read_text(encoding="utf-8"))

    def override(self) -> str:
        return subprocess.run(
            [sys.executable, str(LIB / "harbor_egress.py"), "override", "--probe", str(self.out)],
            capture_output=True, text=True, check=True,
        ).stdout.strip()

    def test_harbor_default_probe_runs_so_nothing_is_substituted(self):
        proc = self.check()
        self.assertEqual(proc.returncode, 0, proc.stderr)
        self.assertIn(f"OK egress control: Harbor default probe {DEFAULT}", proc.stdout)
        self.assertIn(f"OK egress sidecar {SIDECAR} present", proc.stdout)
        doc = self.probe()
        self.assertEqual((doc["image"], doc["substituted"], doc["sidecar"]), (DEFAULT, False, SIDECAR))
        self.assertEqual(self.override(), "")
        self.assertNotIn(FALLBACK_PROBE_IMAGE, self.log.read_text(encoding="utf-8"))

    def test_build_53_store_damage_switches_to_the_fallback_image(self):
        proc = self.check(STUB_BROKEN=DEFAULT)
        self.assertEqual(proc.returncode, 0, proc.stderr)
        self.assertIn(f"WARNING: Harbor's probe image {DEFAULT} cannot run on this host", proc.stdout)
        self.assertIn(f"using {FALLBACK_PROBE_IMAGE} for the kernel check. Network isolation is unchanged.", proc.stdout)
        self.assertIn("Docker image store is damaged", proc.stdout)
        doc = self.probe()
        self.assertTrue(doc["substituted"])
        self.assertEqual(doc["image"], FALLBACK_PROBE_IMAGE)
        self.assertEqual(doc["harbor_default"], DEFAULT)
        self.assertIn("unable to prepare extraction snapshot", doc["reason"])
        self.assertEqual(self.override(), FALLBACK_PROBE_IMAGE)
        runs = [ln for ln in self.log.read_text(encoding="utf-8").splitlines() if ln.startswith("container run")]
        # our fallback probe, then Harbor's own check through harbor_probe_override.py
        self.assertEqual(sum(FALLBACK_PROBE_IMAGE in ln for ln in runs), 2, runs)

    def test_both_images_failing_names_both_docker_errors(self):
        proc = self.check(STUB_BROKEN=f"{DEFAULT},{FALLBACK_PROBE_IMAGE}")
        self.assertEqual(proc.returncode, 1)
        self.assertIn("Harbor cannot check egress control on this host", proc.stderr)
        self.assertIn(f"Harbor's probe image {DEFAULT}: {ALREADY_EXISTS}", proc.stderr)
        self.assertIn(f"fallback {FALLBACK_PROBE_IMAGE}: {ALREADY_EXISTS}", proc.stderr)
        self.assertIn("hint: this host's Docker image store is damaged", proc.stderr)
        self.assertFalse(self.out.exists())

    def test_a_kernel_answer_never_falls_back(self):
        proc = self.check(STUB_KERNEL_FAIL="1")
        self.assertEqual(proc.returncode, 1)
        self.assertIn("CONFIG_NFT_FIB_INET", proc.stderr)
        self.assertNotIn(FALLBACK_PROBE_IMAGE, self.log.read_text(encoding="utf-8"))
        self.assertFalse(self.out.exists())

    def test_missing_sidecar_after_a_fallback_stops_the_run(self):
        proc = self.check(STUB_BROKEN=DEFAULT, STUB_NO_SIDECAR="1")
        self.assertEqual(proc.returncode, 1)
        self.assertIn(f"egress sidecar {SIDECAR} is not on this host", proc.stderr)

    def test_missing_sidecar_on_a_healthy_host_is_built_by_harbor(self):
        proc = self.check(STUB_NO_SIDECAR="1")
        self.assertEqual(proc.returncode, 0, proc.stderr)
        self.assertIn(f"note: Harbor builds its egress sidecar {SIDECAR} on the first trial", proc.stdout)

    def test_rate_limit_gets_the_docker_login_hint(self):
        proc = self.check(
            STUB_BROKEN=f"{DEFAULT},{FALLBACK_PROBE_IMAGE}",
            STUB_ERR="toomanyrequests: You have reached your pull rate limit",
        )
        self.assertEqual(proc.returncode, 1)
        self.assertIn("docker login", proc.stderr)


class ProbeOverrideTests(unittest.TestCase):
    def run_override(self, docker_py: str, image: str | None) -> subprocess.CompletedProcess[str]:
        with tempfile.TemporaryDirectory() as tmp:
            site = fake_harbor(Path(tmp), docker_py)
            env = dict(os.environ, PYTHONPATH=f"{site}{os.pathsep}{LIB}")
            env.pop("MAC_K3D_EGRESS_PROBE_IMAGE", None)
            if image is not None:
                env["MAC_K3D_EGRESS_PROBE_IMAGE"] = image
            script = (
                "import harbor_probe_override\n"
                "from harbor.environments.docker.docker import DockerEnvironment as D\n"
                "print(getattr(D, '_EGRESS_CONTROL_KERNEL_PROBE_IMAGE', '<missing>'))\n"
            )
            return subprocess.run([sys.executable, "-c", script], capture_output=True, text=True,
                                  check=False, env=env)

    def test_env_set_swaps_the_image_on_import(self):
        proc = self.run_override(HARBOR_DOCKER, FALLBACK_PROBE_IMAGE)
        self.assertEqual(proc.returncode, 0, proc.stderr)
        self.assertEqual(proc.stdout.split(), [FALLBACK_PROBE_IMAGE])

    def test_env_unset_leaves_harbor_alone(self):
        proc = self.run_override(HARBOR_DOCKER, None)
        self.assertEqual(proc.returncode, 0, proc.stderr)
        self.assertEqual(proc.stdout.split()[0], DEFAULT)
        self.assertEqual(proc.stderr, "")

    def test_a_harbor_without_the_attribute_only_warns(self):
        bare = "class DockerEnvironment:\n    pass\n"
        proc = self.run_override(bare, FALLBACK_PROBE_IMAGE)
        self.assertEqual(proc.returncode, 0, proc.stderr)
        self.assertEqual(proc.stdout.split(), ["<missing>"])
        self.assertIn("probe image left unchanged", proc.stderr)

    def test_a_cached_probe_answer_is_cleared(self):
        import functools

        with tempfile.TemporaryDirectory() as tmp:
            site = fake_harbor(Path(tmp))
            sys.path.insert(0, str(site))
            saved = {k: v for k, v in sys.modules.items() if k == "harbor" or k.startswith("harbor.")}
            prev = os.environ.pop("MAC_K3D_EGRESS_PROBE_IMAGE", None)
            try:
                for key in saved:
                    sys.modules.pop(key)
                from harbor.environments.docker.docker import DockerEnvironment as D

                import harbor_probe_override

                # Harbor caches the probe's answer for the process (functools.cache).
                D._egress_control_kernel_support = staticmethod(functools.cache(lambda: False))
                D._egress_control_kernel_support()
                self.assertEqual(D._egress_control_kernel_support.cache_info().currsize, 1)
                self.assertTrue(harbor_probe_override.apply("img:1"))
                self.assertEqual(D._EGRESS_CONTROL_KERNEL_PROBE_IMAGE, "img:1")
                self.assertEqual(D._egress_control_kernel_support.cache_info().currsize, 0)
                self.assertFalse(harbor_probe_override.apply(""))
            finally:
                if prev is not None:
                    os.environ["MAC_K3D_EGRESS_PROBE_IMAGE"] = prev
                sys.path.remove(str(site))
                for key in [k for k in sys.modules if k == "harbor" or k.startswith("harbor.")]:
                    sys.modules.pop(key)
                sys.modules.update(saved)


class EgressRecordTests(unittest.TestCase):
    def test_record_egress_probe_and_report_line(self):
        from provenance import isolation_lines, isolation_view, record_egress_probe

        with tempfile.TemporaryDirectory() as tmp:
            inputs = Path(tmp) / "eval_protocol_inputs.json"
            inputs.write_text(json.dumps({"isolation": {"mode": "git"}}), encoding="utf-8")
            probe = Path(tmp) / "egress_probe.json"
            probe.write_text(json.dumps({
                "harbor_default": DEFAULT, "image": FALLBACK_PROBE_IMAGE, "substituted": True,
                "reason": ALREADY_EXISTS, "sidecar": SIDECAR, "sidecar_present": True,
            }), encoding="utf-8")
            doc = record_egress_probe(inputs, probe)
            stored = json.loads(inputs.read_text(encoding="utf-8"))
            self.assertEqual(stored, doc)
            self.assertEqual(stored["isolation"]["mode"], "git")
            self.assertEqual(stored["isolation"]["egress_probe"]["image"], FALLBACK_PROBE_IMAGE)
            lines = isolation_lines(isolation_view(stored["isolation"]))
            self.assertIn(
                f"Egress probe: substituted {FALLBACK_PROBE_IMAGE} (Harbor's {DEFAULT} could not run: {ALREADY_EXISTS})",
                lines,
            )

            probe.write_text(json.dumps({"harbor_default": DEFAULT, "image": DEFAULT, "substituted": False}),
                             encoding="utf-8")
            stored = record_egress_probe(inputs, probe)
            lines = isolation_lines(isolation_view(stored["isolation"]))
            self.assertIn(f"Egress probe: Harbor default {DEFAULT}", lines)

    def test_report_without_a_probe_record_has_no_egress_line(self):
        from provenance import isolation_lines, isolation_view

        lines = isolation_lines(isolation_view({"mode": "git"}))
        self.assertFalse(any(line.startswith("Egress probe") for line in lines))


class EgressWiringTests(unittest.TestCase):
    def test_env_runs_egress_after_harbor_and_clears_the_last_output(self):
        text = (STAGES / "env.sh").read_text(encoding="utf-8")
        self.assertIn("STEPS=(env/host env/compose env/harbor env/egress env/model_api)", text)
        self.assertIn('rm -f "$WORKDIR/last_output.txt"', text)
        self.assertLess(text.index("last_output.txt"), text.index("run_steps"))
        step = (STAGES / "env" / "egress.sh").read_text(encoding="utf-8")
        self.assertIn('harbor_egress.py" check --out "$WORKDIR/egress_probe.json"', step)

    def test_evaluate_rechecks_after_the_dry_run_exit(self):
        for name in ("canary.sh", "harbor_run.sh"):
            text = (STAGES / "evaluate" / name).read_text(encoding="utf-8")
            with self.subTest(step=name):
                self.assertEqual(text.count("\nensure_harbor_egress\n"), 1)
                dry = text.index("if harbor_dry_run; then")
                self.assertGreater(text.index("\nensure_harbor_egress\n"), dry)
        cmd = (STAGES / "evaluate" / "harbor_cmd.sh").read_text(encoding="utf-8")
        self.assertIn("record-egress-probe", cmd)

    def test_agent_imports_the_override_before_harbor(self):
        src = (LIB / "icode_harbor_agent.py").read_text(encoding="utf-8")
        imports = [ln for ln in src.splitlines() if ln.startswith(("import ", "from ")) and "__future__" not in ln]
        self.assertTrue(imports[0].startswith("import harbor_probe_override"), imports[:3])
        for agent in ("canary_harbor_agent.py", "patch_harbor_agent.py"):
            self.assertIn("from icode_harbor_agent import", (LIB / agent).read_text(encoding="utf-8"))

    def _run_cmd_env(self, image: str, inherited: str | None) -> str:
        with tempfile.TemporaryDirectory() as tmp:
            env = dict(os.environ, MAC_K3D_EVAL_WORKDIR=str(Path(tmp) / "work"))
            env.pop("MAC_K3D_EGRESS_PROBE_IMAGE", None)
            if inherited is not None:
                env["MAC_K3D_EGRESS_PROBE_IMAGE"] = inherited
            proc = subprocess.run(
                ["bash", "-c",
                 'source "$1"; source "$2"; EGRESS_PROBE_IMAGE="$3"; unit_run_dir="$4"; cmd=(env); run_cmd',
                 "_", str(STAGES / "_common.sh"), str(STAGES / "evaluate" / "harbor_cmd.sh"), image, tmp],
                capture_output=True, text=True, check=False, env=env,
            )
            self.assertEqual(proc.returncode, 0, proc.stderr)
            found = [ln for ln in proc.stdout.splitlines() if ln.startswith("MAC_K3D_EGRESS_PROBE_IMAGE=")]
            return found[0].split("=", 1)[1] if found else ""

    def test_run_cmd_exports_the_probe_image_only_when_substituted(self):
        self.assertEqual(self._run_cmd_env(FALLBACK_PROBE_IMAGE, None), FALLBACK_PROBE_IMAGE)
        self.assertEqual(self._run_cmd_env("", None), "")
        self.assertEqual(self._run_cmd_env("", "stale:image"), "", "a caller's leftover must not reach Harbor")

    def test_harbor_crash_before_any_trial_fails_with_harbor_error(self):
        text = (STAGES / "evaluate" / "harbor_run.sh").read_text(encoding="utf-8")
        self.assertIn('die "harbor run exited $rc before any trial finished: $(harbor_last_error "$unit_log")', text)
        self.assertIn("-mindepth 3 -maxdepth 3 -name result.json", text)
        with tempfile.TemporaryDirectory() as tmp:
            log = Path(tmp) / "harbor.log"
            log.write_text(
                "╭──── Traceback ────╮\n"
                "│ raise ValueError( │\n"
                "╰───────────────────╯\n"
                "ValueError: network_mode='allowlist' is not supported by DockerEnvironment\n",
                encoding="utf-8",
            )
            proc = subprocess.run(
                ["bash", "-c", "source <(sed -n '/^harbor_last_error()/,/^}/p' \"$1\"); harbor_last_error \"$2\"",
                 "_", str(STAGES / "evaluate" / "harbor_run.sh"), str(log)],
                capture_output=True, text=True, check=False,
            )
            self.assertEqual(proc.returncode, 0, proc.stderr)
            self.assertEqual(
                proc.stdout.strip(),
                "ValueError: network_mode='allowlist' is not supported by DockerEnvironment",
            )


if __name__ == "__main__":
    unittest.main()
