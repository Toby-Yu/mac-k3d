# Eval resource packing

Jenkins `CPU_LOCK_QTY` is the `CPU_CORES` lock **and** the upper bound on the Harbor slot pool. Field meanings (`concurrency`, `cpus_each`, `tokens.total`) stay in [evaluation.md](evaluation.md).

## CPU and RAM

Each live unit is one question × one rollout: `harbor run -n 1 -k 1`. At most `EVAL_SLOTS` units run at once.

```text
EVAL_SLOTS = min(CPU_LOCK_QTY, RAM_SLOTS, disk_floor)
RAM_SLOTS  = floor((MemAvailable_GB - 2) / per_container_GB)
per_container_GB = history_peak(question) × 1.5
                 (else live sample; else fallback: DeepSWE 1.5, LoLBench 4.0, SWE-bench Pro 8)
EVAL_CPUS_EACH = max(1, CPU_LOCK_QTY / EVAL_SLOTS)
EVAL_MEMORY_MB = min(ceil(per_container_GB × 1024), floor((MemAvailable_MB - 2048) / EVAL_SLOTS))
```

Slots and the Docker mem cap are **per question**: light DeepSWE / LoLBench ids pack up to `CPU_LOCK_QTY` containers; heavy Flink history peaks shrink to 1 slot with a tight host-capped `--override-memory-mb`. A heavy prior wave does not pin later light questions (global noted peak is cleared on flush; planning prefers that question’s history).

P5 samples `docker stats` for containers whose names contain `_icode_` or `__env-main-` and writes `peak_gb` to `container_mem.jsonl`. Durable peaks live in `harness/container_mem_history.jsonl` (not archived) and drive the next wave of the same question. Free disk at `WORKDIR` below 40 GB sets `EVAL_SLOTS` to 1.

Harbor gets `--override-memory-mb` so a heavy container is OOM-killed **inside Docker** (P5 then skips that question) instead of the kernel OOM-killing the Jenkins agent and aborting the whole suite (`exit -1` / durable-task death).

After the question list is chosen, P5 prints:

```text
P5 parallel: questions=4 ids=a,b,c,d rollouts=4 mem_available_gb=9.8 container_gb=0.40 budget_gb=0.60 slots=4 parallel_containers=4 four_containers=yes
```

`container_gb=unmeasured` means no live/history sample yet; LoLBench then uses the 4.0 GB fallback until a peak exists. `four_containers=yes` means `EVAL_SLOTS` is at least 4.

Fill order: one question's rollouts run together. A free slot waits while that question still has a container running. The next question starts only after those containers exit.

If an attempt's log shows an out-of-memory kill, P5 appends that question to `harness/skipped_questions.txt`, does not launch its remaining rollouts, and continues. Reward files already written stay on disk and P8 scores them.

Examples with `CPU_LOCK_QTY=4` and `N_ROLLOUTS=4`:

| Questions | What runs |
|-----------|-----------|
| Light DeepSWE (~0.4 GB peak) | Up to 4 rollouts of one question together; Docker cap ≈ peak×1.5 |
| Heavy LoLBench Flink (~6 GB peak, ~9 GB free) | `EVAL_SLOTS` drops to 1; four rollouts run one after another under a Docker memory cap |
| 113 DeepSWE / 20 LoLBench | Same-question waves; slot count and mem cap change per question from history |

P6 grading uses the same `EVAL_SLOTS` cap. A local stage run that leaves `CPU_LOCK_QTY` unset uses 1. The Jenkins parameter defaults to 4. Set `EVAL_RESOURCE_CAP=0` only for fixture tests that must ignore RAM.

`--override-cpus` is `EVAL_CPUS_EACH`. DeepSWE `task.toml` files request `cpus = 2` and LoLBench requests `cpus = 4`; those values are not copied into `--override-cpus`.

Time and token cost are reduced by `ICODE_REASONING_EFFORT=high` (less explore soft-stop), not by cutting rollouts.

## Resume after agent death

Set Jenkins **`RESUME=true`** and keep the **same** `TASK` / `TASKS` / `N_TASKS` / `N_ROLLOUTS` as the aborted full suite (LoLBench or DeepSWE).

P5 then:

1. Keeps prior `harness/harbor_runs/**` (does not wipe other builds’ trees).
2. Seeds `ASSIGNED` from `reward.json` in **one** jobs tree: `RESUME_FROM=jenkins-N` if set, else the newest prior `jenkins-*` (not the empty new build).
3. Continues only the remaining units; P7/P8 still score Harbor dirs under `harness/harbor_runs`.

Console line: `P5 harbor: resume from jenkins-23 seeded done=N/M remaining=flink_1,...`.

Leave `RESUME` false for a clean re-run that clears this build’s `harbor_runs/jenkins-$BUILD_NUMBER`. Optional: set env `RESUME_FROM=jenkins-23` when auto-pick is wrong.

## Disk and I/O

Free disk at `WORKDIR` below 40 GB sets `EVAL_SLOTS` to 1. One jobs tree per question so extra rollouts reuse image layers. `PYTHONDONTWRITEBYTECODE=1`. The heartbeat does not scan huge logs.

## Not done

No cgroup I/O weights and no Harbor image-build changes.
