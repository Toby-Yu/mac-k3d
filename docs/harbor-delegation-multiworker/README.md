# Branch process: `int/PF-harbor-delegation`

This folder is **branch process and re-test notes**, not the user start-here. User docs stay under [docs/](../) ([evaluation](../evaluation.md), [architecture](../architecture.md), [workflow](../workflow.md), [user-guide](../user-guide.md)).

Read this one first if you are my mentor: the [responsibility split](#the-responsibility-split-what-mac-k3d-does-and-what-harbor-does) and the [open questions](#open-questions-for-my-mentor) are the two sections written for you.

## What changed on this branch

1. **Harbor schedules the rollouts, not mac-k3d.** P5 used to loop over questions, build one `harbor run` per rollout, keep an in-flight table and a heartbeat scheduler, and wait on `wait -n`. It is now a single `harbor run -p <dataset> -i <id> ... -k <rollouts> -n <slots> -r <retries>`. Harbor expands `n_attempts x tasks x agents` into trials itself and runs `-n` of them at a time.
2. **Declared resources are honored instead of overridden.** `pipeline/lib/task_resources.py` reads `cpus` / `memory_mb` / `storage_mb` out of each `task.toml`. The pipeline no longer passes `--override-cpus` / `--override-memory-mb` on the normal path, so a DeepSWE task gets the 2 CPUs and 8 GB it asks for and a LoLBench task gets its 4 CPUs. The build fails before starting if this worker cannot satisfy one task.
3. **Each worker locks its own cores.** The Jenkinsfile lock moved from the shared `label: 'CPU_CORES'` to `label: env.NODE_NAME`, so a build on worker A can no longer hold tokens that belong to worker B. `numExecutors` on a registered node went from 1 to the configured core count; the lock, not the executor count, bounds concurrency.
4. **Three job shapes per suite.** `<suite>_one_task` (one question, one rollout — the smoke test), `<suite>_some_task` (N questions at the full rollout count — the mentor-PDF comparison arm), `<suite>_full_suite_task` (a dispatcher). For `deepswe`, `lolbench` and `swebenchpro`, plus one shared `eval_aggregate`.
5. **Full suite = sharded dispatch.** `<suite>_full_suite_task` runs on `agent none`, slices the sorted question list into `SHARDS` contiguous ranges, triggers that many `<suite>_some_task` builds in parallel with a `TASK_OFFSET` each, then triggers `eval_aggregate` to merge them into one report.
6. **Lock held only for the rollouts.** The Jenkinsfile is three stages — `Prepare` (unlocked), `Evaluate` (locked), `Report` (unlocked) — driven by `MAC_K3D_PHASE`. Image pulls and report rendering no longer sit on cores other builds are waiting for.
7. **The pipeline under test is a git ref, not a local path.** `MAC_K3D_GIT_URL` + `MAC_K3D_GIT_REF` + `MAC_K3D_GIT_REF_KIND` clone mac-k3d on the worker and record the resolved SHA in `artifact.json`, the same shape the iCode inputs already used. `eval --sync-pipeline` and the `MAC_K3D_ROOT` / `~/.local/share` fallback chain are gone.
8. **Empty-patch P2P no longer counts as a regression.** A trial that produced a zero-byte patch is dropped from the macro P2P average rather than averaged in as 0/N; the verifier only reported 0 because nothing applied. `Pass@1` in the ladder is now labelled "first rollout only" so it is not confused with Macro Pass@1.

## The responsibility split: what mac-k3d does and what Harbor does

This is the part you asked me to get right, so here it is explicitly.

| | Owner | Why |
| --- | --- | --- |
| Bare-machine prep (Docker, k3d, Harbor, Jenkins agent, images) | mac-k3d | Harbor assumes a working Docker host; something has to build one. |
| Per-build prep (iCode checkout, benchmark checkout, task selection, env file) | mac-k3d | These are the inputs to Harbor, not Harbor's job. |
| Container lifecycle, per-task CPU/memory limits, retries, rollout fan-out, verifier execution, reward | **Harbor** | This was the 700 lines of mac-k3d I deleted. Harbor already does it. |
| Job placement across workers, resource admission | Jenkins + lockable-resources | Jenkins is the scheduler; mac-k3d only registers nodes and core tokens. |
| Patch-capture receipts, anti-cheat verdicts, sanitizer, canary, report | mac-k3d | See below — Harbor has no equivalent. |

**Harbor has no contamination gate.** I checked the installed 0.22.0 rather than assuming: `harbor job summarize` is a removed shim that only prints a deprecation notice, and `harbor analyze` is rubric-only (it asks an LLM to judge a transcript against a rubric). Neither inspects a transcript for solution leakage, reads the gold patch, or can reject a trial. So the integrity layer stays in mac-k3d:

- `icode_capture.sh` writes a receipt (`capture.json`) per trial: repo, base SHA, patch bytes and SHA256.
- P7 cross-checks each receipt against the grader's patch file and the task's declared base commit, and flags `capture_mismatch`, `capture_missing`, `patch_oversize` or `base_commit_mismatch`.
- The anti-cheat pass (`mac-k3d-anticheat-v1`) and the sanitizer run on the transcripts and can mark a trial `flagged` or `rejected`.
- The canary question runs before the real rollouts to prove isolation on the exact `-p` / `-i` path the rollouts will take.

So "Harbor as a black box that returns results" is now true for *scheduling and grading*, which is what it is good at. It is not true for *trusting* the results, and that gap is mine to cover.

## How scalability is guaranteed

Adding a worker is one command: `mac-k3d setup -c worker.yaml`. That registers the Jenkins node and creates its `<agent>-core-N` lockable resources, each labelled with both the shared label and the agent name. Nothing else changes:

- The dispatcher derives shard count from registered cores divided by the suite's declared per-task CPUs, so a new worker raises the ceiling automatically.
- Jenkins places each shard on whichever node has free tokens; no node list is hardcoded in the pipeline.
- `lock(label: env.NODE_NAME)` resolves per build, so the Nth worker needs no pipeline edit.

**Keep exactly one controller.** Lockable-resources state is per-controller, so a second controller would hand out tokens for cores the first controller already lent out. Scale workers, not controllers.

## Open questions for my mentor

1. **Should an official run honor declared CPUs, or pin them?** Right now it honors `task.toml` (2 CPUs for DeepSWE, 4 for LoLBench). That is faithful to the benchmark, but it means two workers with different core counts run the same question at the same per-task limit yet at different concurrency. If you want bit-comparable timing across machines, I should pin `-n` instead of deriving it.
2. **Compare raw or gated numbers against your PDF?** `artifact.json` carries both: `icode_raw` (every trial, no anti-cheat exclusions) and `icode` (post-gate). Your PDF predates the gate, so `icode_raw` is the apples-to-apples number — but `icode` is the one I would publish. Confirm which I should put in the comparison table.
3. **Is first-rollout Pass@1 or Macro Pass@1 the headline?** They differ whenever a question is inconsistent across rollouts. I report both; tell me which one you consider "the" score.

## How to re-test

Step-by-step log: [testing.md](testing.md). The cloud runbook is [../testing/cloud-eval-runbook.md](../testing/cloud-eval-runbook.md).
