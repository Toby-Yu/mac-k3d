# Evaluation results

One Jenkins build scores the iCode harness. Harbor runs iCode for `N_ROLLOUTS` attempts on each question. A later comparison harness is not part of this run.

**One build issues one `harbor run`.** mac-k3d selects the questions and prepares the environment; Harbor expands `n_attempts x tasks x agents` into trials, runs `-n` of them at a time, applies each task's declared limits, retries, and grades. mac-k3d does not loop over rollouts and does not schedule containers. See [architecture.md](architecture.md#evaluation-architecture) for the full responsibility split.

Release and git both mount the supplied iCode at `/opt/icode-host` and run the same single-shot `icode run` (one question × one rollout, no improve-loop). DeepSeek official API uses `ICODE_PROVIDER=DeepSeek` and `ICODE_REASONING_EFFORT=high` (wire fields `thinking.type=enabled` and `reasoning_effort=high`) unless those env vars are already set. `ICODE_API_BASE` is `https://api.deepseek.com/v1`. `ICODE_MODEL` is the catalog id (`deepseek-flash` or `deepseek-v4-pro`). The artifact `model` field is still `openai/<catalog id>` (catalog label, not the provider string). The `env` phase (`env/model_api`) still checks `GET https://api.deepseek.com/models` for that id.

After `icode run`, the Harbor agent force-commits a dirty git tree so DeepSWE can grade `git diff <base> HEAD` and LoLBench submit sees the same tree. That commit does not start another model call.

Each task's `cpus`, `memory_mb` and `storage_mb` come from its own `task.toml` and are **not** overridden: a DeepSWE task gets the 2 CPUs and 8 GB it declares, a LoLBench task gets its 4 CPUs. `pipeline/lib/task_resources.py` reads the declared values, plans how many fit on this worker (`EVAL_SLOTS`, which becomes Harbor's `-n`), and **fails the build before any container starts** if even one selected task does not fit. The build locks every core of its worker (one eval build per worker); that count arrives as `CPU_LOCK_QTY` and is an upper bound on the plan, not the per-task limit. `artifact.json` records `resources.declared` against `resources.applied`. See [optimization.md](optimization.md).

Harbor’s agent budget is the task’s `agent.timeout_sec` (often 10800s on DeepSWE) with timeout multiplier 1. A rollout that ends near 5100s is iCode stopping itself, not Harbor cutting the trial.

The `report` phase stores `eval_protocol` on `artifact.json` (iCode mode/version/source, model params, concurrency, `cpus_each`, the timeout note, and `isolation`: what the agent could see at `/opt/icode-host`) and repeats it in `summary.md` and `report.html`.

DeepSWE, LoLBench, and SWE-bench Pro all use this path. Question choice: a non-empty `TASKS` list wins; otherwise one `TASK`; otherwise `N_TASKS` is the first N sorted ids, skipping the first `TASK_OFFSET` of them. Full suite sizes are DeepSWE 113, LoLBench 20, and SWE-bench Pro 731. `N_ROLLOUTS` is how many attempts each question gets. Job default is 4.

## Which job to run

Three jobs per suite, nine in total:

| Job | Questions | Rollouts (default) | Use it for |
|---|---|---|---|
| `<suite>_one_task` | `TASK` | 1, editable | smoke test after a code change; cheapest proof the pipeline still works |
| `<suite>_some_task` | `TASKS` list, or first `N_TASKS` | 4, editable | comparing a handful of questions against a known-good result |
| `<suite>_full_suite_task` | whole suite | 4, editable | the real run |

`<suite>_one_task` is the only job that evaluates. `some_task` and `full_suite_task` are dispatchers on `agent none`: they split the questions into contiguous shards of `SHARD_SIZE` (default 10, at least one shard per online worker), queue every shard as a `<suite>_one_task` build with its own `TASKS` or `TASK_OFFSET` and a shared `RUN_GROUP`, and wait. Each worker has one executor, so a worker that finishes a shard takes the next one from the queue, and a faster worker simply runs more of them. A shard build's description reads `shard 3/12 of deepswe_full_suite_task #7`.

