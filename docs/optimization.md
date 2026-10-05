# Eval resource packing

Harbor applies each task's own `cpus` / `memory_mb` / `storage_mb`. mac-k3d's only job is to decide **how many** of those fit on this worker and to refuse a build that cannot run even one. Field meanings (`concurrency`, `cpus_each`, `tokens.total`) stay in [evaluation.md](evaluation.md).

## Declared, not overridden

`pipeline/lib/task_resources.py` reads the `[environment]` and `[verifier.environment]` blocks of every selected `task.toml` and takes the maximum across them, because a verifier can ask for more than the agent phase:

| Suite | `cpus` | `memory_mb` | `storage_mb` |
|---|---|---|---|
| DeepSWE | 2 | 8192 | 20480 |
| LoLBench | 4 | 7168–8192 | — |

The pipeline passes **no** `--override-cpus` and **no** `--override-memory-mb` on the normal path. A task gets what it asks for, so a score is comparable with anyone else's run of the same task.

## How many fit

```text
EVAL_SLOTS = max(1, min(CPU_LOCK_QTY / declared_cpus, mem_slots, disk_floor))
mem_slots  = floor((MemAvailable_GB - HOST_RESERVE_GB) / declared_memory_GB)
disk_floor = 1 when free disk at WORKDIR < MIN_DISK_GB, else unbounded
```

`HOST_RESERVE_GB` is 2 and `MIN_DISK_GB` is 40. `EVAL_SLOTS` becomes Harbor's `-n`, so Harbor keeps that many trials in flight and queues the rest itself.

Admission and throughput use different numbers on purpose:

- **Admission** (can this worker run one task at all?) uses `MemTotal - reserve`. A worker with enough RAM but a full page cache should still be allowed to start.
- **Throughput** (how many at once?) uses `MemAvailable - reserve`. Packing against total RAM would thrash.

If one declared task does not fit under either test, the build **fails before the first container starts** with the shortfall printed. That is deliberate: a half-resourced run produces numbers nobody can use.

After the question list is chosen, P5 prints the plan and `artifact.json` keeps it:

```json
"resources": {
  "declared": {"peak": {"cpus": 2, "memory_mb": 8192, "storage_mb": 20480}},
  "applied":  {"slots": 2, "cpus_each": 2}
}
```

Inspect a plan without running anything:

```bash
python3 pipeline/lib/task_resources.py plan \
  --tasks-dir eval-runs/deep-swe/tasks \
  --selected eval-runs/selected_tasks.txt \
  --cpu 8 --n-rollouts 4 --workdir eval-runs
```

A local stage run that leaves `CPU_LOCK_QTY` unset uses 1. The Jenkins parameter defaults to 4. Set `EVAL_RESOURCE_CAP=0` only for fixture tests that must ignore RAM.

Later stages do not re-derive any of this: `eval_parallel_degree` reads the `eval_resources.json` that P5 wrote, so the report describes the run that happened rather than a fresh guess about the current machine.

## Memory trace

A single `harbor run` has no one "active question", so the memory trace is per build rather than per question. P5's heartbeat appends a `docker stats` snapshot to `harness/container_mem.jsonl` for containers whose names contain `_icode_` or `__env-main-`, ignoring sidecars. The largest `peak_gb` is kept in `harness/container_mem_peak_gb`, and the next build feeds it back in as `--measured-peak-gb` so a worker learns its real ceiling over time.

```bash
python3 pipeline/lib/task_resources.py sample --harness-dir eval-runs/harness --slots 2
```

## Scaling out instead of up

Packing more trials onto one worker has a ceiling; adding workers does not. The real lever for a full suite is `SHARDS` on `<suite>_full_suite_task`, which splits the suite across nodes and merges the results — see [evaluation.md](evaluation.md#which-job-to-run). Within one build, the useful knob is `CPU_LOCK_QTY`, which raises the core budget this build reserves on its node.

Time and token cost are reduced by `ICODE_REASONING_EFFORT=high` (less explore soft-stop), not by cutting rollouts.

## Retries

Harbor's `-r` (max retries) handles a trial that dies on infrastructure, so mac-k3d has no resume mode of its own. To re-run a suite that aborted, trigger the dispatcher again: the shards that already finished are archived under their `RUN_GROUP`, and `eval_aggregate` reports on whatever is present (`propagate: false`), so a partial suite still yields a report.

## Disk and I/O

Free disk at `WORKDIR` below 40 GB forces one slot. Harbor keeps one jobs tree per run so rollouts of the same task reuse image layers. `PYTHONDONTWRITEBYTECODE=1`. The heartbeat does not scan huge logs.

## Not done

No cgroup I/O weights and no Harbor image-build changes.
