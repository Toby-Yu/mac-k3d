# Branch process: `int/PF-harbor-delegation`

This folder is **branch process and re-test notes**, not the user start-here. User docs stay under [docs/](../) ([evaluation](../evaluation.md), [architecture](../architecture.md), [workflow](../workflow.md), [user-guide](../user-guide.md)).

Read this one first if you are my mentor: the [responsibility split](#the-responsibility-split-what-mac-k3d-does-and-what-harbor-does) and the [open questions](#open-questions-for-my-mentor) are the two sections written for you.

## What changed on this branch

1. **Harbor schedules the rollouts, not mac-k3d.** The old harness stage used to loop over questions, build one `harbor run` per rollout, keep an in-flight table and a heartbeat scheduler, and wait on `wait -n`. It is now a single `harbor run -p <dataset> -i <id> ... -k <rollouts> -n <slots> -r <retries>`. Harbor expands `n_attempts x tasks x agents` into trials itself and runs `-n` of them at a time.
2. **Declared resources are honored instead of overridden.** `pipeline/lib/task_resources.py` reads `cpus` / `memory_mb` / `storage_mb` out of each `task.toml`. The pipeline no longer passes `--override-cpus` / `--override-memory-mb` on the normal path, so a DeepSWE task gets the 2 CPUs and 8 GB it asks for and a LoLBench task gets its 4 CPUs. The build fails before starting if this worker cannot satisfy one task.
3. **One build per worker, every core.** A registered node has one executor, and the Jenkinsfile locks every core of its own node (`lock(label: env.NODE_NAME)` with no quantity), so a build on worker A can never hold tokens that belong to worker B and Harbor's `-n` fills the whole machine. An earlier draft set executors to the core count; Jenkins' default load balancer hashes the job name to one preferred node, so every shard piled onto the same worker while it had a free executor.
4. **Nine jobs, three per suite.** `<suite>_one_task` (one question, one rollout by default — the smoke test, and the shard runner), `<suite>_some_task` (a `TASKS` list or the first `N_TASKS` at the full rollout count — the mentor-PDF comparison arm), `<suite>_full_suite_task` (the whole suite). For `deepswe`, `lolbench` and `swebenchpro`. There is no separate aggregate job.
5. **Sharded dispatch with work stealing.** `some_task` and `full_suite_task` run on `agent none`, split their questions into contiguous shards of `SHARD_SIZE` (default 2, at least one per online worker), queue every shard as a `<suite>_one_task` build, and let each worker take the next shard when it frees up. The same dispatcher build then merges the shards in an `Aggregate` stage, copying each shard's trials by build number.
6. **Lock held only for the rollouts.** The Jenkinsfile has one stage per pipeline phase (item 12), driven by `MAC_K3D_PHASE`, and only `Evaluate` is locked. Benchmark checkout, image builds and report rendering no longer sit on cores other builds are waiting for.
7. **The pipeline under test ships in the worker's binary and names its commit.** `build.rs` bakes the commit, a dirty flag and a hash of `pipeline/` into `mac-k3d`. Each build extracts that pipeline once (`mac-k3d pipeline --extract-to`) and `artifact.json` records `eval_protocol.pipeline.commit`. `scripts/redeploy.sh` moves a pushed commit to the controller and every worker in one command; an aggregate whose shards ran different pipelines is marked `pipeline_status: mixed`. An earlier version of this branch cloned mac-k3d from GitHub at a `MAC_K3D_GIT_REF` per build; that added three parameters and a push-then-type-the-branch step to every iteration and is gone. `eval --sync-pipeline` and the `MAC_K3D_ROOT` / `~/.local/share` fallback chain are gone too.
8. **Empty-patch P2P no longer counts as a regression.** A trial that produced a zero-byte patch is dropped from the macro P2P average rather than averaged in as 0/N; the verifier only reported 0 because nothing applied. `Pass@1` in the ladder is now labelled "first rollout only" so it is not confused with Macro Pass@1.
9. **Short parameter lists for users.** Each job shows `HARNESS`, `LLM` and `BENCHMARK` first (fixed, so every build names what it evaluated) and only the inputs its shape needs. Pins, canary and `SHARD_SIZE` are developer-only: `jenkins_job.ui_profile: user` (the default) turns them into hidden parameters with config defaults via the Hidden Parameter plugin, which `mac-k3d start` installs. `CPU_LOCK_QTY`, `SHARDS` and `RESUME` are gone.
10. **A redeploy no longer drops a worker.** Worker `config` used to `curl -o agent.jar` over the jar the running agent had open (`NoClassDefFoundError: hudson/remoting/Channel$OrderlyShutdown` after a remoting bump) and then left the old JVM running. It now downloads beside the jar, swaps it in by rename only when the bytes differ, and restarts the agent only when the jar, `launch-agent.sh` or the unit changed.
11. **Fractional CPU declarations.** DeepSWE's `task.toml` writes `cpus = 2.0`. Build #51 died in the report stage on `--cpus-each 2.0` (`invalid int value`). `task_resources.cpu_count` now keeps whole counts as ints (`2.0` → `2`) and fractions as floats, and every reader (`parallel_degree.sh`, `render_report.py`, `aggregate_runs.py`, `provenance.py`) goes through it.
12. **One phase, one job.** The nine `p0`…`p8` scripts are replaced by seven phases — `env` (bare-metal checks), `tasks` (per-task setup and checks), `evaluate` (slots, canary, one `harbor run`), `anticheat`, `score`, `report`, `archive` — each an ordered list of single-purpose steps under `pipeline/stages/<phase>/`. Steps pass state only through files in `$WORKDIR`. `evaluate/harbor_cmd.sh` is the only file that builds a Harbor command line, so every flag is in one place ([pipeline.md](../pipeline.md#how-harbor-is-called)). The cost/token report now runs in `archive`, and the backup moved out of `render_report.py` into `archive_run.py`. Manual scripts (canary only, baseline, report check, fixtures) moved to `pipeline/tools/`. `mac-k3d eval --stage` takes phase names and answers an old `pN` with the phase that replaced it. The unused Pier adapter is deleted.
13. **Pinned toolchain and a versioned allowlist.** `pipeline/config/toolchain.env` pins Harbor (`0.22.0`), compose and buildx; `mac-k3d setup` and the `env` phase install the same Harbor, and the `env` phase replaces any other version. The agent's `--allow-agent-host` list moved out of the script into `pipeline/config/network-allowlist-v1.json`; its version, hosts and hash go into `eval_protocol.isolation.network_allowlist`, and a test keeps the canary's model hosts equal to it.
14. **Setup and config are modular, and `worker.yaml` always shows its API fields.** The wizard only asks questions (`src/prepare/wizard/`), saves the YAML, then `src/prepare/apply.rs` and `root_steps.rs` do the work, and `config` is split per role (`src/commands/config/`). A worker's `jenkins.api_user` and `api_token` are always written, empty when skipped, so the user can fill them in with an editor later. `config -c <missing file>` now says the file is missing instead of failing on a k3d check.

## The responsibility split: what mac-k3d does and what Harbor does

This is the part you asked me to get right, so here it is explicitly.

| | Owner | Why |
| --- | --- | --- |
| Bare-machine prep (Docker, k3d, pinned Harbor, Jenkins agent, images), re-checked per build by the `env` phase | mac-k3d | Harbor assumes a working Docker host; something has to build one. |
| Per-build prep and checks (`tasks` phase: benchmark checkout, task selection, iCode checkout and sandbox, read-only mount, network allowlist, leak scan) | mac-k3d | These are the inputs to Harbor, not Harbor's job. |
| Container lifecycle, per-task CPU/memory limits, retries, rollout fan-out, verifier execution, reward | **Harbor** | This was the 700 lines of mac-k3d I deleted. Harbor already does it. |
| Job placement across workers, resource admission | Jenkins + lockable-resources | Jenkins is the scheduler; mac-k3d only registers nodes and core tokens. |
| Isolation canary, patch-capture receipts, anti-cheat verdicts, score, report, archive | mac-k3d (`evaluate/canary`, then the `anticheat`, `score`, `report`, `archive` phases) | See below — Harbor has no equivalent. |

**Harbor has no contamination gate.** I checked the installed 0.22.0 rather than assuming: `harbor job summarize` is a removed shim that only prints a deprecation notice, and `harbor analyze` is rubric-only (it asks an LLM to judge a transcript against a rubric). Neither inspects a transcript for solution leakage, reads the gold patch, or can reject a trial. So the integrity layer stays in mac-k3d:

- `icode_capture.sh` writes a receipt (`capture.json`) per trial: repo, base SHA, patch bytes and SHA256.
- The `anticheat` phase cross-checks each receipt against the grader's patch file and the task's declared base commit, and flags `capture_mismatch`, `capture_missing`, `patch_oversize` or `base_commit_mismatch`.
- The anti-cheat pass (`mac-k3d-anticheat-v1`) and the sanitizer run on the transcripts and can mark a trial `flagged` or `rejected`.
- The canary question runs before the real rollouts to prove isolation on the exact `-p` / `-i` path the rollouts will take.

So "Harbor as a black box that returns results" is now true for *scheduling and grading*, which is what it is good at. It is not true for *trusting* the results, and that gap is mine to cover.

## How scalability is guaranteed

Adding a worker is one command: `mac-k3d setup -c worker.yaml`. That registers the Jenkins node and creates its `<agent>-core-N` lockable resources, each labelled with both the shared label and the agent name. Nothing else changes:

- The dispatcher counts online workers when it plans shards (at least one shard each), so a new worker gets work automatically.
- Each worker has one executor, so Jenkins hands the next queued shard to whichever worker frees up first; a bigger or faster worker runs more shards. No node list is hardcoded in the pipeline.
- `lock(label: env.NODE_NAME)` resolves per build, so the Nth worker needs no pipeline edit.

**Keep exactly one controller.** Lockable-resources state is per-controller, so a second controller would hand out tokens for cores the first controller already lent out. Scale workers, not controllers.

## Open questions for my mentor

1. **Should an official run honor declared CPUs, or pin them?** Right now it honors `task.toml` (2 CPUs for DeepSWE, 4 for LoLBench). That is faithful to the benchmark, but it means two workers with different core counts run the same question at the same per-task limit yet at different concurrency. If you want bit-comparable timing across machines, I should pin `-n` instead of deriving it.
2. **Compare raw or gated numbers against your PDF?** `artifact.json` carries both: `icode_raw` (every trial, no anti-cheat exclusions) and `icode` (post-gate). Your PDF predates the gate, so `icode_raw` is the apples-to-apples number — but `icode` is the one I would publish. Confirm which I should put in the comparison table.
3. **Is first-rollout Pass@1 or Macro Pass@1 the headline?** They differ whenever a question is inconsistent across rollouts. I report both; tell me which one you consider "the" score.

## How to re-test

Step-by-step log: [testing.md](testing.md). The cloud runbook is [../testing/cloud-eval-runbook.md](../testing/cloud-eval-runbook.md).
