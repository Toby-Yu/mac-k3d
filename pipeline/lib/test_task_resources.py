#!/usr/bin/env python3
"""Declared-resource admission tests (PF.3): what task.toml asks for, and what fits."""

from __future__ import annotations

import json
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path

LIB = Path(__file__).resolve().parent
ROOT = LIB.parent.parent
sys.path.insert(0, str(LIB))

from task_resources import (  # noqa: E402
    HOST_RESERVE_GB,
    MIN_DISK_GB,
    declared_for_task,
    declared_for_tasks,
    plan_slots,
)

GB_KB = 1024 * 1024

DEEPSWE_TOML = """\
[metadata]
tags = ["python"]

[environment]
build_context = "env"
cpus = 2
memory_mb = 8192
storage_mb = 20480

[verifier]
timeout_sec = 1800

[verifier.environment]
cpus = 2
memory_mb = 8192
storage_mb = 20480
gpus = 0
"""

# The verifier asks for more than the agent does; both have to fit.
LOPSIDED_TOML = """\
[environment]
cpus = 1
memory_mb = 2048

[verifier.environment]
cpus = 4
memory_mb = 7168
"""


def write_task(tasks_dir: Path, tid: str, body: str) -> None:
    (tasks_dir / tid).mkdir(parents=True, exist_ok=True)
    (tasks_dir / tid / "task.toml").write_text(body, encoding="utf-8")


class DeclaredTests(unittest.TestCase):
    def test_reads_cpus_memory_and_storage(self):
        with tempfile.TemporaryDirectory() as tmp:
            tasks = Path(tmp)
            write_task(tasks, "alpha", DEEPSWE_TOML)
            got = declared_for_task(tasks / "alpha" / "task.toml")
            self.assertEqual(got, {"cpus": 2.0, "memory_mb": 8192, "storage_mb": 20480})

    def test_takes_the_largest_block_not_the_first(self):
        with tempfile.TemporaryDirectory() as tmp:
            tasks = Path(tmp)
            write_task(tasks, "beta", LOPSIDED_TOML)
            got = declared_for_task(tasks / "beta" / "task.toml")
            self.assertEqual(got["cpus"], 4.0)
            self.assertEqual(got["memory_mb"], 7168)

    def test_missing_task_toml_is_not_an_error(self):
        self.assertEqual(declared_for_task(Path("/nonexistent/task.toml")), {})

    def test_peak_spans_every_selected_task(self):
        with tempfile.TemporaryDirectory() as tmp:
            tasks = Path(tmp)
            write_task(tasks, "alpha", DEEPSWE_TOML)
            write_task(tasks, "beta", LOPSIDED_TOML)
            (tasks / "gamma").mkdir()
            got = declared_for_tasks(tasks, ["alpha", "beta", "gamma"])
            self.assertEqual(got["peak"]["cpus"], 4.0)
            self.assertEqual(got["peak"]["memory_mb"], 8192)
            self.assertEqual(got["tasks"], 3)
            # gamma declares nothing, so it is counted but not declared.
            self.assertEqual(got["declared_tasks"], 2)


class PlanTests(unittest.TestCase):
    PEAK = {"cpus": 2, "memory_mb": 8192, "storage_mb": 20480}

    def plan(self, **over):
        args = {
            "peak": self.PEAK,
            "cpu_lock_qty": 8,
            "trials": 100,
            "mem_available_kb_": 64 * GB_KB,
            "mem_total_kb_": 64 * GB_KB,
            "free_disk_gb": 500.0,
            "measured_peak_gb": None,
        }
        args.update(over)
        return plan_slots(
            args["peak"],
            args["cpu_lock_qty"],
            args["trials"],
            args["mem_available_kb_"],
            args["mem_total_kb_"],
            args["free_disk_gb"],
            args["measured_peak_gb"],
        )

    def test_slots_are_the_tightest_of_cpu_and_memory(self):
        self.assertEqual(self.plan()["slots"], 4)  # 8 cores / 2 cpus
        self.assertEqual(self.plan(cpu_lock_qty=16)["slots"], 7)  # (64-2) GB / 8 GB
        self.assertTrue(self.plan()["fits"])

    def test_a_locked_core_below_the_declared_cpus_blocks_the_build(self):
        got = self.plan(cpu_lock_qty=1)
        self.assertFalse(got["fits"])
        self.assertIn("declares 2 CPUs", got["blockers"][0])
        self.assertIn("locked only 1", got["blockers"][0])

    def test_a_worker_smaller_than_one_trial_blocks_the_build(self):
        got = self.plan(mem_total_kb_=4 * GB_KB, mem_available_kb_=4 * GB_KB)
        self.assertFalse(got["fits"])
        self.assertIn("8.0 GB", got["blockers"][0])

    def test_busy_ram_lowers_slots_without_blocking(self):
        """Installed RAM decides admission; free RAM only decides throughput."""
        got = self.plan(mem_total_kb_=64 * GB_KB, mem_available_kb_=12 * GB_KB)
        self.assertTrue(got["fits"])
        self.assertEqual(got["slots"], 1)  # (12-2) GB / 8 GB

    def test_a_measured_peak_beats_the_declared_ceiling(self):
        declared = self.plan(cpu_lock_qty=16, mem_available_kb_=20 * GB_KB)
        measured = self.plan(cpu_lock_qty=16, mem_available_kb_=20 * GB_KB, measured_peak_gb=1.5)
        self.assertEqual(declared["slots"], 2)
        self.assertEqual(measured["slots"], 8)
        self.assertTrue(any("measured peak" in r for r in measured["reasons"]))

    def test_a_full_disk_forces_one_at_a_time(self):
        got = self.plan(free_disk_gb=float(MIN_DISK_GB - 1))
        self.assertEqual(got["slots"], 1)
        self.assertTrue(any("free disk" in r for r in got["reasons"]))

    def test_never_more_slots_than_trials(self):
        self.assertEqual(self.plan(trials=2)["slots"], 2)

    def test_a_task_declaring_nothing_runs_one_at_a_time(self):
        got = self.plan(peak={})
        self.assertEqual(got["slots"], 1)
        self.assertTrue(got["fits"])
        self.assertIsNone(got["cpus_each"])

    def test_host_reserve_is_held_back(self):
        self.assertGreater(HOST_RESERVE_GB, 0)