When the shards are done, the same dispatcher build runs an `Aggregate` stage on a worker: it copies each shard's archived trials by build number and merges them into one `artifact.json` / `summary.md` / `report.html` under `aggregate/`, archived on the dispatcher build. Verdict counts add; shard failures do not hide the shards that worked (`propagate: false`, the build turns UNSTABLE), so a partial suite still produces a report for what finished. If the shards ran different pipeline commits, the merged anti-cheat block is marked `status: mixed_versions` rather than silently averaged. There is no separate aggregate job.

Merge an already-collected set of shards by hand:

```bash
python3 pipeline/lib/aggregate_runs.py \
  --shards <dir of shard eval-runs trees> \
  --benchmark deepswe --run-group deepswe_full_suite_task-7 \
  --n-rollouts 4 --out aggregate
```

## Same protocol for every benchmark and model

A score measures the iCode harness only if every run sees the same rules. So every security, anti-cheat and run control below applies identically to DeepSWE, LoLBench and SWE-bench Pro, and to every LLM. When you compare models, change only `DEEPSEEK_MODEL` / `ICODE_MODEL`; everything else stays fixed and is recorded in `eval_protocol`.

**Rule for changes:** a new control is added for all suites or not at all. A per-suite exception needs a written reason in this section and a matching test. Do not switch a control off to make one suite pass.

