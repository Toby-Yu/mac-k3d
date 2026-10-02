# Evaluation results

One Jenkins build scores the iCode harness. Harbor runs iCode for `N_ROLLOUTS` attempts on each question. A later comparison harness is not part of this run.

Release and git both mount the supplied iCode at `/opt/icode-host` and run the same single-shot `icode run` (one question × one rollout, no improve-loop). DeepSeek official API uses `ICODE_PROVIDER=DeepSeek` and `ICODE_REASONING_EFFORT=high` (wire fields `thinking.type=enabled` and `reasoning_effort=high`) unless those env vars are already set. `ICODE_API_BASE` is `https://api.deepseek.com/v1`. `ICODE_MODEL` is the catalog id (`deepseek-flash` or `deepseek-v4-pro`). The artifact `model` field is still `openai/<catalog id>` (catalog label, not the provider string). P0 still checks `GET https://api.deepseek.com/models` for that id.

After `icode run`, the Harbor agent force-commits a dirty git tree so DeepSWE can grade `git diff <base> HEAD` and LoLBench submit sees the same tree. That commit does not start another model call.

Default packing stays `CPU_LOCK_QTY=4` as the **upper** concurrency bound, with `N_ROLLOUTS=4`. Live `EVAL_SLOTS` and `--override-memory-mb` are set **per question** from that question’s history peak (×1.5), falling back to suite defaults (LoLBench 4.0 GB, DeepSWE 1.5 GB) so Flink-class peaks shrink to one slot while light questions keep all CPUs. See [optimization.md](optimization.md). After an aborted full suite, set Jenkins `RESUME=true` with the same question selection so P5 skips units that already have `reward.json`.

Harbor’s agent budget is the task’s `agent.timeout_sec` (often 10800s on DeepSWE) with timeout multiplier 1. A rollout that ends near 5100s is iCode stopping itself, not Harbor cutting the trial.

P8 stores `eval_protocol` on `artifact.json` (iCode mode/version/source, model params, concurrency, `cpus_each`, the timeout note, and `isolation`: what the agent could see at `/opt/icode-host`) and repeats it in `summary.md` and `report.html`.

DeepSWE, LoLBench, and SWE-bench Pro all use this path. Question choice: a non-empty `TASKS` list wins; otherwise one `TASK`; otherwise `N_TASKS` is the first N sorted ids. Full suite sizes are DeepSWE 113, LoLBench 20, and SWE-bench Pro 731. `N_ROLLOUTS` is how many attempts each question gets. Job default is 4.

## Same protocol for every benchmark and model

A score measures the iCode harness only if every run sees the same rules. So every security, anti-cheat and run control below applies identically to DeepSWE, LoLBench and SWE-bench Pro, and to every LLM. When you compare models, change only `DEEPSEEK_MODEL` / `ICODE_MODEL`; everything else stays fixed and is recorded in `eval_protocol`.

**Rule for changes:** a new control is added for all suites or not at all. A per-suite exception needs a written reason in this section and a matching test. Do not switch a control off to make one suite pass.

