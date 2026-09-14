# Pipeline helpers (Pier agent + baseline + scoring)

Used by `mac-k3d eval` and Jenkins job `icode_eval`. Stages live in [`../stages/`](../stages/).

| Path | Role |
|------|------|
| `icode_pier_agent.py` | Pier 0.3.1 `ICodeAgent` (`--agent-import-path icode_pier_agent:ICodeAgent`) |
| `pier-agent-icode/` | Sandbox scripts: install iCode, run against `instruction.md` |
| `baseline_deepseek.py` | Arm B: DeepSeek chat without iCode |
| `score_results.py` | Merge harness/baseline into named JSON |
| `check_report.py` | Validate required report keys (no network) |
| `testdata/report-min.json` | Fixture for `pipeline/stages/check_report.sh` |

Runtime artifacts go to **`eval-runs/`** (gitignored), reports under `eval-runs/reports/`.

Users drop the iCode binary at `~/.local/share/mac-k3d/icode` (not in this folder). Developers may use `ICODE_MODE=source` and a gitignored `.env`. See [docs/binary-initializer/user-guide.md](../../docs/binary-initializer/user-guide.md).
