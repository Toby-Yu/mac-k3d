# Eval resource packing

Jenkins `CPU_LOCK_QTY` is the `CPU_CORES` lock **and** the Harbor slot pool. Field meanings (`concurrency`, `cpus_each`, `tokens.total`) stay in [evaluation.md](evaluation.md).

## CPU

Each live unit is one question × one rollout: `harbor run -n 1 -k 1`. At most `EVAL_SLOTS` units run at once.

```text
EVAL_SLOTS = CPU_LOCK_QTY
EVAL_CPUS_EACH = max(1, CPU_LOCK_QTY / EVAL_SLOTS)
```

P5 still samples `docker stats` for containers whose names contain `_icode_` or `__env-main-` and writes `peak_gb` to `container_mem.jsonl`. That file is emptied at the start of P5, so the report copy contains only this build. The same row, plus the build number, is appended to `harness/container_mem_history.jsonl` on the worker and is not copied into the report. That sample does not change the slot count and does not set a Docker memory cap. Free disk at `WORKDIR` below 40 GB sets `EVAL_SLOTS` to 1.

After the question list is chosen, P5 prints:

```text
P5 parallel: questions=4 ids=a,b,c,d rollouts=4 mem_available_gb=9.8 container_gb=0.40 budget_gb=cpu_lock slots=4 parallel_containers=4 four_containers=yes
```

`container_gb=unmeasured` means no eval container has been sampled yet. `four_containers=yes` means `CPU_LOCK_QTY` is at least 4. The first wave is every rollout of the first question. A list longer than eight ids prints the first and last id and points at `selected_tasks.txt`.

Fill order: one question's rollouts run together. A free slot waits while that question still has a container running. The next question starts only after those containers exit.

Harbor does not receive `--override-memory-mb`. If an attempt's log shows an out-of-memory kill, P5 appends that question to `harness/skipped_questions.txt`, does not launch its remaining rollouts, and continues. Reward files already written stay on disk and P8 scores them.

Examples with `CPU_LOCK_QTY=4` and `N_ROLLOUTS=4`:

| Questions | What runs |
|-----------|-----------|
| 113 | Attempts 1–4 of one question; the next question starts after those four exit |
| 3 | The same wave, three times |
| 1 | Four rollouts of that question |

P6 grading uses the same `EVAL_SLOTS` cap. A local stage run that leaves `CPU_LOCK_QTY` unset uses 1. The Jenkins parameter defaults to 4.

`--override-cpus` is `EVAL_CPUS_EACH`. With `CPU_LOCK_QTY=4` that is one core per container.

## Disk and I/O

Free disk at `WORKDIR` below 40 GB sets `EVAL_SLOTS` to 1. One jobs tree per question so extra rollouts reuse image layers. `PYTHONDONTWRITEBYTECODE=1`. The heartbeat does not scan huge logs.

## Not done

No cgroup I/O weights and no Harbor image-build changes.
