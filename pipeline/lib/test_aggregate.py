#!/usr/bin/env python3
"""Shard merge tests (PF.5): one report out of many workers' trials."""

from __future__ import annotations

import json
import shutil
import sys
import tempfile
import unittest
from pathlib import Path

LIB = Path(__file__).resolve().parent
ROOT = LIB.parent.parent
sys.path.insert(0, str(LIB))

from aggregate_runs import main, merge_anticheat, merge_harness, pipeline_status, shard_dirs  # noqa: E402


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
    # Harbor 0.22: task_name is <org>/<id> and task_id names the task dir.
    (trial / "result.json").write_text(
        json.dumps({"task_name": f"datacurve/{task}", "task_id": {"path": f"/eval-runs/deep-swe/tasks/{task}"}}),
        encoding="utf-8",
    )
    (trial / "agent").mkdir(exist_ok=True)
    (trial / "agent" / "capture.json").write_text(
        json.dumps({"patch": {"bytes": patch_bytes}}), encoding="utf-8"
    )


def make_shard(
    root: Path,
    build: str,
    tasks: dict[str, list[float]],
    commit: str = "84c66ededd24",
    inputs: dict | None = None,
) -> Path:
    """One shard's archived ``eval-runs`` tree, as copyArtifacts would leave it."""
    shard = root / build / "eval-runs"
    job = shard / "harness" / "harbor_runs" / build / "icode_deepswe"
    for task, rewards in tasks.items():
        for attempt, reward in enumerate(rewards, start=1):
            write_trial(job, task, attempt, reward)
    shard.mkdir(parents=True, exist_ok=True)
    (shard / "selected_tasks.txt").write_text("\n".join(tasks) + "\n", encoding="utf-8")
    pipeline = {"source": "binary", "commit": commit, "dirty": False, "pipeline_hash": f"hash-{commit}"}
    (shard / "eval_protocol_inputs.json").write_text(
        json.dumps(
            {
                "model": "deepseek-flash",
                "api_base": "https://api.deepseek.com/v1",
                "pipeline": pipeline,
                **(inputs or {}),
            }
        ),
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


# What a shard's post step archives (src/prepare/jenkins_job.rs), relative to eval-runs/.
SHARD_ARCHIVE = (
    "harness/harbor_runs/jenkins-{build}",
    "harness/anticheat",
    "selected_tasks.txt",
    "suite_tasks.txt",
    "eval_protocol_inputs.json",
    "eval_resources.json",
)


def archive_like_a_shard(work: Path, build: str, dest: Path) -> None:
    """Copy a worker's eval-runs/ the way the shard archive and copyArtifacts would."""
    for pattern in SHARD_ARCHIVE:
        rel = pattern.format(build=build)
        src, target = work / rel, dest / rel
        if src.is_dir():
            shutil.copytree(src, target)
        elif src.is_file():
            target.parent.mkdir(parents=True, exist_ok=True)
            shutil.copy2(src, target)


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
            self.assertEqual(merged["protocol"]["model_params"]["model"], "deepseek-flash")
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

    def test_pipeline_status_compares_commit_and_hash(self):
        self.assertIsNone(pipeline_status([]))
        same = {"commit": "a", "pipeline_hash": "h"}
        self.assertEqual(pipeline_status([same, dict(same)]), {"pipeline_status": "same"})
        # Same commit, different embedded scripts: a dirty build on one worker.
        got = pipeline_status([same, {"commit": "a", "pipeline_hash": "other"}])
        self.assertEqual(got["pipeline_status"], "mixed")
        self.assertEqual(len(got["pipelines"]), 2)


def run_aggregate(shards: Path, out: Path, rollouts: int = 2, extra: tuple[str, ...] = ()) -> int:
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
        *extra,
        "--out",
        str(out),
    ]
    try:
        return main()
    finally:
        sys.argv = argv