| Control | What P3/P5 does | Code |
|---|---|---|
| Pinned iCode | `OFFICIAL=1` requires `ICODE_MODE=git` at the pinned commit; iCode is evaluated unmodified | `icode_input.sh` (`icode_apply_official_pin`) |
| Runtime stripped of deliverables | Removes `tomllib`, `test/`, `idlelib/idle_test`, vendored `tomli` and `backports.zoneinfo`; `zoneinfo` stays importable as `.pyc` only | `icode_sanitize.py` |
| Sourceless stdlib | The mounted sandbox CPython keeps `.pyc` only, so no stdlib `.py` is readable at `/opt/icode-host`. On for every benchmark | `ICODE_SOURCELESS_STDLIB` (see below) |
| No iCode Python paths in the agent's shell | The launcher exports no `PYTHONPATH` or `VIRTUAL_ENV`; iCode's own interpreter gets its paths from `icode-host.pth` | `icode_input.sh` |
| Runtime still starts | Host probe: `icode --help` and a pydantic model build on the sanitized tree, before any rollout | `icode_probe_sandbox` |
| One read-only mount | The only agent mount is the iCode tree at `/opt/icode-host`, `read_only: true`. P5 refuses a tree that is, contains or sits inside a benchmark or tasks folder | `agent_mounts.py` |
| Leak scan | Gold lines of every selected task are searched in the mounted tree (`anticheat_leakscan.json`). `OFFICIAL=1` stops on a hit; smoke runs warn | `anticheat_leakscan.py` |
| Secrets | Only `DEEPSEEK_API_KEY` reaches the agent. Clone tokens are unset before Harbor and never written to `.harbor-env` | `p5_harness.sh` |
| Anti-cheat verdict | P7 gives every rollout `clean`, `flagged` or `rejected` from line/path Jaccard against the gold patch and a transcript scan (mounted-runtime reads, filesystem searches, network and retrieval tools, git archaeology, gold and grader paths). P8 scores `rejected` as unresolved with F2P/P2P zeroed, so `artifact["icode"]` is official and `artifact["icode_raw"]` keeps the raw numbers. Thresholds and rules: `pipeline/config/anticheat-v1.json`; reviewer decisions: `anticheat_overrides.json`. `OFFICIAL=1` requires the verdicts | `anticheat_verdict.py` |
| Model protocol | Same `ICODE_PROVIDER`, `ICODE_REASONING_EFFORT` and `ICODE_API_BASE` defaults for every suite | `p5_harness.sh` |

`ICODE_SOURCELESS_STDLIB` defaults to on. Setting it to `0`/`false`/`no`/`off` is only for a non-official debugging run: P5 prints a warning that the run is not comparable, and `OFFICIAL=1` refuses it. Any other value keeps sourceless on.

### Recorded proof: `eval_protocol.isolation`

Every run records the controls above in `eval_protocol.isolation` (in `artifact.json`, and as the Isolation, Agent mount and Leak scan lines under Provenance in `summary.md` and `report.html`). P5 hashes the tree itself when it records them, so the values describe the runtime the agent actually got rather than what the sanitizer claimed.

| Field | Meaning |
|---|---|
| `mode` | `git` (sanitized sandbox CPython) or `release` (binary only, nothing to sanitize) |
| `sanitizer` | Sanitizer version, for example `mac-k3d-icode-sanitize-v2` |
| `sourceless` | `true` when the mounted stdlib has no `.py` files |
| `removed` | How many deliverable paths the sanitizer removed |
| `runtime_sha256` | Fingerprint of the mounted runtime that is the same for every clone of one iCode commit, whatever the workspace path or file times. **Compare this one across runs** |
| `tree_sha256` | Exact bytes of `.venv`. It differs per workspace, because uv writes the clone path into `.venv/bin`. Use it only within a run |
| `manifest_matches_tree` | `true` when both fingerprints still equal what the sanitizer wrote, so nothing changed the tree after sanitizing |
| `mount` | Count, target and `read_only` of the agent mounts |
| `leak_scan` | Scanner version, `hit_tasks`, task counts per status, and the sha256 of `anticheat_leakscan.json` |

`runtime_sha256` reads the clone path as `/opt/icode-host` and leaves out what the container never uses: `pyvenv.cfg`, `__pycache__/`, links that leave the tree, and install bookkeeping (`*.dist-info/RECORD`, `uv_cache.json`). Stdlib bytecode is compiled with hash-based invalidation and container paths, so it is byte-identical on every clone. Two independent PR 2 clones (the DeepSWE and LoLBench Jenkins workspaces) gave the same `runtime_sha256`.

**Two runs are comparable** only when they have the same iCode commit, `sanitizer`, `runtime_sha256` and model protocol, with `sourceless: true`, `manifest_matches_tree: true`, one read-only mount and no leak hits. With `OFFICIAL=1`, `check_report.py` rejects an artifact whose own record breaks a per-run rule (git mode, sourceless, both fingerprints present, manifest still matching, one read-only mount, leak scan present with no hits). Matching `runtime_sha256` between runs is checked by comparing their artifacts.

A tree sanitized by an older sanitizer version has already lost its stdlib sources, so it cannot be re-sanitized in place. P3/P5 then copy the sandbox CPython again from the host interpreter and sanitize it fresh (the console prints `re-embedding … for the current sanitizer`). Paths already removed outside the sandbox stay listed in the manifest.

