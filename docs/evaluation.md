# Evaluation results

One Jenkins build scores the iCode harness. Harbor runs iCode for `N_ROLLOUTS` attempts on each question. A later comparison harness is not part of this run.

Release and git both mount the supplied iCode at `/opt/icode-host` and run the same `icode run` command. That process calls the model through the OpenAI-compatible client: `ICODE_PROVIDER=OpenAI`, `ICODE_API_BASE=https://api.deepseek.com/v1`, and `ICODE_MODEL` set to the catalog id (`deepseek-flash` or `deepseek-v4-pro`). The artifact `model` field is `openai/<catalog id>`. The HTTP model id is still the catalog id. P0 still checks `GET https://api.deepseek.com/models` for that id.

DeepSWE, LoLBench, and SWE-bench Pro all use this path. Question choice: a non-empty `TASKS` list wins; otherwise one `TASK`; otherwise `N_TASKS` is the first N sorted ids. Full suite sizes are DeepSWE 113, LoLBench 20, and SWE-bench Pro 731. `N_ROLLOUTS` is how many attempts each question gets. Default rollout count is 1.

## Where a run is stored

P8 writes one folder per run under the Jenkins workspace (or `eval-runs` for a local stage run). The folder holds three files:

```text
eval-runs/output/<benchmark>/jenkins-<BUILD_NUMBER>-<UTC>/
  artifact.json
  summary.md
  report.html
```

Example: `eval-runs/output/deepswe/jenkins-23-20260923T052713Z/artifact.json`. Jenkins archives only that folder, so the build page shows these three files. Older build pages stay downloadable. A local stage run with no `BUILD_NUMBER` names the folder `<UTC>-<task>` or `<UTC>-n<count>`. `report.pdf` and the long `eval-icode-deepseek-...json` name are not written.

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

`n_tasks`, `n_rollouts`, `concurrency`, and `cpus_each` are stored once at the top of the JSON. The spread next to macro Pass@1 is `macro_pass@1_sd`, the sample standard deviation of the per-question `c/n` values. `macro_pass@1_scored_sd` is the same for `pass_frac_scored`. One question has SD 0.

CPU slot packing, the RAM ceiling, and the disk gate are in [optimization.md](optimization.md).
