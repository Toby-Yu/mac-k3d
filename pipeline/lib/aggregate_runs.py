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

With ``--plan`` (the dispatcher's ``shards/plan.txt``) every shard the dispatcher
started gets a row, including one that archived nothing, and a shard's files
count only when its own build wrote them: a workspace is reused, so a build that
stopped early still holds the previous build's inputs and verdicts.
"""

from __future__ import annotations

import argparse
import json
import shutil
import sys
from pathlib import Path

LIB = Path(__file__).resolve().parent
sys.path.insert(0, str(LIB))

from render_report import _thinking_type, build_artifact, eval_model_label, write_report  # noqa: E402
from score_results import load_json  # noqa: E402
from task_resources import cpu_count  # noqa: E402

VERDICTS = ("clean", "flagged", "rejected")
# What has to match for the shards to be one measurement of one iCode setup.
COMPARED_MODEL_PARAMS = ("model", "api_base", "provider", "reasoning_effort", "max_tokens", "max_iterations")
INPUT_PROTOCOL_KEYS = ("harbor", "benchmark", "grader_overlay", "images", "worker", "requester", "pipeline", "isolation")


def shard_report_protocol(shard: Path, builds: list[str], own_only: bool = False) -> dict | None:
    """The eval_protocol the shard's own report phase wrote on its worker; None when it did not run.

    A reused workspace can hold older runs' reports, so the shard's own build wins.
    With ``own_only`` another build's report never stands in for it.
    """
    found = []
    for path in (shard / "output").glob("*/*/artifact.json"):
        doc = load_json(path)
        if isinstance(doc, dict) and isinstance(doc.get("eval_protocol"), dict):
            own = str(doc.get("run_id") or "") in builds
            if own or not own_only:
                found.append((own, path.stat().st_mtime, str(path), doc["eval_protocol"]))
    return max(found, key=lambda item: item[:3])[3] if found else None


def read_plan(path: Path) -> list[dict]:
    """The dispatcher's ``shards/plan.txt``: one ``index|build|result|offset|count|tasks`` line per shard."""
    rows: list[dict] = []
    if not path.is_file():
        return rows
    for line in path.read_text(encoding="utf-8").splitlines():
        parts = line.strip().split("|")
        if len(parts) != 6:
            continue
        index, build, result, offset, count, tasks = parts
        try:
            rows.append(
                {
                    "shard": int(index),
                    "build": str(int(build)),
                    "result": result.strip() if result.strip() not in ("", "null") else "UNKNOWN",
                    "offset": int(offset),
                    "count": int(count),
                    "tasks": [t.strip() for t in tasks.split(",") if t.strip()],
                }
            )
        except ValueError:
            continue
    return rows


def built_by(inputs: dict, build: str) -> bool:
    """Whether Jenkins build ``build`` wrote this ``eval_protocol_inputs.json``."""
    requester = inputs.get("requester") if isinstance(inputs.get("requester"), dict) else {}
    return str(requester.get("build_url") or "").rstrip("/").endswith(f"/{build}")


def planned_shards(shards: list[Path], plan: list[dict]) -> list[tuple[int, Path | None, dict | None]]:
    """``(index, eval-runs dir or None, plan line)`` per shard; without a plan, every dir found."""
    if not plan:
        return [(index, shard, None) for index, shard in enumerate(shards, start=1)]
    by_build = {shard.parent.name: shard for shard in shards}
    out = [(entry["shard"], by_build.pop(entry["build"], None), entry) for entry in plan]
    for name in sorted(by_build):
        print(f"WARNING: {by_build[name]} is not in the dispatcher's plan; ignored", file=sys.stderr)
    return out


def inputs_protocol(inputs: dict) -> dict:
    """A protocol from a shard's raw inputs alone. A value the shard did not record stays None."""
    out = {key: inputs[key] for key in INPUT_PROTOCOL_KEYS if key in inputs}
    effort = inputs.get("reasoning_effort")
    out["model_params"] = {
        "model": inputs.get("model"),
        "api_base": inputs.get("api_base"),
        "provider": inputs.get("provider"),
        "reasoning_effort": effort,
        "thinking": {"type": _thinking_type(effort)} if effort else {},
        "max_tokens": inputs.get("max_tokens") or None,
        "max_iterations": inputs.get("max_iterations") or None,
        "n_rollouts": inputs.get("n_rollouts"),
    }
    out["resources"] = {
        "cpu_lock_qty": inputs.get("cpu_lock_qty"),
        "concurrency": inputs.get("concurrency"),
        "cpus_each": inputs.get("cpus_each"),
    }
    return out


def egress_probe_of(protocol: dict) -> dict | None:
    isolation = protocol.get("isolation") if isinstance(protocol.get("isolation"), dict) else {}
    probe = isolation.get("egress_probe")
    return probe if isinstance(probe, dict) and probe.get("image") else None


def shard_row(index: int, builds: list[str], protocol: dict) -> dict:
    icode = protocol.get("icode") if isinstance(protocol.get("icode"), dict) else {}
    row = {
        "shard": index,
        "builds": builds,
        "worker": protocol.get("worker") if isinstance(protocol.get("worker"), dict) else {},
        "resources": protocol.get("resources") if isinstance(protocol.get("resources"), dict) else {},
        "model_params": protocol.get("model_params") if isinstance(protocol.get("model_params"), dict) else {},
        "icode_version": icode.get("version") or None,
    }
    probe = egress_probe_of(protocol)
    if probe is not None:
        row["egress_probe"] = {"image": str(probe.get("image")), "substituted": probe.get("substituted") is True}
    return row


def merge_protocols(protocols: list[dict]) -> dict:
    """The first shard's protocol, with every shard's per-task image and benchmark pins.

    When the shards' egress probes differ, the merged protocol names none of them;
    each shard's row keeps its own.
    """
    if not protocols:
        return {}
    out = json.loads(json.dumps(protocols[0]))
    probes = {
        (str(p.get("image")), p.get("substituted") is True)
        for p in (egress_probe_of(protocol) for protocol in protocols)
        if p is not None
    }
    if len(probes) > 1 and isinstance(out.get("isolation"), dict):
        out["isolation"].pop("egress_probe", None)
    images = out.get("images") if isinstance(out.get("images"), dict) else {}
    bench = out.get("benchmark") if isinstance(out.get("benchmark"), dict) else None
    pins = bench.get("tasks") if bench is not None and isinstance(bench.get("tasks"), dict) else None
    for other in protocols[1:]:
        if isinstance(other.get("images"), dict):
            images.update({k: v for k, v in other["images"].items() if k not in images})
        other_bench = other.get("benchmark") if isinstance(other.get("benchmark"), dict) else {}
        if pins is not None and isinstance(other_bench.get("tasks"), dict):
            pins.update({k: v for k, v in other_bench["tasks"].items() if k not in pins})
    if images:
        out["images"] = images
    return out


def shard_mismatches(rows: list[dict]) -> dict:
    """Model params and iCode versions that differ between shards, by key.

    A value a shard did not record (it stopped early) is unknown, not different.
    """
    found: dict[str, list] = {}
    for key in COMPARED_MODEL_PARAMS:
        values = {json.dumps(row["model_params"].get(key)) for row in rows if row["model_params"].get(key) is not None}
        if len(values) > 1:
            found[key] = sorted(json.loads(v) for v in values)
    versions = {row.get("icode_version") for row in rows if row.get("icode_version")}
    if len(versions) > 1:
        found["icode_version"] = sorted(str(v) for v in versions)
    return found


def shard_dirs(root: Path) -> list[Path]:
    """Every ``eval-runs`` directory under the copied artifacts, in a stable order.

    copyArtifacts keeps each build's paths, so the layout is
    ``shards/<build>/eval-runs/...`` — but a hand-assembled ``shards/eval-runs``
    has to work too. A shard that stopped before Harbor has no ``harness`` but
    may still hold its inputs.
    """
    marks = ("harness", "eval_protocol_inputs.json", "selected_tasks.txt")
    found = sorted({p.resolve() for p in root.rglob("eval-runs") if any((p / m).exists() for m in marks)})
    if not found and (root / "harness").is_dir():
        found = [root]
    return found


def merge_harness(shards: list[Path], dest: Path, plan: list[dict] | None = None) -> dict:
    """Copy every shard's trials and anti-cheat verdicts under one harness dir.

    Shard directories are named per build, so trials cannot collide. A task that
    somehow appears in two shards keeps both, and the discovery contract in
    ``score_results`` then takes the newest build that ran it.

    With a ``plan``, a shard's inputs, selection and resources count only when
    its own build wrote them, its verdicts only when its own build has trials,
    and a shard without trials keeps a row naming the questions it did not run.
    """
    harness = dest / "harness"
    (harness / "harbor_runs").mkdir(parents=True, exist_ok=True)
    (harness / "anticheat").mkdir(parents=True, exist_ok=True)
    tasks: list[str] = []
    summaries: list[dict] = []
    pipelines: list[dict] = []
    protocols: list[dict] = []
    rows: list[dict] = []
    first_inputs: dict | None = None
    resources: dict | None = None
    for index, shard, entry in planned_shards(shards, plan or []):
        build = entry["build"] if entry else None
        own_run = f"jenkins-{build}" if build else None
        src = shard / "harness" / "harbor_runs" if shard else None
        builds = sorted(p for p in src.iterdir() if p.is_dir()) if src and src.is_dir() else []
        for run in builds:
            target = harness / "harbor_runs" / f"shard{index}-{run.name}"
            if target.exists():
                shutil.rmtree(target)
            shutil.copytree(run, target, symlinks=True)
        names = [run.name for run in builds]
        ran = own_run in names if own_run else bool(names)
        got = load_json(shard / "eval_protocol_inputs.json") if shard else None
        inputs = got if isinstance(got, dict) else {}
        trusted = shard is not None and (build is None or built_by(inputs, build))
        if shard is not None and inputs and not trusted:
            requester = inputs.get("requester") if isinstance(inputs.get("requester"), dict) else {}
            print(
                f"WARNING: shard {index} #{build} holds files from {requester.get('build_url') or 'another build'}; ignored",
                file=sys.stderr,
            )
        if not trusted:
            inputs = {}
        if shard is not None and (ran or build is None):
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
        shard_tasks: list[str] = []
        selected = shard / "selected_tasks.txt" if shard else None
        if trusted and selected is not None and selected.is_file():
            shard_tasks = [ln.strip() for ln in selected.read_text(encoding="utf-8").splitlines() if ln.strip()]
        elif entry is not None:
            shard_tasks = list(entry["tasks"])
        for tid in shard_tasks:
            if tid not in tasks:
                tasks.append(tid)
        if inputs and first_inputs is None:
            first_inputs = inputs
        if isinstance(inputs.get("pipeline"), dict):
            pipelines.append(inputs["pipeline"])
        protocol = None
        if trusted:
            protocol = shard_report_protocol(shard, names, own_only=build is not None) or (
                inputs_protocol(inputs) if inputs else None
            )
        if protocol is not None:
            protocols.append(protocol)
        row_builds = [own_run] if own_run else names
        if protocol is not None or entry is not None:
            row = shard_row(index, row_builds, protocol or {})
            if entry is not None:
                row["result"] = entry["result"]
                if not ran:
                    if shard_tasks:
                        row["not_run"] = shard_tasks
                    elif entry["count"]:
                        row["not_run_count"] = entry["count"]
                        row["not_run_offset"] = entry["offset"]
            rows.append(row)
        if resources is None and trusted:
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
        "protocol": merge_protocols(protocols),
        "inputs": first_inputs or {},
        "shard_rows": rows,
        "resources": resources,
        "anticheat": merged,
        "pipelines": pipelines,
        "shards": len(plan) if plan else len(shards),
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
    ap.add_argument("--plan", default="", help="the dispatcher's shards/plan.txt (index|build|result|offset|count|tasks)")
    ap.add_argument("--build-url", default="", help="the dispatcher build's URL, the merged report's requester")
    ap.add_argument("--out", required=True)
    args = ap.parse_args()

    root = Path(args.shards)
    shards = shard_dirs(root)
    plan = read_plan(Path(args.plan)) if args.plan else []
    if not shards and not plan:
        print(
            f"ERROR: no shard archives under {root}. The shards archive eval-runs/ only when "
            "RUN_GROUP is set, so check that the dispatcher passed it.",
            file=sys.stderr,
        )
        return 1

    out = Path(args.out)
    out.mkdir(parents=True, exist_ok=True)
    merged = merge_harness(shards, out, plan)
    tasks = merged["tasks"]
    if not tasks:
        print(f"ERROR: shards under {root} name no tasks (no selected_tasks.txt)", file=sys.stderr)
        return 1

    protocol = merged["protocol"] or {}
    inputs = merged["inputs"]
    params = protocol.get("model_params") if isinstance(protocol.get("model_params"), dict) else {}
    rows = merged["shard_rows"]
    for row in rows:
        if row.get("not_run"):
            print(
                f"WARNING: shard {row['shard']} {row['builds'][0]} ({row.get('result')}) ran no trials; "
                f"{len(row['not_run'])} questions count as missing",
                file=sys.stderr,
            )
        elif row.get("not_run_count"):
            print(
                f"WARNING: shard {row['shard']} {row['builds'][0]} ({row.get('result')}) ran no trials; "
                f"its {row['not_run_count']} questions at offset {row.get('not_run_offset')} are not named, "
                "so the report does not count them",
                file=sys.stderr,
            )
    if isinstance(protocol.get("images"), dict):
        # A shard on an older pipeline still lists its workspace's earlier tasks.
        protocol["images"] = {tid: image for tid, image in protocol["images"].items() if tid in tasks}
    if args.build_url:
        requester = protocol.get("requester") if isinstance(protocol.get("requester"), dict) else {}
        protocol["requester"] = {**requester, "build_url": args.build_url}
    if rows:
        protocol["shards"] = rows
        mismatched = shard_mismatches(rows)
        protocol["model_params_status"] = "mixed" if mismatched else "same"
        if mismatched:
            protocol["model_params_mismatch"] = mismatched
            detail = "; ".join(f"{key} {values}" for key, values in mismatched.items())
            print(
                f"WARNING: shards ran different model params or iCode ({detail}). "
                "This report is not one measurement of one iCode setup.",
                file=sys.stderr,
            )
    applied = (merged["resources"] or {}).get("applied") or {}
    model = str(params.get("model") or inputs.get("model") or "")
    doc = build_artifact(
        suite=args.benchmark,
        model=eval_model_label(model) if model else "",
        api_base=str(params.get("api_base") or inputs.get("api_base") or ""),
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