class CliTests(unittest.TestCase):
    SCRIPT = ROOT / "pipeline" / "lib" / "task_resources.py"

    def run_plan(self, tasks: Path, ids: list[str], cpu: int, out: Path):
        selected = tasks.parent / "selected_tasks.txt"
        selected.write_text("\n".join(ids) + "\n", encoding="utf-8")
        return subprocess.run(
            [
                sys.executable,
                str(self.SCRIPT),
                "plan",
                "--tasks-dir",
                str(tasks),
                "--selected",
                str(selected),
                "--cpu",
                str(cpu),
                "--n-rollouts",
                "4",
                "--workdir",
                str(tasks.parent),
                "--out",
                str(out),
            ],
            check=False,
            capture_output=True,
            text=True,
        )

    def test_plan_emits_shell_assignments_and_json(self):
        with tempfile.TemporaryDirectory() as tmp:
            tasks = Path(tmp) / "tasks"
            write_task(tasks, "alpha", DEEPSWE_TOML)
            out = Path(tmp) / "eval_resources.json"
            proc = self.run_plan(tasks, ["alpha"], 8, out)
            self.assertEqual(proc.returncode, 0, proc.stderr)
            lines = dict(ln.split("=", 1) for ln in proc.stdout.strip().splitlines())
            self.assertEqual(lines["DECLARED_CPUS"], "2.0")
            self.assertEqual(lines["DECLARED_MEMORY_MB"], "8192")
            self.assertEqual(lines["DECLARED_STORAGE_MB"], "20480")
            self.assertGreaterEqual(int(lines["EVAL_SLOTS"]), 1)
            doc = json.loads(out.read_text(encoding="utf-8"))
            self.assertEqual(doc["applied"]["source"], "task.toml")
            self.assertEqual(doc["applied"]["cpu_lock_qty"], 8)
            self.assertEqual(doc["trials"], 4)
            self.assertEqual(doc["declared"]["per_task"]["alpha"]["cpus"], 2.0)

    def test_a_worker_that_cannot_honor_the_declaration_exits_nonzero(self):
        with tempfile.TemporaryDirectory() as tmp:
            tasks = Path(tmp) / "tasks"
            write_task(tasks, "beta", LOPSIDED_TOML)
            out = Path(tmp) / "eval_resources.json"
            proc = self.run_plan(tasks, ["beta"], 2, out)
            self.assertEqual(proc.returncode, 1)
            self.assertIn("cannot run the selected tasks as declared", proc.stderr)
            self.assertIn("EVAL_OVERRIDE_CPUS", proc.stderr)
            self.assertFalse(out.exists())


class StageWiringTests(unittest.TestCase):
    def test_p5_plans_before_it_runs_and_keeps_the_declared_values(self):
        p5 = (ROOT / "pipeline" / "stages" / "p5_harness.sh").read_text(encoding="utf-8")
        self.assertIn('task_resources.py" plan', p5)
        self.assertLess(p5.index('task_resources.py" plan'), p5.index("run_harbor() {"))
        self.assertIn("record-resources", p5)
        # Harbor applies task.toml unless an operator deliberately opts out.
        self.assertNotIn('cmd+=(--override-cpus "$EVAL_CPUS_EACH")', p5)
        self.assertIn("EVAL_OVERRIDE_CPUS", p5)
        self.assertIn("EVAL_OVERRIDE_MEMORY_MB", p5)

    def test_provenance_records_declared_against_applied(self):
        from provenance import record_resources

        with tempfile.TemporaryDirectory() as tmp:
            inputs = Path(tmp) / "eval_protocol_inputs.json"
            inputs.write_text(json.dumps({"model": "m"}), encoding="utf-8")
            plan = Path(tmp) / "eval_resources.json"
            plan.write_text(
                json.dumps(
                    {
                        "declared": {"peak": {"cpus": 2, "memory_mb": 8192}, "declared_tasks": 1},
                        "applied": {"slots": 4, "cpus_each": 2, "memory_mb_each": 8192},
                        "host": {"mem_total_gb": 64.0},
                        "reasons": ["cpu_lock 8/2 = 4"],
                    }
                ),
                encoding="utf-8",
            )
            record_resources(inputs, plan)
            doc = json.loads(inputs.read_text(encoding="utf-8"))
            self.assertEqual(doc["resources"]["declared"]["cpus"], 2)
            self.assertEqual(doc["resources"]["applied"]["cpus_each"], 2)
            self.assertEqual(doc["resources"]["host"]["mem_total_gb"], 64.0)


if __name__ == "__main__":
    unittest.main()
