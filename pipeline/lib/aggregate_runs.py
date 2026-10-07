#!/usr/bin/env python3
"""Merge the shards of one suite run into a single report.

A full-suite build splits its tasks across N ``<suite>_some_task`` shards, each
holding its own worker's CPU lock. Every shard archives ``eval-runs/`` and the
aggregator copies those archives here. This script stitches them back into one
harness tree and then runs the ordinary report path over it, so an aggregated
run and a single-worker run are scored by the same code.

Only tasks are merged. Anti-cheat verdicts stay exactly as the shard decided
them (the anticheat phase is local and needs the gold patch), so this re-adds up the counts
rather than re-judging anything.
"""

from __future__ import annotations

import argparse
import json
import shutil
import sys
from pathlib import Path

LIB = Path(__file__).resolve().parent
sys.path.insert(0, str(LIB))

from render_report import build_artifact, write_report  # noqa: E402
from score_results import load_json  # noqa: E402
from task_resources import cpu_count  # noqa: E402

VERDICTS = ("clean", "flagged", "rejected")


def shard_dirs(root: Path) -> list[Path]:
    """Every ``eval-runs`` directory under the copied artifacts, in a stable order.

    copyArtifacts keeps each build's paths, so the layout is
    ``shards/<build>/eval-runs/...`` — but a hand-assembled ``shards/eval-runs``
    has to work too.
    """
    found = sorted({p.resolve() for p in root.rglob("eval-runs") if (p / "harness").is_dir()})
    if not found and (root / "harness").is_dir():
        found = [root]
    return found


def merge_harness(shards: list[Path], dest: Path) -> dict:
    """Copy every shard's trials and anti-cheat verdicts under one harness dir.

    Shard directories are named per build, so trials cannot collide. A task that
    somehow appears in two shards keeps both, and the discovery contract in
    ``score_results`` then takes the newest build that ran it.
    """
    harness = dest / "harness"
    (harness / "harbor_runs").mkdir(parents=True, exist_ok=True)
    (harness / "anticheat").mkdir(parents=True, exist_ok=True)
    tasks: list[str] = []
    summaries: list[dict] = []
    pipelines: list[dict] = []
    protocol: dict | None = None
    resources: dict | None = None
    for index, shard in enumerate(shards, start=1):
        src = shard / "harness" / "harbor_runs"
        if src.is_dir():
            for build in sorted(p for p in src.iterdir() if p.is_dir()):
                target = harness / "harbor_runs" / f"shard{index}-{build.name}"
                if target.exists():
                    shutil.rmtree(target)
                shutil.copytree(build, target, symlinks=True)
        for name in ("anticheat.jsonl", "summary.json"):
            got = shard / "harness" / "anticheat" / name
            if not got.is_file():
                continue
            if name == "summary.json":
                doc = load_json(got)
                if isinstance(doc, dict):
                    summaries.append(doc)
            else:
                with (harness / "anticheat" / name).open("a", encoding="utf-8") as out:
                    out.write(got.read_text(encoding="utf-8"))
        selected = shard / "selected_tasks.txt"
        if selected.is_file():
            for line in selected.read_text(encoding="utf-8").splitlines():
                tid = line.strip()
                if tid and tid not in tasks:
                    tasks.append(tid)
        got = load_json(shard / "eval_protocol_inputs.json")
        if isinstance(got, dict):
            if protocol is None:
                protocol = got
            if isinstance(got.get("pipeline"), dict):
                pipelines.append(got["pipeline"])
        if resources is None:
            got = load_json(shard / "eval_resources.json")
            if isinstance(got, dict):
                resources = got
    merged = merge_anticheat(summaries)
    if merged is not None:
        (harness / "anticheat" / "summary.json").write_text(
            json.dumps(merged, indent=2) + "\n", encoding="utf-8"
        )
    return {
        "harness": harness,
        "tasks": sorted(tasks),
        "protocol": protocol,
        "resources": resources,
        "anticheat": merged,
        "pipelines": pipelines,
        "shards": len(shards),
    }


