# Pipeline helpers (Harbor agent + baseline + scoring)

Used by `mac-k3d eval` and the Jenkins eval jobs (all Harbor). The phase and step scripts that call these live in [`../stages/`](../stages/); the phase map is in [docs/pipeline.md](../../docs/pipeline.md).

| Path | Role |
|------|------|
| `icode_input.sh` | `tasks/icode` iCode inputs: Jenkins upload / local `*-full-*`, or git clone; leftover `icode-src` wipe via Docker |
| `icode_harbor_agent.py` | Harbor `ICodeAgent` for all three benchmarks; worker iCode tree bind-mounted at `/opt/icode-host` |
| `harbor_egress.py` | `check`: runs Harbor's egress-control kernel probe (`env/egress`, and again in both evaluate steps), falling back to a pinned image only when Docker cannot run Harbor's; checks the egress sidecar image; writes `egress_probe.json`. `override`: the image `run_cmd` exports as `MAC_K3D_EGRESS_PROBE_IMAGE` |
| `harbor_probe_override.py` | Imported first by `icode_harbor_agent.py`: when `MAC_K3D_EGRESS_PROBE_IMAGE` is set, points Harbor's probe at that image and clears Harbor's cached answer. Changes nothing else in Harbor |
| `agent_mounts.py` | `tasks/isolation`: the one read-only mount (iCode at `/opt/icode-host`) and the check that refuses a tree overlapping a benchmark |
| `network_allowlist.py` | `hosts`: the agent's `--allow-agent-host` list from `../config/network-allowlist-v1.json`. `check --api-base`: the model API host is on it. `record`: writes version, hosts and file hash into `eval_protocol_inputs.json` |
| `anticheat_leakscan.py` | `tasks/leakscan`: any selected task's gold patch lines inside the mounted iCode tree |
| `icode_capture.sh` | Runs in the task container for `ICodeAgent` and `PatchAgent`. `base` records the base commit before work; `capture` removes runtime output, diffs against that base, hands the patch to the suite's grader (LoLBench: `lolbench-submit` before the commit, replaced by the standard patch when they differ; DeepSWE / SWE-bench Pro: their `verifier.collect` hook), commits, and writes `/logs/agent/capture.json` |
| `capture_receipt.py` | `declared-repo`: repo and base a task declares (`evaluate/harbor_run` passes the repo candidates as `MAC_K3D_REPO_CANDIDATES`; the base stays host-side). `check-trials`: `anticheat/receipts` fails the build when a trial in this build's jobs dir matches no selected task. `annotate`: `anticheat/receipts` compares the patch each grader read with the receipt and writes `agent/capture_flags.json` (`capture_mismatch`, `capture_missing`, `capture_error`, `patch_oversize`, `base_commit_mismatch`) |
| `anticheat_similarity.py` | Line and path Jaccard of a patch against `solution/solution.patch`, over added non-test, non-generated, non-trivial lines |
| `anticheat_transcript.py` | Rule scan of `events.jsonl` tool calls (plain or gzipped); excerpts of at most 200 characters with secrets masked |
| `anticheat_verdict.py` | `anticheat/verdict`: `agent/anticheat.json` per trial and `anticheat/{summary.json,report.md}`; `--run-dir` rescores an old run into `artifact.anticheat.json`. Rules in `../config/anticheat-v1.json` |
| `canary_harbor_agent.py` | Harbor `CanaryAgent(ICodeAgent)`: same mounts, env and flags as iCode, runs `canary_probe.sh` instead of the model (`evaluate/canary`, `CANARY`; `../tools/canary.sh`) |
| `canary_probe.sh` | Runs in the task container: `home <tag>`, `probe` (egress, gold-file search, mount write, repo history, env names) and `mount` (as root); writes facts to `/logs/agent/canary*.json`, always exits 0 |
| `canary_verdict.py` | `spec`: host lists and gold file names for one task (never gold content). `summarize`: pass/warn/fail per check and task into `summary.json` and `report.md`; exit 2 on a failed task. Rules in `../config/canary-v1.json` |
| `harness_labels.py` | Loads `../config/harness/icode-pr2-eea9d66.yaml` (`tuned_on`, `hinted`); `generate` rebuilds it from the improve loop and iCode's matchers, read-only |
| `task_resources.py` | `plan`: declared `cpus`/`memory_mb`/`storage_mb` per task.toml → `EVAL_SLOTS` (Harbor's `-n`), failing when one task does not fit this worker. `sample`: append a `docker stats` snapshot to the build's memory trace ([optimization.md](../../docs/optimization.md)) |
| `eval_slots.py` | Memory and disk helpers only (`docker stats` parsing, container peak). Harbor owns scheduling, so there is no slot loop here |
| `aggregate_runs.py` | `Aggregate` stage of `_some_task` / `_full_suite_task`: merge the shard trials of one `RUN_GROUP` into one `artifact.json` / `summary.md` / `report.html` |
| `eval_progress.py` | `evaluate/harbor_run` `progress.json` and the 60s heartbeat (`done/needed`, inflight, slots) |
| `swebenchpro_tasks.py` | `tasks/benchmark`: materialize Harbor `tasks/<instance_id>/` (`task.toml`, tests) from the SWE-bench Pro dataset |
| `swebenchpro_run.py` | Docker Hub tag helper used when writing those Harbor tasks |
| `baseline_deepseek.py` | Manual baseline only (`../tools/baseline.sh`). A full eval does not run it |
| `score_results.py` | `score/score`: merge harness/baseline into one report schema (Pass@1, F2P/P2P rates, tokens, `wall_minutes`) for DeepSWE, LoLBench, and SWE-bench Pro |
| `render_report.py` | `report/render`: `artifact.json`, `summary.md`, `report.html` for one run folder |
| `cost_token_report.py` | `archive/analysis`: `cost-token-report.md` in the run folder; also by hand for older runs |
| `archive_run.py` | `archive/backup`: copy the run folder, verdicts and per-trial files (transcripts gzipped and masked), then `tar.gz` |
| `check_report.py` | Validate required report keys (no network) |
| `question_log.py` | `record`: one row per question of a run into `docs/testing/question-log.md` (from `artifact.json`, or the console when the run stopped early), the Open problems table, and `status`/`builds` in `question-coverage.md`. Called by `mac-k3d eval record` and at the end of `eval --local` |
| `testdata/report-min.json` | Fixture for `pipeline/tools/check_report.sh` |

Runtime artifacts go to **`eval-runs/`** and **`output/`** (both gitignored). Each run writes `artifact.json`, `summary.md`, and `report.html` under `eval-runs/output/<benchmark>/<run>/` (`render_report.py`), adds `cost-token-report.md` (`cost_token_report.py`), and stores that folder as `output/<benchmark>/<run>.tar.gz` (`archive_run.py`).

Users provide iCode via Jenkins upload (`ICODE_RELEASE_FILE`), local persist/discover (`*-full-*` or `icode` under `~/.local/share/mac-k3d/`), or `ICODE_MODE=git`. See [docs/icode-harness-inputs.md](../../docs/icode-harness-inputs.md).