class CliTests(unittest.TestCase):
    def run_main(self, shards: Path, out: Path, rollouts: int = 2) -> int:
        return run_aggregate(shards, out, rollouts)

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
            self.assertEqual(doc["pipeline_status"], "same")
            self.assertNotIn("pipelines", doc)
            self.assertTrue((out / "summary.md").is_file())
            self.assertTrue((out / "report.html").is_file())

    def test_the_combined_tar_gz_holds_every_shard(self):
        """The Aggregate stage packs aggregate/ with archive_run.py (src/prepare/jenkins_job.rs)."""
        import tarfile

        from archive_run import main as archive_main

        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            make_shard(root / "shards", "101", {"alpha": [1.0]})
            make_shard(root / "shards", "102", {"beta": [0.0]})
            out = root / "aggregate"
            self.assertEqual(self.run_main(root / "shards", out, rollouts=1), 0)
            self.assertEqual((out / "selected_tasks.txt").read_text(encoding="utf-8"), "alpha\nbeta\n")
            group = "deepswe_full_suite_task-7"
            self.assertEqual(
                archive_main(
                    [
                        "--report-dir", str(out),
                        "--harness-dir", str(out / "harness"),
                        "--task-file", str(out / "selected_tasks.txt"),
                        "--suite", "deepswe",
                        "--backup-root", str(root / "backup"),
                        "--run-folder", group,
                    ]
                ),
                0,
            )
            with tarfile.open(root / "backup" / "deepswe" / f"{group}.tar.gz") as tar:
                names = set(tar.getnames())
            self.assertIn(f"{group}/artifact.json", names)
            self.assertIn(f"{group}/tasks.txt", names)
            self.assertIn(f"{group}/anticheat/summary.json", names)
            for task in ("alpha", "beta"):
                self.assertTrue(
                    any(n.startswith(f"{group}/icode/{task}/attempt-01/") for n in names), f"{task}: {sorted(names)}"
                )

    def test_shards_from_reused_workspaces_merge_only_their_own_build(self):
        """Each worker still holds a pre-PF.1 build of the same task, scored 0."""
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            copied = root / "shards"
            for build, task in (("101", "alpha"), ("102", "beta")):
                work = make_shard(root / "workspaces", f"jenkins-{build}", {task: [1.0]})
                write_trial(work / "harness" / "harbor_runs" / "jenkins-40" / task / "job", task, 1, 0.0)
                archive_like_a_shard(work, build, copied / build / "eval-runs")
            out = root / "aggregate"
            self.assertEqual(self.run_main(copied, out, rollouts=1), 0)
            doc = json.loads((out / "artifact.json").read_text(encoding="utf-8"))
            builds = sorted(p.name for p in (out / "harness" / "harbor_runs").iterdir())
            self.assertEqual(builds, ["shard1-jenkins-101", "shard2-jenkins-102"])
            self.assertEqual(doc["shards"], 2)
            self.assertEqual({row["id"]: row["c"] for row in doc["icode"]["tasks"]}, {"alpha": 1, "beta": 1})
            self.assertEqual(doc["icode"]["macro_pass@1"], 1.0)
            self.assertEqual(doc["anticheat"]["status"], "ok")
            self.assertEqual(doc["anticheat"]["attempts"], 2)
            self.assertEqual(doc["anticheat"]["counts"]["clean"], 2)
            self.assertIn("clean **2**", (out / "summary.md").read_text(encoding="utf-8"))

    def test_shards_from_different_pipeline_builds_are_marked_mixed(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            make_shard(root, "101", {"alpha": [1.0]}, commit="aaaa")
            make_shard(root, "102", {"beta": [1.0]}, commit="bbbb")
            out = root / "aggregate"
            self.assertEqual(self.run_main(root, out, rollouts=1), 0)
            doc = json.loads((out / "artifact.json").read_text(encoding="utf-8"))
            self.assertEqual(doc["pipeline_status"], "mixed")
            self.assertEqual(
                doc["pipelines"],
                [
                    {"commit": "aaaa", "pipeline_hash": "hash-aaaa"},
                    {"commit": "bbbb", "pipeline_hash": "hash-bbbb"},
                ],
            )

    def test_float_cpus_each_from_an_old_plan_is_whole(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            shard = make_shard(root, "101", {"alpha": [1.0]})
            (shard / "eval_resources.json").write_text(
                json.dumps({"applied": {"slots": 1, "cpus_each": 2.0}}), encoding="utf-8"
            )
            out = root / "aggregate"
            self.assertEqual(self.run_main(root, out, rollouts=1), 0)
            doc = json.loads((out / "artifact.json").read_text(encoding="utf-8"))
            self.assertEqual(doc["cpus_each"], 2)
            self.assertIsInstance(doc["cpus_each"], int)

    def test_an_empty_collection_says_why(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            self.assertEqual(self.run_main(root, root / "out"), 1)

    def test_every_shard_keeps_its_own_worker_and_model_params(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            for build, node in (("101", "mac-cloud"), ("102", "mac-home")):
                make_shard(
                    root,
                    build,
                    {f"task{build}": [1.0]},
                    inputs={
                        "provider": "DeepSeek",
                        "reasoning_effort": "high",
                        "max_tokens": 65536,
                        "max_iterations": 500,
                        "concurrency": 2,
                        "cpus_each": 2,
                        "worker": {"node": node, "nproc": 16},
                        "images": {f"task{build}": {"id": f"sha256:{build}"}},
                    },
                )
            out = root / "aggregate"
            self.assertEqual(self.run_main(root, out, rollouts=1), 0)
            doc = json.loads((out / "artifact.json").read_text(encoding="utf-8"))
            protocol = doc["eval_protocol"]
            self.assertEqual(protocol["model_params_status"], "same")
            self.assertEqual([s["worker"]["node"] for s in protocol["shards"]], ["mac-cloud", "mac-home"])
            self.assertEqual([s["builds"] for s in protocol["shards"]], [["101"], ["102"]])
            self.assertEqual(protocol["model_params"]["max_tokens"], 65536)
            # Both shards' image pins, not only the first shard's.
            self.assertEqual(sorted(protocol["images"]), ["task101", "task102"])
            md = (out / "summary.md").read_text(encoding="utf-8")
            self.assertIn("- Worker (shard 1 101): node mac-cloud", md)
            self.assertIn("- Worker (shard 2 102): node mac-home", md)
            self.assertIn("max_tokens `65536` · max_iterations `500`", md)
            page = (out / "report.html").read_text(encoding="utf-8")
            self.assertIn("Worker (shard 2 102): node mac-home", page)

    def test_shards_with_different_max_tokens_are_marked_mixed(self):
        import contextlib
        import io

        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            make_shard(root, "101", {"alpha": [1.0]}, inputs={"max_tokens": 65536, "worker": {"node": "a"}})
            make_shard(root, "102", {"beta": [1.0]}, inputs={"max_tokens": 8192, "worker": {"node": "b"}})
            out = root / "aggregate"
            err = io.StringIO()
            with contextlib.redirect_stderr(err):
                self.assertEqual(self.run_main(root, out, rollouts=1), 0)
            doc = json.loads((out / "artifact.json").read_text(encoding="utf-8"))
            self.assertEqual(doc["eval_protocol"]["model_params_status"], "mixed")
            self.assertEqual(doc["eval_protocol"]["model_params_mismatch"], {"max_tokens": [8192, 65536]})
            self.assertIn("WARNING: shards ran different model params or iCode (max_tokens [8192, 65536])", err.getvalue())
            md = (out / "summary.md").read_text(encoding="utf-8")
            self.assertIn("- Shards differ: max_tokens [8192, 65536]. Not one measurement of one iCode setup.", md)

    def test_a_shards_own_report_supplies_the_icode_version(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            shard = make_shard(root, "101", {"alpha": [1.0]})
            report = shard / "output" / "deepswe" / "101-20261007T000000Z"
            report.mkdir(parents=True)
            own = {
                "icode": {"mode": "git", "version": "eea9d66"},
                "model_params": {"model": "deepseek-flash", "provider": "DeepSeek", "max_tokens": 65536},
                "resources": {"concurrency": 2, "cpus_each": 2},
                "worker": {"node": "mac-cloud"},
            }
            (report / "artifact.json").write_text(
                json.dumps({"run_id": "101", "eval_protocol": own}), encoding="utf-8"
            )
            out = root / "aggregate"
            self.assertEqual(self.run_main(root, out, rollouts=1), 0)
            doc = json.loads((out / "artifact.json").read_text(encoding="utf-8"))
            self.assertEqual(doc["eval_protocol"]["icode"]["version"], "eea9d66")
            self.assertEqual(doc["eval_protocol"]["shards"][0]["icode_version"], "eea9d66")
            self.assertIn("- iCode version: `eea9d66`", (out / "summary.md").read_text(encoding="utf-8"))


SHARD_URL = "http://jenkins:8080/job/deepswe_one_task/{build}/"
DISPATCHER_URL = "http://jenkins:8080/job/deepswe_some_task/5/"
DEFAULT_PROBE = "alpine:3.23.4@sha256:5b10"
SUBSTITUTE_PROBE = "alpine:3.19@sha256:6baf"


def shard_inputs(build: str, node: str, probe: str = DEFAULT_PROBE, **extra) -> dict:
    """What a shard's tasks phase records, naming the build that wrote it."""
    return {
        "requester": {"user": "toby", "build_url": SHARD_URL.format(build=build)},
        "worker": {"node": node, "nproc": 16},
        "max_tokens": 65536,
        "max_iterations": 500,
        "isolation": {
            "egress_probe": {"harbor_default": DEFAULT_PROBE, "image": probe, "substituted": probe != DEFAULT_PROBE}
        },
        **extra,
    }


def set_anticheat(shard: Path, clean: int, attempts: int) -> None:
    summary = shard / "harness" / "anticheat" / "summary.json"
    doc = json.loads(summary.read_text(encoding="utf-8"))
    doc["counts"]["clean"] = clean
    doc["attempts"] = attempts
    summary.write_text(json.dumps(doc), encoding="utf-8")


class DispatcherPlanTests(unittest.TestCase):
    """The dispatcher's shards/plan.txt (src/prepare/jenkins_job.rs) and reused shard workspaces."""

    def collect(self, root: Path, build: str, work: Path) -> None:
        archive_like_a_shard(work, build, root / "shards" / build / "eval-runs")

    def run_plan(
        self, root: Path, plan: list[str], rollouts: int = 4, extra: tuple[str, ...] = ()
    ) -> tuple[int, str]:
        import contextlib
        import io

        (root / "shards").mkdir(parents=True, exist_ok=True)
        (root / "shards" / "plan.txt").write_text("".join(f"{line}\n" for line in plan), encoding="utf-8")
        err = io.StringIO()
        with contextlib.redirect_stderr(err):
            code = run_aggregate(
                root / "shards",
                root / "aggregate",
                rollouts,
                ("--plan", str(root / "shards" / "plan.txt"), "--build-url", DISPATCHER_URL, *extra),
            )
        return code, err.getvalue()

    def report(self, root: Path) -> tuple[dict, str, str]:
        out = root / "aggregate"
        return (
            json.loads((out / "artifact.json").read_text(encoding="utf-8")),
            (out / "summary.md").read_text(encoding="utf-8"),
            (out / "report.html").read_text(encoding="utf-8"),
        )

    def test_a_shard_that_failed_early_does_not_ship_the_previous_builds_files(self):
        """deepswe_some_task #5: cloud shard #61 stopped before Harbor and still held build #58's files."""
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            # Written before the image table kept only the run's tasks.
            images = {"Yb43UkF": {"id": "sha256:y"}, "abs-module-cache-flags": {"id": "sha256:old"}}
            local = make_shard(
                root / "home", "jenkins-60", {"Yb43UkF": [1.0, 1.0, 1.0, 0.0]},
                inputs=shard_inputs("60", "mac-Michael-Ubuntu", images=images),
            )
            set_anticheat(local, clean=4, attempts=4)
            self.collect(root, "60", local)
            stale = make_shard(
                root / "cloud", "jenkins-58", {"Yb43UkF": [0.0, 0.0]}, commit="a3d2e19",
                inputs=shard_inputs("58", "mac-iZt4ndd2dff7gqjta7mppaZ", SUBSTITUTE_PROBE, max_tokens=8192),
            )
            set_anticheat(stale, clean=2, attempts=2)
            self.collect(root, "61", stale)
            code, err = self.run_plan(
                root,
                [
                    "1|60|SUCCESS|0|1|Yb43UkF",
                    "2|61|FAILURE|1|2|wazero-multi-module-snapshots,ytt-jsonpath-query-api",
                ],
            )
            self.assertEqual(code, 0, err)
            doc, md, page = self.report(root)

            self.assertIn(f"WARNING: shard 2 #61 holds files from {SHARD_URL.format(build='58')}; ignored", err)
            self.assertEqual(doc["anticheat"]["counts"]["clean"], 4)
            self.assertEqual(doc["anticheat"]["attempts"], 4)
            self.assertEqual(doc["pipeline_status"], "same")
            protocol = doc["eval_protocol"]
            self.assertEqual(protocol["model_params_status"], "same")
            self.assertNotIn("Shards differ", md)
            self.assertEqual(doc["shards"], 2)
            tasks = {row["id"]: row for row in doc["icode"]["tasks"]}
            self.assertEqual((tasks["Yb43UkF"]["c"], tasks["Yb43UkF"]["n_scored"]), (3, 4))
            for tid in ("wazero-multi-module-snapshots", "ytt-jsonpath-query-api"):
                self.assertEqual((tasks[tid]["c"], tasks[tid]["n_scored"], tasks[tid]["unscored"]), (0, 0, 4))
            self.assertIn("(missing/excluded: 2)", md)
            failed = protocol["shards"][1]
            self.assertEqual(failed["builds"], ["jenkins-61"])
            self.assertEqual(failed["result"], "FAILURE")
            self.assertEqual(failed["worker"], {})
            self.assertEqual(failed["not_run"], ["wazero-multi-module-snapshots", "ytt-jsonpath-query-api"])
            line = (
                "Worker (shard 2 jenkins-61): FAILURE before any trial · "
                "2 questions not run: wazero-multi-module-snapshots, ytt-jsonpath-query-api"
            )
            self.assertIn(f"- {line}", md)
            self.assertIn(line, page)
            self.assertIn("- Worker (shard 1 jenkins-60): node mac-Michael-Ubuntu", md)
            self.assertIn(f"egress probe Harbor default {DEFAULT_PROBE}", md)
            self.assertNotIn(SUBSTITUTE_PROBE, md)
            self.assertIn(f"- Requester: `toby` `{DISPATCHER_URL}`", md)
            self.assertIn(
                "- Coverage: 1 of 3 planned questions have trials; 2 not run (shard 2 jenkins-61 FAILURE)", md
            )
            self.assertEqual(doc["model"], "openai/deepseek-flash")
            self.assertEqual(sorted(protocol["images"]), ["Yb43UkF"])
            self.assertNotIn("abs-module-cache-flags", md)
            # Only the shard's own trials were merged; the stale shard archived none.
            runs = sorted(p.name for p in (root / "aggregate" / "harness" / "harbor_runs").iterdir())
            self.assertEqual(runs, ["shard1-jenkins-60"])

    def test_a_shard_that_archived_nothing_still_gets_a_row(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            local = make_shard(root / "home", "jenkins-60", {"alpha": [1.0]}, inputs=shard_inputs("60", "a"))
            self.collect(root, "60", local)
            code, err = self.run_plan(root, ["1|60|SUCCESS|0|1|alpha", "2|61|ABORTED|1|1|beta"], rollouts=1)
            self.assertEqual(code, 0, err)
            doc, md, _ = self.report(root)
            self.assertEqual(doc["n_tasks"], 2)
            self.assertIn("- Worker (shard 2 jenkins-61): ABORTED before any trial · 1 question not run: beta", md)

    def offset_shard(self, root: Path, build: str, tasks: dict[str, list[float]], suite: list[str]) -> None:
        """A full_suite_task shard: selected by offset, with the suite list its tasks phase wrote."""
        work = make_shard(root / f"ws{build}", f"jenkins-{build}", tasks, inputs=shard_inputs(build, f"node{build}"))
        (work / "suite_tasks.txt").write_text("".join(f"{tid}\n" for tid in suite), encoding="utf-8")
        self.collect(root, build, work)

    def test_an_offset_shard_that_ran_nothing_is_named_from_the_suite_list(self):
        """full_suite_task names no ids, so a failed shard's questions come from the shards' suite_tasks.txt."""
        suite = ["alpha", "beta", "delta", "epsilon", "gamma", "zeta"]
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            self.offset_shard(root, "60", {"alpha": [1.0]}, suite)
            code, err = self.run_plan(root, ["1|60|SUCCESS|0|1|", "2|61|FAILURE|1|5|"], rollouts=1)
            self.assertEqual(code, 0, err)
            doc, md, page = self.report(root)
            failed = doc["eval_protocol"]["shards"][1]
            self.assertEqual(failed["not_run"], suite[1:])
            self.assertNotIn("not_run_count", failed)
            self.assertEqual(doc["n_tasks"], 6)
            self.assertIn("(missing/excluded: 5)", md)
            self.assertIn(
                "- Worker (shard 2 jenkins-61): FAILURE before any trial · "
                "5 questions not run: beta, delta, epsilon, gamma, zeta",
                md,
            )
            coverage = "Coverage: 1 of 6 planned questions have trials; 5 not run (shard 2 jenkins-61 FAILURE)"
            self.assertIn(f"- {coverage}", md)
            self.assertIn(coverage, page)
            self.assertEqual(
                {k: doc["coverage"][k] for k in ("planned", "with_trials", "not_named", "in_metrics")},
                {"planned": 6, "with_trials": 1, "not_named": 0, "in_metrics": 6},
            )
            self.assertIn("5 questions count as missing", err)
            self.assertNotIn("Pass@k covers", err)

    def test_an_offset_shard_stays_unnamed_when_the_suite_lists_disagree(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            self.offset_shard(root, "60", {"alpha": [1.0]}, ["alpha", "beta", "eta"])
            self.offset_shard(root, "62", {"eta": [1.0]}, ["alpha", "eta", "beta"])
            code, err = self.run_plan(
                root, ["1|60|SUCCESS|0|1|", "2|61|FAILURE|1|5|", "3|62|SUCCESS|6|1|"], rollouts=1
            )
            self.assertEqual(code, 0, err)
            doc, md, _ = self.report(root)
            failed = doc["eval_protocol"]["shards"][1]
            self.assertEqual((failed["not_run_count"], failed["not_run_offset"]), (5, 1))
            self.assertIn("WARNING: the shards listed different suites", err)
            self.assertIn("its 5 questions at offset 1 are not named, so the report does not count them", err)
            self.assertIn("WARNING: the dispatcher planned 7 questions but Pass@k covers 2", err)
            self.assertIn("- Worker (shard 2 jenkins-61): FAILURE before any trial · 5 questions at offset 1 not run", md)
            self.assertIn(
                "- Coverage: 2 of 7 planned questions have trials; 5 not run (shard 2 jenkins-61 FAILURE); "
                "5 not named, so Pass@k covers 2 questions",
                md,
            )

    def test_a_slice_overlapping_another_shards_questions_is_flagged(self):
        """Two workers that sorted the suite differently cut overlapping slices."""
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            self.offset_shard(root, "60", {"beta": [1.0]}, ["alpha", "beta", "gamma"])
            code, err = self.run_plan(root, ["1|60|SUCCESS|0|1|", "2|61|FAILURE|1|2|"], rollouts=1)
            self.assertEqual(code, 0, err)
            self.assertIn(
                "WARNING: shard 2 jenkins-61's slice overlaps questions another shard ran (beta); "
                "the shards cut different slices of the suite",
                err,
            )

    def test_the_dispatchers_user_is_the_requester(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            local = make_shard(root / "home", "jenkins-60", {"alpha": [1.0]}, inputs=shard_inputs("60", "a"))
            self.collect(root, "60", local)
            code, err = self.run_plan(root, ["1|60|SUCCESS|0|1|alpha"], rollouts=1, extra=("--requester-user", "Toby"))
            self.assertEqual(code, 0, err)
            doc, md, page = self.report(root)
            self.assertEqual(doc["eval_protocol"]["requester"], {"user": "Toby", "build_url": DISPATCHER_URL})
            self.assertIn(f"- Requester: `Toby` `{DISPATCHER_URL}`", md)
            self.assertIn(f"Requester Toby {DISPATCHER_URL}", page)

    def test_a_fresh_shard_from_its_own_build_counts_in_full(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            for build, task, node in (("60", "alpha", "home"), ("61", "beta", "cloud")):
                work = make_shard(root / node, f"jenkins-{build}", {task: [1.0, 0.0]}, inputs=shard_inputs(build, node))
                self.collect(root, build, work)
            code, err = self.run_plan(root, ["1|60|SUCCESS|0|1|alpha", "2|61|SUCCESS|1|1|beta"], rollouts=2)
            self.assertEqual(code, 0, err)
            self.assertNotIn("WARNING", err)
            doc, md, _ = self.report(root)
            self.assertEqual({row["id"] for row in doc["icode"]["tasks"]}, {"alpha", "beta"})
            self.assertEqual(doc["anticheat"]["attempts"], 4)
            self.assertNotIn("not run", md)
            self.assertIn("- Coverage: 2 of 2 planned questions have trials", md)
            self.assertIn("- Worker (shard 2 jenkins-61): node cloud", md)
            self.assertIn(f"- Egress probe: Harbor default {DEFAULT_PROBE}", md)

    def test_shards_with_different_egress_probes_each_name_their_own(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            for build, task, node, probe in (
                ("60", "alpha", "home", DEFAULT_PROBE),
                ("61", "beta", "cloud", SUBSTITUTE_PROBE),
            ):
                work = make_shard(root / node, f"jenkins-{build}", {task: [1.0]}, inputs=shard_inputs(build, node, probe))
                self.collect(root, build, work)
            code, err = self.run_plan(root, ["1|60|SUCCESS|0|1|alpha", "2|61|SUCCESS|1|1|beta"], rollouts=1)
            self.assertEqual(code, 0, err)
            doc, md, _ = self.report(root)
            self.assertNotIn("egress_probe", doc["eval_protocol"].get("isolation") or {})
            self.assertNotIn("- Egress probe:", md)
            lines = [line for line in md.splitlines() if line.startswith("- Worker (shard")]
            self.assertTrue(lines[0].startswith("- Worker (shard 1 jenkins-60): node home · nproc 16"), lines[0])
            self.assertIn(f"egress probe Harbor default {DEFAULT_PROBE}", lines[0])
            self.assertIn(f"egress probe substituted {SUBSTITUTE_PROBE}", lines[1])

    def test_without_a_plan_every_archived_shard_counts_as_before(self):
        """An older dispatcher passes no --plan; nothing is dropped."""
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            stale = make_shard(root / "cloud", "jenkins-58", {"alpha": [1.0]}, inputs=shard_inputs("58", "cloud"))
            archive_like_a_shard(stale, "58", root / "shards" / "61" / "eval-runs")
            self.assertEqual(run_aggregate(root / "shards", root / "aggregate", 1), 0)
            doc, _, _ = self.report(root)
            self.assertEqual(doc["n_tasks"], 1)
            self.assertEqual(doc["anticheat"]["counts"]["clean"], 1)
            self.assertEqual(doc["eval_protocol"]["requester"]["build_url"], SHARD_URL.format(build="58"))


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
            self.assertIsNone(rates["partial"])
            # What the grader said is kept for the report, outside the averages.
            self.assertEqual((rates["grader_p2p_pass"], rates["grader_p2p_total"]), (0, 52))
            # F2P still counts: the agent genuinely did not fix the bug.
            self.assertEqual(rates["f2p"], 0.0)

    def test_deepswe_passes_the_base_repo_and_that_is_kept_as_the_graders_count(self):
        """DeepSWE runs the untouched base's tests for an empty patch, so they pass (1/1)."""
        from score_results import rates_for_trial

        with tempfile.TemporaryDirectory() as tmp:
            trial = Path(tmp) / "trial"
            (trial / "verifier").mkdir(parents=True)
            (trial / "verifier" / "reward.json").write_text(
                json.dumps(
                    {"reward": 0, "f2p": 0.0, "f2p_total": 103, "f2p_passed": 0, "p2p": 1.0, "p2p_total": 1, "p2p_passed": 1}
                ),
                encoding="utf-8",
            )
            (trial / "agent").mkdir()
            (trial / "agent" / "capture.json").write_text(json.dumps({"patch": {"bytes": 0}}), encoding="utf-8")
            rates = rates_for_trial(trial)
            self.assertIsNone(rates["p2p"])
            self.assertEqual((rates["grader_p2p_pass"], rates["grader_p2p_total"]), (1, 1))

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
