# Pipeline helpers (Harbor agent + baseline + scoring)

Used by `mac-k3d eval` and Jenkins jobs `deepswe_one_task`, `lolbench_one_task`, and `swebenchpro_one_task` (all Harbor). Stages live in [`../stages/`](../stages/).

| Path | Role |
|------|------|
| `icode_input.sh` | P3 iCode inputs: Jenkins upload / local `*-full-*`, or git clone; leftover `icode-src` wipe via Docker |
| `icode_pier_agent.py` | Pier 0.3.1 `ICodeAgent` (`--agent-import-path icode_pier_agent:ICodeAgent`) |
| `icode_harbor_agent.py` | Harbor `ICodeAgent` for all three benchmarks; worker iCode tree bind-mounted at `/opt/icode-host` |
| `icode_capture.sh` | Runs in the task container for `ICodeAgent` and `PatchAgent`. `base` records the base commit before work; `capture` removes runtime output, diffs against that base, hands the patch to the suite's grader (LoLBench: `lolbench-submit` before the commit, replaced by the standard patch when they differ; DeepSWE / SWE-bench Pro: their `verifier.collect` hook), commits, and writes `/logs/agent/capture.json` |
| `capture_receipt.py` | `declared-repo`: repo and base a task declares (P5 passes the repo candidates as `MAC_K3D_REPO_CANDIDATES`; the base stays host-side). `annotate`: P7 compares the patch each grader read with the receipt and writes `agent/capture_flags.json` (`capture_mismatch`, `capture_missing`, `capture_error`, `patch_oversize`, `base_commit_mismatch`) |
| `anticheat_similarity.py` | Line and path Jaccard of a patch against `solution/solution.patch`, over added non-test, non-generated, non-trivial lines |
| `anticheat_transcript.py` | Rule scan of `events.jsonl` tool calls (plain or gzipped); excerpts of at most 200 characters with secrets masked |
| `anticheat_verdict.py` | P7: `agent/anticheat.json` per trial and `anticheat/{summary.json,report.md}`; `--run-dir` rescores an old run into `artifact.anticheat.json`. Rules in `../config/anticheat-v1.json` |
| `canary_harbor_agent.py` | Harbor `CanaryAgent(ICodeAgent)`: same mounts, env and flags as iCode, runs `canary_probe.sh` instead of the model (P5 `CANARY`, `../stages/p5c_canary.sh`) |
| `canary_probe.sh` | Runs in the task container: `home <tag>`, `probe` (egress, gold-file search, mount write, repo history, env names) and `mount` (as root); writes facts to `/logs/agent/canary*.json`, always exits 0 |
| `canary_verdict.py` | `spec`: host lists and gold file names for one task (never gold content). `summarize`: pass/warn/fail per check and task into `summary.json` and `report.md`; exit 2 on a failed task. Rules in `../config/canary-v1.json` |
| `harness_labels.py` | Loads `../config/harness/icode-pr2-eea9d66.yaml` (`tuned_on`, `hinted`); `generate` rebuilds it from the improve loop and iCode's matchers, read-only |
| `task_resources.py` | `plan`: declared `cpus`/`memory_mb`/`storage_mb` per task.toml → `EVAL_SLOTS` (Harbor's `-n`), failing when one task does not fit this worker. `sample`: append a `docker stats` snapshot to the build's memory trace ([optimization.md](../../docs/optimization.md)) |
| `eval_slots.py` | Memory and disk helpers only (`docker stats` parsing, container peak). Harbor owns scheduling, so there is no slot loop here |
| `aggregate_runs.py` | `Aggregate` stage of `_some_task` / `_full_suite_task`: merge the shard trials of one `RUN_GROUP` into one `artifact.json` / `summary.md` / `report.html` |
| `eval_progress.py` | P5 `progress.json` and the 60s heartbeat (`done/needed`, inflight, slots) |
| `pier-agent-icode/` | Unused by the current runner. Kept from the Pier path. |
| `swebenchpro_tasks.py` | P2: materialize Harbor `tasks/<instance_id>/` (`task.toml`, tests) from the SWE-bench Pro dataset |
| `swebenchpro_run.py` | Docker Hub tag helper used when writing those Harbor tasks |
| `baseline_deepseek.py` | Manual P6 only. A full eval does not run it |
| `score_results.py` | Merge harness/baseline into one P8 schema (Pass@1, F2P/P2P rates, tokens, `wall_minutes`) for DeepSWE, LoLBench, and SWE-bench Pro |
| `check_report.py` | Validate required report keys (no network) |
| `testdata/report-min.json` | Fixture for `pipeline/stages/check_report.sh` |

Runtime artifacts go to **`eval-runs/`** and **`output/`** (both gitignored). Each run writes `artifact.json`, `summary.md`, and `report.html` under `eval-runs/output/<benchmark>/<run>/`, and stores that folder as `output/<benchmark>/<run>.tar.gz`.

Users provide iCode via Jenkins upload (`ICODE_RELEASE_FILE`), local persist/discover (`*-full-*` or `icode` under `~/.local/share/mac-k3d/`), or `ICODE_MODE=git`. See [docs/icode-harness-inputs.md](../../docs/icode-harness-inputs.md).