| Control | What the pipeline does (step in [pipeline.md](pipeline.md#steps)) | Code |
|---|---|---|
| Pinned iCode | `OFFICIAL=1` requires `ICODE_MODE=git` at the pinned commit; iCode is evaluated unmodified | `icode_input.sh` (`icode_apply_official_pin`) |
| Runtime stripped of deliverables | Removes `test/`, `idlelib/idle_test`, vendored `tomli` and `backports.zoneinfo`. Replaces the stdlib `tomllib` (the `cpython_5` deliverable) with a `.pyc`-only stub that imports but refuses to parse, because iCode's `alembic` imports `tomllib` on Python 3.11+. `zoneinfo` stays importable as `.pyc` only | `icode_sanitize.py` |
| Sourceless stdlib | The mounted sandbox CPython keeps `.pyc` only, so no stdlib `.py` is readable at `/opt/icode-host`. On for every benchmark | `ICODE_SOURCELESS_STDLIB` (see below) |
| No iCode Python paths in the agent's shell | The launcher exports no `PYTHONPATH` or `VIRTUAL_ENV`; iCode's own interpreter gets its paths from `icode-host.pth` | `icode_input.sh` |
| Runtime still starts | Host probe on the sanitized tree, before any rollout: `icode --help`, a pydantic model build, and an import of the modules `icode run` loads (`agent.factory`, `host.bootstrap`) | `icode_probe_sandbox` |
| One read-only mount | The only agent mount is the iCode tree at `/opt/icode-host`, `read_only: true`. `tasks/isolation` refuses a tree that is, contains or sits inside a benchmark or tasks folder | `agent_mounts.py` |
| Leak scan | Gold lines of every selected task are searched in the mounted tree (`anticheat_leakscan.json`). `OFFICIAL=1` stops on a hit; smoke runs warn | `anticheat_leakscan.py` |
| Network allowlist | The agent may reach only the hosts in `pipeline/config/network-allowlist-v1.json` (`--allow-agent-host`); `tasks/isolation` checks that the model API host is on it and records the list version, hosts and file hash in `eval_protocol.isolation.network_allowlist` | `network_allowlist.py` |
| Secrets | Only `DEEPSEEK_API_KEY` reaches the agent. Clone tokens are unset before Harbor and never written to `.harbor-env` | `evaluate/harbor_cmd.sh` |
| Anti-cheat verdict | The `anticheat` phase gives every rollout `clean`, `flagged` or `rejected` from line/path Jaccard against the gold patch and a transcript scan (mounted-runtime reads, filesystem searches, network and retrieval tools, git archaeology, gold and grader paths). The `report` phase scores `rejected` as unresolved with F2P/P2P zeroed, so `artifact["icode"]` is official and `artifact["icode_raw"]` keeps the raw numbers. Thresholds and rules: `pipeline/config/anticheat-v1.json`; reviewer decisions: `anticheat_overrides.json`. `OFFICIAL=1` requires the verdicts | `anticheat_verdict.py` |
| Declared base commit | `anticheat/receipts` compares each trial's capture receipt against the base commit the task declares and flags `base_commit_mismatch` when the agent started somewhere else. The check runs on the host because the agent env deliberately does not carry the declared base | `capture_receipt.py` (`base_mismatch`) |
| Pinned pipeline | The pipeline is the one embedded in the worker's `mac-k3d`, and `eval_protocol.pipeline` records its `commit`, `dirty` and `pipeline_hash`. `OFFICIAL=1` extracts it with `--require-clean` (no dirty build, no unknown commit), and an aggregate whose shards ran different pipelines is `pipeline_status: mixed`, which `OFFICIAL=1` rejects | `jenkins_job.rs` (`eval_bootstrap_sh`), `eval_assets.rs`, `check_report.py` |
| Isolation canary | Before the rollouts, `evaluate/canary` runs `CanaryAgent` with iCode's exact mounts, env and Harbor flags, but no model. In each task container it probes 13 source hosts (must be blocked), the model API (must answer), the task's declared hosts (a reached one warns), a filesystem search for the task's gold file names outside the repo, writes to `/opt/icode-host` as the agent and as root, git history beyond `HEAD`, env names that look like secrets, `PYTHONPATH`/`VIRTUAL_ENV`, and the iCode home before and after install. Any failed check stops the run before a token is spent. `OFFICIAL=1` requires it | `canary_verdict.py`, `canary_probe.sh` |
| Model protocol | Same `ICODE_PROVIDER`, `ICODE_REASONING_EFFORT` and `ICODE_API_BASE` defaults for every suite | `evaluate/harbor_cmd.sh` |

`ICODE_SOURCELESS_STDLIB` defaults to on. Setting it to `0`/`false`/`no`/`off` is only for a non-official debugging run: the `tasks` phase prints a warning that the run is not comparable, and `OFFICIAL=1` refuses it. Any other value keeps sourceless on.

The canary runs on the Jenkins parameter `CANARY`:

- `official` (default): on when `OFFICIAL=1`, off for smoke runs.
- `on`: run it on the first selected task, then the rollouts.
- `only`: run it on every selected task, then stop with no rollouts and no model tokens. `bash pipeline/tools/canary.sh` does the same from a worker shell.
- `off`: skip it. `OFFICIAL=1` refuses this.

`CANARY_ALLOW_HOST=<host>` is a test switch that opens one host for the canary only, to prove the canary fails. `OFFICIAL=1` refuses it. The canary then fails either way: `<host> reached (HTTP …)` is the proof, while `negative test inconclusive` means the opened host never answered, so choose a host this worker can reach. The model API gets 3 tries of 30 s each before `model_api` fails. If it still fails, test HTTPS from a plain container: a VPN tunnel with a smaller MTU than Docker's bridge stalls every TLS handshake, and the `env` phase and the evaluate steps print `WARNING: egress via … has MTU …` when they detect one. The report is `eval-runs/canary/jenkins-<BUILD_NUMBER>/report.md`; each failure prints a `CANARY FAIL task=… check=…` line. Rules and host lists: `pipeline/config/canary-v1.json`.

The canary skips one check: iCode PR 2 has no command that lists its enabled tools, so `tool_list` is `skip`. Web tools are still covered by the network check and by the transcript scan in the `anticheat` phase.

### Recorded proof: `eval_protocol.isolation`

Every run records the controls above in `eval_protocol.isolation` (in `artifact.json`, and as the Isolation, Agent mount, Leak scan and Canary lines under Provenance in `summary.md` and `report.html`). `tasks/isolation` hashes the tree itself when it records them, so the values describe the runtime the agent actually got rather than what the sanitizer claimed.

| Field | Meaning |
|---|---|
| `mode` | `git` (sanitized sandbox CPython) or `release` (binary only, nothing to sanitize) |
| `sanitizer` | Sanitizer version, for example `mac-k3d-icode-sanitize-v3` |
| `sourceless` | `true` when the mounted stdlib has no `.py` files |
| `removed` | How many deliverable paths the sanitizer removed |
| `runtime_sha256` | Fingerprint of the mounted runtime that is the same for every clone of one iCode commit, whatever the workspace path or file times. **Compare this one across runs** |
| `tree_sha256` | Exact bytes of `.venv`. It differs per workspace, because uv writes the clone path into `.venv/bin`. Use it only within a run |
| `manifest_matches_tree` | `true` when both fingerprints still equal what the sanitizer wrote, so nothing changed the tree after sanitizing |
| `mount` | Count, target and `read_only` of the agent mounts |
| `leak_scan` | Scanner version, `hit_tasks`, task counts per status, and the sha256 of `anticheat_leakscan.json` |
| `canary` | Canary rules version, `status`, the tasks it ran on, `failed_tasks`, `warn_tasks`, worker node, and the sha256 of its `summary.json`. Missing when the canary was off |

`runtime_sha256` reads the clone path as `/opt/icode-host` and leaves out what the container never uses: `pyvenv.cfg`, `__pycache__/`, links that leave the tree, and install bookkeeping (`*.dist-info/RECORD`, `uv_cache.json`). Stdlib bytecode is compiled with hash-based invalidation and container paths, so it is byte-identical on every clone. Two independent PR 2 clones (the DeepSWE and LoLBench Jenkins workspaces) gave the same `runtime_sha256`.

**Two runs are comparable** only when they have the same iCode commit, `sanitizer`, `runtime_sha256` and model protocol, with `sourceless: true`, `manifest_matches_tree: true`, one read-only mount and no leak hits. With `OFFICIAL=1`, `check_report.py` rejects an artifact whose own record breaks a per-run rule (git mode, sourceless, both fingerprints present, manifest still matching, one read-only mount, leak scan present with no hits, canary present with `status: pass`). Matching `runtime_sha256` between runs is checked by comparing their artifacts.

A tree sanitized by an older sanitizer version has already lost its stdlib sources, so it cannot be re-sanitized in place. `tasks/icode_sandbox` then copies the sandbox CPython again from the host interpreter and sanitizes it fresh (the console prints `re-embedding … for the current sanitizer`). Paths already removed outside the sandbox stay listed in the manifest.

**Allowed differences** come from the task, not from the harness: the task image, the agent timeout (`agent.timeout_sec`), the verifier and its declared network, the task path and working folder, LoLBench's `LOLBENCH_SUITE=union` verifier variable and its reward overlay, and the per-suite memory fallback for packing ([optimization.md](optimization.md)).

**Limits to keep in mind:**
- SWE-bench Pro tasks carry no `solution/solution.patch`, so the leak scan reports them as `no_gold` and cannot prove them clean. Every other control still applies.
- Release mode (`ICODE_MODE=release`) mounts only the binary, so there is no sandbox CPython to strip. Official runs use git mode.
- SWE-bench Pro has no gold patch, so its verdicts rest on the transcript scan alone (`similarity.status: no_gold`).
- Rescore an old run without changing it: `python3 pipeline/lib/anticheat_verdict.py --run-dir output/<suite>/<run> [--harbor-runs <jenkins harbor_runs/jenkins-N>]`. It writes `artifact.anticheat.json` and `anticheat/report.md` next to the original. Runs from before the anti-cheat scan (report item P0.5) keep their transcripts only in the Jenkins workspace, hence `--harbor-runs`.
- The canary proves isolation on the tasks it ran on, on that worker. `CANARY=on` covers only the first selected task; run `CANARY=only` on the whole suite after a Harbor, Docker or kernel change.
- LoLBench task allowlists declare `openrouter.ai`, `api.openai.com` and `api.anthropic.com` for the agent phase. The canary warns when they are reachable; closing them needs a versioned task overlay (tracked in the integration log).
- **Harbor has no contamination gate.** `harbor job summarize` is a removed shim that only prints a deprecation notice, and `harbor analyze` is rubric-only. Neither reads the gold patch, scans a transcript for leakage, or can reject a trial. Every control in this table is therefore mac-k3d's, and delegating scheduling to Harbor did not delegate integrity.

## Where a run is stored

The `report` phase writes one folder per run under the Jenkins workspace (or `eval-runs` for a local stage run). The folder holds the report files, and also `container_mem.jsonl` and `skipped_questions.txt` when this build wrote them:

```text
eval-runs/output/<benchmark>/jenkins-<BUILD_NUMBER>-<UTC>/
  artifact.json
  summary.md
  report.html
  cost-token-report.md   (archive phase)
  container_mem.jsonl
  skipped_questions.txt
```

Example: `eval-runs/output/deepswe/jenkins-23-20260923T052713Z/artifact.json`. Jenkins archives only that folder. Older build pages stay downloadable. A local stage run with no `BUILD_NUMBER` names the folder `<UTC>-<task>` or `<UTC>-n<count>`. `report.pdf` and the long `eval-icode-deepseek-...json` name are not written.

The `archive` phase adds `cost-token-report.md`, then packs the same files plus `tasks.txt`, the anti-cheat verdicts and the per-question trial files into one gitignored archive so a workspace wipe does not remove them:

```text
output/<benchmark>/jenkins-<BUILD_NUMBER>-<UTC>.tar.gz
  <folder>/artifact.json
  <folder>/summary.md
  <folder>/report.html
  <folder>/cost-token-report.md
  <folder>/tasks.txt
  <folder>/icode/<task>/attempt-01/   result.json, reward.json, usage, patches
```

Extracting the archive recreates `<folder>/`. The checkout `output/` directory and the benchmark folder are created when missing. One local question is named like `20260923T023527Z-abs-module-cache-flags.tar.gz`. Several questions use `<UTC>-n<count>.tar.gz`, and `tasks.txt` lists every id. The root is the pipeline's own folder: `$WORKSPACE/mac-k3d-pipeline/output/` on Jenkins (kept across builds), the checkout `output/` for a local run. `MAC_K3D_BACKUP_ROOT` or `MAC_K3D_OUTPUT_ROOT` overrides it. The archive does not include `.harbor-env` or the API key.

The JSON is the source for `summary.md` and `report.html`. The harness arm is `icode`. `summary.md` labels the run `<suite>-<build>` (for example `deepswe-25`) and points at `output/<suite>/<folder>.tar.gz`. `pass@1` is the share of questions whose oldest attempt resolved. `pass@k` is the share with at least one resolved attempt in the first k attempts. `macro_pass@1` is the mean of each question’s `c/n`, which is a different number from `pass@1`. `best_attempt_hits` is how many questions had a resolved best attempt. `infra_excluded` is how many selected questions had no scored attempt. `tokens.avg_total_per_task` is `tokens.total` divided by tasks that reported tokens. `dur_s` on a question is the best attempt’s Harbor agent span (`agent_execution` finished minus started, otherwise the trial span, otherwise the chat `usage.json` duration). `timing.wall_seconds` is the span from the earliest attempt start to the latest attempt finish.

## What the fields mean

Pass@k is stored twice. `pass@k` / `macro_pass@1` / `c` / `n` / `pass_frac` are **padded**: a missing attempt counts as not resolved and `n` is `N_ROLLOUTS`. `pass@k_scored` / `macro_pass@1_scored` / `c_scored` / `n_scored` / `pass_frac_scored` are **scored-only**: the denominator is attempts that have a `reward.json`, missing attempts are omitted, and a question with `n_scored == 0` is dropped from that ladder. `pass_methods` on the arm repeats those two formulas. If `N_ROLLOUTS` is k, each arm stores Pass@1 through Pass@k for both methods.

| Field | Meaning |
|-------|---------|
| `c` / `n` | Resolved attempts / requested attempts for that question (padded). |
| `c_scored` / `n_scored` | Resolved attempts / attempts that have a verifier result. |
| `pass_frac` | `c/n`. Macro Pass@1 is the mean of these fractions. |
| `pass_frac_scored` | `c_scored/n_scored`. `macro_pass@1_scored` is the mean of these, over questions with `n_scored > 0`. |
| `first` | The oldest attempt resolved. First-rollout Pass@1 is the share of questions where this is true. |
| `best` | Any attempt resolved. Any-pass is the share of questions where this is true. |
| `reward` | 1 when `best` is true, otherwise 0. |
| `f2p`, `p2p`, `partial` | Rates from the best attempt (resolved, then highest partial). |
| `f2p_pass` / `f2p_total`, `p2p_pass` / `p2p_total` | Counts from that same attempt. Micro rates sum these counts across questions. |
| `tok_in` / `tok_out` | Sum of tokens across that question’s attempts. |
| `dur_s` | Duration of the best attempt, in seconds. For a Harbor trial this is the agent execution span. |
| `notes` | Empty patch, apply failure, or a missing reward file. `-` when there is nothing to say. |
| `concurrency` | Max live Harbor containers in the slot pool. Stored once on the run, not again inside each arm. Packing is in [optimization.md](optimization.md). |
| `any_pass` | Share of questions with any resolved attempt. Same rate as padded `pass@k` when k is the rollout count. |
| `unscored` / `unscored_frac` | No-response attempts for that question / `n`. A missing `reward.json` is unscored, not a scored fail. |
| `unscored_rollouts` | Sum of per-question `unscored`. |
| `unscored_tasks` | Questions with `unscored > 0`, highest `unscored_frac` first. Clean questions are omitted. |
| `pass_methods` | `padded` and `scored` formula strings for the two Pass@k families. |
| `timing.eta_note` | Present only when the report covers an incomplete Harbor run (`progress.json` `done < needed`). A finished run does not invent an ETA. |
| `tokens.total` | `in` plus `out`. |
| `rollouts` | One object per attempt. Omitted when every requested attempt exists and every one has F2P 1 and P2P 1. |

`n_tasks`, `n_rollouts`, `concurrency`, and `cpus_each` are stored once at the top of the JSON. `eval_protocol` records the iCode version under test, `provider`, `reasoning_effort`, `thinking.type`, and the CPU lock actually used. The band next to macro Pass@1 is `macro_pass@1_ci`, a 95% confidence interval half-width of the mean of the per-question `c/n` values: `1.96 * sample_sd / sqrt(M)`. `macro_pass@1_scored_ci` is the same for `pass_frac_scored`. Fewer than two questions has half-width 0.

CPU slot packing, the open slots, and the disk floor are in [optimization.md](optimization.md).

## Cost and token report

The `archive` phase writes `cost-token-report.md` into every run folder from that run’s `artifact.json` (list-price estimate, not DeepSeek’s invoice) before packing it. For an older run, or to re-run it, call the script directly:

```bash
cd /path/to/mac-k3d

# Explicit folder (loose extract or workspace copy)
python3 pipeline/lib/cost_token_report.py \
  --run-dir output/deepswe/jenkins-37-20260927T153132Z

# Suite + Jenkins build number (searches output/ and eval-runs/output/)
python3 pipeline/lib/cost_token_report.py --suite deepswe --build 37

# Suite + full folder stem
python3 pipeline/lib/cost_token_report.py --suite lolbench --run jenkins-21-20260929T010830Z

# Compressed backup only
python3 pipeline/lib/cost_token_report.py \
  --tar output/deepswe/jenkins-37-20260927T153132Z.tar.gz
```

Default output is `<run-dir>/cost-token-report.md`. Override with `--out PATH`. A failure here prints a warning and does not stop the backup.