**Allowed differences** come from the task, not from the harness: the task image, the agent timeout (`agent.timeout_sec`), the verifier and its declared network, the task path and working folder, LoLBench's `LOLBENCH_SUITE=union` verifier variable and its reward overlay, and the per-suite memory fallback for packing ([optimization.md](optimization.md)).

**Limits to keep in mind:**
- SWE-bench Pro tasks carry no `solution/solution.patch`, so the leak scan reports them as `no_gold` and cannot prove them clean. Every other control still applies.
- Release mode (`ICODE_MODE=release`) mounts only the binary, so there is no sandbox CPython to strip. Official runs use git mode.
- SWE-bench Pro has no gold patch, so its verdicts rest on the transcript scan alone (`similarity.status: no_gold`).
- Rescore an old run without changing it: `python3 pipeline/lib/anticheat_verdict.py --run-dir output/<suite>/<run> [--harbor-runs <jenkins harbor_runs/jenkins-N>]`. It writes `artifact.anticheat.json` and `anticheat/report.md` next to the original. Runs from before P0.5 keep their transcripts only in the Jenkins workspace, hence `--harbor-runs`.
- Still open and tracked in the integration report: the isolation canary (P0.6) and honoring each task's declared CPUs (P0.9).

## Where a run is stored

P8 writes one folder per run under the Jenkins workspace (or `eval-runs` for a local stage run). The folder holds the report files, and also `container_mem.jsonl` and `skipped_questions.txt` when this build wrote them:

```text
eval-runs/output/<benchmark>/jenkins-<BUILD_NUMBER>-<UTC>/
  artifact.json
  summary.md
  report.html
  container_mem.jsonl
  skipped_questions.txt
```

Example: `eval-runs/output/deepswe/jenkins-23-20260923T052713Z/artifact.json`. Jenkins archives only that folder. Older build pages stay downloadable. A local stage run with no `BUILD_NUMBER` names the folder `<UTC>-<task>` or `<UTC>-n<count>`. `report.pdf` and the long `eval-icode-deepseek-...json` name are not written.

The same files, plus `tasks.txt` and the per-question trial files, are packed into one gitignored archive so a workspace wipe does not remove them:

```text
output/<benchmark>/jenkins-<BUILD_NUMBER>-<UTC>.tar.gz
  <folder>/artifact.json
  <folder>/summary.md
  <folder>/report.html
  <folder>/tasks.txt
  <folder>/icode/<task>/attempt-01/   result.json, reward.json, usage, patches
```

Extracting the archive recreates `<folder>/`. The checkout `output/` directory and the benchmark folder are created when missing. One local question is named like `20260923T023527Z-abs-module-cache-flags.tar.gz`. Several questions use `<UTC>-n<count>.tar.gz`, and `tasks.txt` lists every id. `MAC_K3D_OUTPUT_ROOT` overrides the checkout `output/` directory. When Jenkins runs the extracted pipeline under `~/.local/share/mac-k3d`, the archive still goes to the checkout `output/`, not into the share directory. The archive does not include `.harbor-env` or the API key.

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
| `timing.eta_note` | Present only when P8 scores an incomplete P5 (`progress.json` `done < needed`). A finished run does not invent an ETA. |
| `tokens.total` | `in` plus `out`. |
| `rollouts` | One object per attempt. Omitted when every requested attempt exists and every one has F2P 1 and P2P 1. |

`n_tasks`, `n_rollouts`, `concurrency`, and `cpus_each` are stored once at the top of the JSON. `eval_protocol` records the iCode version under test, `provider`, `reasoning_effort`, `thinking.type`, and the CPU lock actually used. The band next to macro Pass@1 is `macro_pass@1_ci`, a 95% confidence interval half-width of the mean of the per-question `c/n` values: `1.96 * sample_sd / sqrt(M)`. `macro_pass@1_scored_ci` is the same for `pass_frac_scored`. Fewer than two questions has half-width 0.

CPU slot packing, the open slots, and the disk floor are in [optimization.md](optimization.md).

## Cost and token report (on demand)

After a run finishes, generate `cost-token-report.md` from that run’s `artifact.json` (list-price estimate, not DeepSeek’s invoice):

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

Default output is `<run-dir>/cost-token-report.md`. Override with `--out PATH`. This is not part of P8; re-run the command whenever you need the report.
