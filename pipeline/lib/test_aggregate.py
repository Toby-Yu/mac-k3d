#!/usr/bin/env python3
"""Shard merge tests (PF.5): one report out of many workers' trials."""

from __future__ import annotations

import json
import sys
import tempfile
import unittest
from pathlib import Path

LIB = Path(__file__).resolve().parent
ROOT = LIB.parent.parent
sys.path.insert(0, str(LIB))

from aggregate_runs import main, merge_anticheat, merge_harness, shard_dirs  # noqa: E402


def write_trial(job: Path, task: str, attempt: int, reward: float, patch_bytes: int = 120) -> None:
    trial = job / f"{task}_icode_{attempt}" / "trial"
    (trial / "verifier").mkdir(parents=True)
    (trial / "verifier" / "reward.json").write_text(
        json.dumps(
            {
                "reward": reward,
                "f2p": reward,
                "f2p_total": 4,
                "f2p_passed": int(4 * reward),
                "p2p": 1.0,
                "p2p_total": 2,
                "p2p_passed": 2,
            }
        ),
        encoding="utf-8",
    )
    (trial / "result.json").write_text(json.dumps({"task_name": task}), encoding="utf-8")
    (trial / "agent").mkdir(exist_ok=True)
    (trial / "agent" / "capture.json").write_text(
        json.dumps({"patch": {"bytes": patch_bytes}}), encoding="utf-8"
    )


def make_shard(root: Path, build: str, tasks: dict[str, list[float]]) -> Path:
    """One shard's archived ``eval-runs`` tree, as copyArtifacts would leave it."""
    shard = root / build / "eval-runs"
    job = shard / "harness" / "harbor_runs" / build / "icode_deepswe"
    for task, rewards in tasks.items():
        for attempt, reward in enumerate(rewards, start=1):
            write_trial(job, task, attempt, reward)
    shard.mkdir(parents=True, exist_ok=True)
    (shard / "selected_tasks.txt").write_text("\n".join(tasks) + "\n", encoding="utf-8")
    (shard / "eval_protocol_inputs.json").write_text(
        json.dumps({"model": "deepseek-flash", "api_base": "https://api.deepseek.com/v1"}),
        encoding="utf-8",
    )
    (shard / "eval_resources.json").write_text(
        json.dumps({"applied": {"slots": 2, "cpus_each": 2}}), encoding="utf-8"
    )
    ac = shard / "harness" / "anticheat"
    ac.mkdir(parents=True, exist_ok=True)
    (ac / "summary.json").write_text(
        json.dumps(
            {
                "version": "mac-k3d-anticheat-v1",
                "counts": {"clean": len(tasks), "flagged": 0, "rejected": 0},
                "attempts": sum(len(v) for v in tasks.values()),
                "no_transcript": 0,
                "overrides": 0,
                "rejected": [],
                "flagged": [],
            }
        ),
        encoding="utf-8",
    )
    return shard


