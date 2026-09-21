# Pipeline helpers (Pier agent + baseline + scoring)

Used by `mac-k3d eval` and Jenkins jobs `deepswe_one_task` (Pier) and `lolbench_one_task` (Harbor). Stages live in [`../stages/`](../stages/).

| Path | Role |
|------|------|
| `icode_input.sh` | P3 iCode inputs: Jenkins upload / local `*-full-*`, or git clone; leftover `icode-src` wipe via Docker |
| `icode_pier_agent.py` | Pier 0.3.1 `ICodeAgent` (`--agent-import-path icode_pier_agent:ICodeAgent`) |
| `icode_harbor_agent.py` | Harbor `ICodeAgent` using the worker iCode tree bind-mounted at `/opt/icode-host` |
| `pier-agent-icode/` | Sandbox scripts: install iCode, run against `instruction.md` |
| `baseline_deepseek.py` | Arm B: DeepSeek chat without iCode |
| `score_results.py` | Merge harness/baseline into one P8 schema (Pass@1, F2P/P2P rates, tokens, `wall_minutes`) for DeepSWE and LoLBench |
| `check_report.py` | Validate required report keys (no network) |
| `testdata/report-min.json` | Fixture for `pipeline/stages/check_report.sh` |

Runtime artifacts go to **`eval-runs/`** (gitignored), reports under `eval-runs/reports/`.

Users drop the iCode binary at `~/.local/share/mac-k3d/icode` (not in this folder), or clone via `ICODE_MODE=git`. See [docs/binary-initializer/icode-harness-inputs.md](../../docs/binary-initializer/icode-harness-inputs.md).