def pipeline_status(pipelines: list[dict]) -> dict | None:
    """Whether every shard ran the same pipeline build.

    Each worker runs the pipeline embedded in its own mac-k3d binary, so a worker
    that missed a redeploy shows up here as a second commit or pipeline hash.
    """
    found = sorted(
        {(str(p.get("commit") or ""), str(p.get("pipeline_hash") or "")) for p in pipelines}
    )
    if not found:
        return None
    if len(found) == 1:
        return {"pipeline_status": "same"}
    return {
        "pipeline_status": "mixed",
        "pipelines": [{"commit": commit, "pipeline_hash": digest} for commit, digest in found],
    }


def merge_anticheat(summaries: list[dict]) -> dict | None:
    """Add up the shards' verdict counts. A shard that did not run one wins nothing."""
    if not summaries:
        return None
    out = dict(summaries[0])
    counts = {v: 0 for v in VERDICTS}
    attempts = 0
    no_transcript = 0
    overrides = 0
    rejected: list = []
    flagged: list = []
    for summ in summaries:
        got = summ.get("counts") or {}
        for verdict in VERDICTS:
            counts[verdict] += int(got.get(verdict) or 0)
        attempts += int(summ.get("attempts") or 0)
        no_transcript += int(summ.get("no_transcript") or 0)
        overrides += int(summ.get("overrides") or 0)
        rejected.extend(summ.get("rejected") or [])
        flagged.extend(summ.get("flagged") or [])
    out.update(
        {
            "counts": counts,
            "attempts": attempts,
            "no_transcript": no_transcript,
            "overrides": overrides,
            "rejected": rejected,
            "flagged": flagged,
            "shards": len(summaries),
        }
    )
    versions = {str(s.get("version")) for s in summaries if s.get("version")}
    if len(versions) > 1:
        # Different shards ran different pipelines; the merged number is not comparable.
        out["status"] = "mixed_versions"
        out["versions"] = sorted(versions)
    return out


def main() -> int:
    ap = argparse.ArgumentParser()
    ap.add_argument("--shards", required=True, help="dir holding the copied shard artifacts")
    ap.add_argument("--benchmark", required=True)
    ap.add_argument("--run-group", required=True)
    ap.add_argument("--n-rollouts", type=int, default=4)
    ap.add_argument("--out", required=True)
    args = ap.parse_args()

    root = Path(args.shards)
    shards = shard_dirs(root)
    if not shards:
        print(
            f"ERROR: no shard archives under {root}. The shards archive eval-runs/ only when "
            "RUN_GROUP is set, so check that the dispatcher passed it.",
            file=sys.stderr,
        )
        return 1

    out = Path(args.out)
    out.mkdir(parents=True, exist_ok=True)
    merged = merge_harness(shards, out)
    tasks = merged["tasks"]
    if not tasks:
        print(f"ERROR: shards under {root} name no tasks (no selected_tasks.txt)", file=sys.stderr)
        return 1

    protocol = merged["protocol"] or {}
    applied = (merged["resources"] or {}).get("applied") or {}
    doc = build_artifact(
        suite=args.benchmark,
        model=str(protocol.get("model") or ""),
        api_base=str(protocol.get("api_base") or ""),
        task_ids=tasks,
        harness_dir=merged["harness"],
        baseline_dir=out / "baseline",
        n_rollouts=max(1, args.n_rollouts),
        concurrency=int(applied.get("slots") or 0),
        cpus_each=cpu_count(applied.get("cpus_each")) or 0,
        run_id=args.run_group,
        eval_protocol=protocol or None,
    )
    doc["run_group"] = args.run_group
    doc["shards"] = merged["shards"]
    if merged["resources"]:
        doc["resources"] = merged["resources"]
    status = pipeline_status(merged["pipelines"])
    if status:
        doc.update(status)
        if status["pipeline_status"] == "mixed":
            names = ", ".join(p["commit"][:12] or "unknown" for p in status["pipelines"])
            print(
                f"WARNING: shards ran different pipelines ({names}). "
                "A worker missed a redeploy; this report is not one run of one pipeline.",
                file=sys.stderr,
            )
    write_report(doc, out)
    # archive_run.py packs these tasks' trials into the combined .tar.gz.
    (out / "selected_tasks.txt").write_text("".join(f"{tid}\n" for tid in tasks), encoding="utf-8")
    print(
        f"aggregate: {merged['shards']} shards, {len(tasks)} tasks, "
        f"{args.n_rollouts} rollouts -> {out / 'artifact.json'}"
    )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
