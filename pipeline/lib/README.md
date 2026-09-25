# Pipeline helpers (Harbor agent + baseline + scoring)

Used by `mac-k3d eval` and Jenkins jobs `deepswe_one_task`, `lolbench_one_task`, and `swebenchpro_one_task` (all Harbor). Stages live in [`../stages/`](../stages/).

| Path | Role |
|------|------|
| `icode_input.sh` | P3 iCode inputs: Jenkins upload / local `*-full-*`, or git clone; leftover `icode-src` wipe via Docker |
| `icode_pier_agent.py` | Pier 0.3.1 `ICodeAgent` (`--agent-import-path icode_pier_agent:ICodeAgent`) |
| `icode_harbor_agent.py` | Harbor `ICodeAgent` for all three benchmarks; worker iCode tree bind-mounted at `/opt/icode-host` |
| `eval_slots.py` | P5/P6 slot cap, Docker memory cap, and one-question-at-a-time rollout order ([optimization.md](../../docs/optimization.md)) |
| `eval_progress.py` | P5 `progress.json` and the 60s heartbeat ETA line |
| `pier-agent-icode/` | Unused by the current runner. Kept from the Pier path. |
| `swebenchpro_tasks.py` | P2: materialize Harbor `tasks/<instance_id>/` (`task.toml`, tests) from the SWE-bench Pro dataset |
| `swebenchpro_run.py` | Docker Hub tag helper used when writing those Harbor tasks |
| `baseline_deepseek.py` | Manual P6 only. A full eval does not run it |
| `score_results.py` | Merge harness/baseline into one P8 schema (Pass@1, F2P/P2P rates, tokens, `wall_minutes`) for DeepSWE, LoLBench, and SWE-bench Pro |
| `check_report.py` | Validate required report keys (no network) |
| `testdata/report-min.json` | Fixture for `pipeline/stages/check_report.sh` |

Runtime artifacts go to **`eval-runs/`** and **`output/`** (both gitignored). Each run writes `artifact.json`, `summary.md`, and `report.html` under `eval-runs/output/<benchmark>/<run>/`, and stores that folder as `output/<benchmark>/<run>.tar.gz`.

Users provide iCode via Jenkins upload (`ICODE_RELEASE_FILE`), local persist/discover (`*-full-*` or `icode` under `~/.local/share/mac-k3d/`), or `ICODE_MODE=git`. See [docs/icode-harness-inputs.md](../../docs/icode-harness-inputs.md).