class ShardDiscoveryTests(unittest.TestCase):
    def test_finds_every_shard_under_the_copied_artifacts(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            make_shard(root, "101", {"alpha": [1.0]})
            make_shard(root, "102", {"beta": [0.0]})
            got = shard_dirs(root)
            self.assertEqual(len(got), 2)
            self.assertEqual({p.parent.name for p in got}, {"101", "102"})

    def test_a_hand_assembled_tree_also_works(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            (root / "harness").mkdir()
            self.assertEqual(shard_dirs(root), [root])

    def test_no_shards_is_not_a_crash(self):
        with tempfile.TemporaryDirectory() as tmp:
            self.assertEqual(shard_dirs(Path(tmp)), [])


class MergeTests(unittest.TestCase):
    def test_trials_from_two_workers_land_under_one_harness(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            make_shard(root, "101", {"alpha": [1.0, 0.0]})
            make_shard(root, "102", {"beta": [1.0, 1.0]})
            merged = merge_harness(shard_dirs(root), root / "out")
            self.assertEqual(merged["tasks"], ["alpha", "beta"])
            self.assertEqual(merged["shards"], 2)
            builds = sorted(p.name for p in (merged["harness"] / "harbor_runs").iterdir())
            # Prefixed per shard, so two workers' builds cannot collide.
            self.assertEqual(builds, ["shard1-101", "shard2-102"])
            self.assertEqual(merged["protocol"]["model"], "deepseek-flash")
            self.assertEqual(merged["anticheat"]["attempts"], 4)
            self.assertEqual(merged["anticheat"]["counts"]["clean"], 2)

    def test_verdict_counts_add_up_across_shards(self):
        got = merge_anticheat(
            [
                {
                    "version": "v1",
                    "counts": {"clean": 3, "flagged": 1, "rejected": 0},
                    "attempts": 4,
                    "rejected": [],
                    "flagged": [{"task": "a"}],
                },
                {
                    "version": "v1",
                    "counts": {"clean": 1, "flagged": 0, "rejected": 2},
                    "attempts": 3,
                    "rejected": [{"task": "b"}, {"task": "c"}],
                    "flagged": [],
                },
            ]
        )
        self.assertEqual(got["counts"], {"clean": 4, "flagged": 1, "rejected": 2})
        self.assertEqual(got["attempts"], 7)
        self.assertEqual(len(got["rejected"]), 2)
        self.assertEqual(got["shards"], 2)
        self.assertNotIn("status", got)

    def test_shards_on_different_pipelines_are_flagged_not_averaged(self):
        got = merge_anticheat(
            [
                {"version": "v1", "counts": {"clean": 1, "flagged": 0, "rejected": 0}, "attempts": 1},
                {"version": "v2", "counts": {"clean": 1, "flagged": 0, "rejected": 0}, "attempts": 1},
            ]
        )
        self.assertEqual(got["status"], "mixed_versions")
        self.assertEqual(got["versions"], ["v1", "v2"])

    def test_no_summaries_means_no_merged_verdict(self):
        self.assertIsNone(merge_anticheat([]))


class CliTests(unittest.TestCase):
    def run_main(self, shards: Path, out: Path, rollouts: int = 2) -> int:
        argv = sys.argv
        sys.argv = [
            "aggregate_runs.py",
            "--shards",
            str(shards),
            "--benchmark",
            "deepswe",
            "--run-group",
            "deepswe_full_suite_task-7",
            "--n-rollouts",
            str(rollouts),
            "--out",
            str(out),
        ]
        try:
            return main()
        finally:
            sys.argv = argv

    def test_one_report_covers_every_shard(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            make_shard(root, "101", {"alpha": [1.0, 0.0]})
            make_shard(root, "102", {"beta": [1.0, 1.0]})
            out = root / "aggregate"
            self.assertEqual(self.run_main(root, out), 0)
            doc = json.loads((out / "artifact.json").read_text(encoding="utf-8"))
            self.assertEqual(doc["run_group"], "deepswe_full_suite_task-7")
            self.assertEqual(doc["shards"], 2)
            self.assertEqual(doc["n_tasks"], 2)
            self.assertEqual(doc["n_rollouts"], 2)
            tasks = {row["id"]: row for row in doc["icode"]["tasks"]}
            self.assertEqual(set(tasks), {"alpha", "beta"})
            # tasks x rollouts records, from two different workers. A task whose
            # every attempt passed is stored compacted, so count n, not rollouts.
            self.assertEqual(sum(row["n"] for row in tasks.values()), 4)
            self.assertEqual(len(tasks["alpha"]["rollouts"]), 2)
            self.assertEqual(tasks["beta"]["c"], 2)
            self.assertEqual(tasks["alpha"]["c"], 1)
            self.assertEqual(doc["concurrency"], 2)
            self.assertEqual(doc["cpus_each"], 2)
            self.assertTrue((out / "summary.md").is_file())
            self.assertTrue((out / "report.html").is_file())

    def test_an_empty_collection_says_why(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            self.assertEqual(self.run_main(root, root / "out"), 1)


class EmptyPatchP2PTests(unittest.TestCase):
    def test_a_trial_that_changed_nothing_does_not_count_as_broken_p2p(self):
        """LoLBench reports applied=0 -> p2p 0/N for an empty patch. That is not a regression."""
        from score_results import rates_for_trial

        with tempfile.TemporaryDirectory() as tmp:
            trial = Path(tmp) / "trial"
            (trial / "verifier").mkdir(parents=True)
            (trial / "verifier" / "reward.json").write_text(
                json.dumps(
                    {
                        "applied": 0.0,
                        "build_ok": 0.0,
                        "f2p_pass": 0,
                        "f2p_pass_rate": 0.0,
                        "f2p_total": 27,
                        "p2p_pass": 0,
                        "p2p_pass_rate": 0.0,
                        "p2p_total": 52,
                        "reward": 0.0,
                    }
                ),
                encoding="utf-8",
            )
            (trial / "agent").mkdir()
            (trial / "agent" / "capture.json").write_text(
                json.dumps({"patch": {"bytes": 0}}), encoding="utf-8"
            )
            rates = rates_for_trial(trial)
            self.assertTrue(rates["empty_patch"])
            self.assertIsNone(rates["p2p"])
            self.assertIsNone(rates["p2p_pass"])
            self.assertIsNone(rates["p2p_total"])
            # F2P still counts: the agent genuinely did not fix the bug.
            self.assertEqual(rates["f2p"], 0.0)

    def test_a_real_patch_keeps_its_p2p(self):
        from score_results import rates_for_trial

        with tempfile.TemporaryDirectory() as tmp:
            trial = Path(tmp) / "trial"
            (trial / "verifier").mkdir(parents=True)
            (trial / "verifier" / "reward.json").write_text(
                json.dumps(
                    {"reward": 0, "f2p": 0.5, "f2p_total": 2, "f2p_passed": 1, "p2p": 0.0, "p2p_total": 9, "p2p_passed": 0}
                ),
                encoding="utf-8",
            )
            (trial / "agent").mkdir()
            (trial / "agent" / "capture.json").write_text(
                json.dumps({"patch": {"bytes": 900}}), encoding="utf-8"
            )
            rates = rates_for_trial(trial)
            self.assertNotIn("empty_patch", rates)
            self.assertEqual(rates["p2p"], 0.0)
            self.assertEqual(rates["p2p_total"], 9)

    def test_no_receipt_leaves_the_rates_alone(self):
        from score_results import rates_for_trial, trial_patch_bytes

        with tempfile.TemporaryDirectory() as tmp:
            trial = Path(tmp) / "trial"
            (trial / "verifier").mkdir(parents=True)
            (trial / "verifier" / "reward.json").write_text(
                json.dumps({"reward": 1, "p2p": 1.0, "p2p_total": 3, "p2p_passed": 3}), encoding="utf-8"
            )
            self.assertIsNone(trial_patch_bytes(trial))
            self.assertEqual(rates_for_trial(trial)["p2p"], 1.0)


if __name__ == "__main__":
    unittest.main()
