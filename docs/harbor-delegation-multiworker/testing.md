# Re-test log (`int/PF-harbor-delegation`)

Not a second user guide. Field meanings: [evaluation](../evaluation.md). Job parameters: [commands](../commands.md). Unrun question ids: [question-coverage](../testing/question-coverage.md).

## Fixtures (no Jenkins)

```bash
cargo test
python3 -m unittest discover -s pipeline/lib -p 'test_*.py'
bash -n pipeline/stages/run_all.sh pipeline/stages/p5_harness.sh pipeline/stages/_common.sh
scripts/check_docs.sh
```

The cases that cover this branch:

- `every_suite_gets_three_shapes_plus_one_aggregator` / `shapes_differ_only_in_their_defaults` — nine eval jobs plus one shared `eval_aggregate`; `one_task` pins 1 question x 1 rollout and `some_task` takes the configured counts, and nothing else differs between them
- `full_suite_dispatches_shards_then_aggregates` — the dispatcher takes no node, no lock and no Docker; it triggers `<suite>_some_task` in parallel with a `TASK_OFFSET` per shard, then `eval_aggregate`
- `aggregate_collects_by_run_group` — the aggregator pulls shards by `RUN_GROUP` and needs no iCode, Docker or lock
- `lolbench_one_task_uses_shared_pipeline` — the pipeline is a clone at `MAC_K3D_GIT_REF`; no `--sync-pipeline`, no `MAC_K3D_ROOT`, no `~/.local/share`; `OFFICIAL=1` requires `kind=commit`
- `job_xml_locks_this_node_only_for_the_rollouts` (in `lolbench_one_task_uses_shared_pipeline`) — `lock(label: env.NODE_NAME)` sits inside `Evaluate`, between `Prepare` and `Report`
- `test_p5_uses_import_path_not_bare_agent_icode` / `test_shared_harbor_command_for_every_benchmark` — exactly one `harbor run` per build, carrying `-k $N_ROLLOUTS` and `-n $EVAL_SLOTS`
- `pipeline/lib/test_task_resources.py` (17 cases) — declared cpus/memory/storage are read from `task.toml`, slots are planned from them, and a worker that cannot fit one task fails early
- `pipeline/lib/test_aggregate.py` (12 cases) — two shards' trials merge into one artifact with `tasks x rollouts` records; verdict counts add; mixed pipeline versions are flagged, not averaged; an empty patch keeps its F2P but drops its P2P
- `test_a_trial_that_started_off_the_declared_base_is_flagged` — P7 compares each receipt's base SHA against `task.toml` on the host, since the agent env no longer carries it
- `test_parallel_degree_does_not_guess_when_p5_never_ran` — the report stages read the plan P5 wrote instead of re-probing the machine

## Live Jenkins: the URL + ref dev loop

This replaces the old "sync the pipeline from a local path" loop. Every test now names a commit, so a bad change is revertible and a good result is reproducible.

1. Commit and push to the branch:

   ```bash
   git commit -am "fix: <what>"
   git push origin int/PF-harbor-delegation
   ```

2. Open `deepswe_one_task` -> **Build with Parameters**.
3. Leave `MAC_K3D_GIT_URL` at `https://github.com/Toby-Yu/mac-k3d.git`. Set `MAC_K3D_GIT_REF` to `int/PF-harbor-delegation` and `MAC_K3D_GIT_REF_KIND` to `branch`. The build clones that ref and prints the resolved SHA.
4. To re-run an exact past build, set `MAC_K3D_GIT_REF_KIND=commit` and paste the SHA from that build's `artifact.json` (`pipeline.commit`). An `OFFICIAL=1` run *requires* `kind=commit`, so an official number can never come from a moving branch.

Console signals, in order:

- `mac-k3d pipeline <sha>` in the Prepare stage — the SHA actually under test
- `PROGRESS 40% P5 canary + the Harbor run (holding $CPU_LOCK_QTY cores on $NODE_NAME)` — the lock is per node
- one `harbor run` line, with `-k` equal to `N_ROLLOUTS` and `-n` equal to the planned slots
- `declared cpus=2 memory_mb=8192` (DeepSWE) or `cpus=4` (LoLBench) and **no** `--override-cpus`
- after P8, `artifact.json` has `pipeline.url` / `pipeline.ref` / `pipeline.commit` and `resources.declared` vs `resources.applied`

## Live Jenkins: the two-worker proof

The run that has to pass before anyone else uses this.

1. On the **controller** only: put this binary on the machine and run `mac-k3d start -c ~/.config/mac-k3d/config.yaml`. That Helm-upgrades Jenkins (`copyartifact`, `pipeline-utility-steps`) and rewrites the ten jobs. Do not install plugins in the UI. Workers stay on `config` only — never `start -c worker.yaml`.
2. Both workers registered (`mac-k3d setup -c worker.yaml` on each) and online in **Manage Jenkins -> Nodes**.
3. Open `deepswe_full_suite_task` -> **Build with Parameters**. Set `N_TASKS=10` and `SHARDS=4`.
4. While it runs, check:
   - **Lockable Resources** shows held tokens named `<agent>-core-N` for *both* node names, and no build holding a token whose prefix is not its own node
   - both workers show a running `deepswe_some_task` build at the same time
   - each shard's console prints a different `TASK_OFFSET`, and the four `selected_tasks.txt` files are disjoint
5. When `eval_aggregate` finishes, open its `aggregate/artifact.json` and confirm `shards: 4`, `n_tasks: 10`, and that the rollout records total `10 x N_ROLLOUTS`.
6. Record the build numbers and the outcome as a new row in [../integration-log.md](../integration-log.md).

## Status

Fixtures: **green** (107 Rust, 214 Python). Live two-worker proof: **not yet run** — it needs the second worker online, so it is the one item on this branch I have not verified myself.
