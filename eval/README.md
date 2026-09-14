# Eval helpers (Pier agent + baseline + scoring)

Used by `mac-k3d eval` and Jenkins job `icode_eval`.

| Path | Role |
|------|------|
| `icode_pier_agent.py` | Pier 0.3.1 `ICodeAgent` (`--agent-import-path icode_pier_agent:ICodeAgent`) |
| `pier-agent-icode/` | Sandbox scripts: install iCode, run against `instruction.md`; writes `timing.json` |
| `baseline_deepseek.py` | Arm B: DeepSeek chat API (`DEEPSEEK_MODEL`, default `deepseek-v4-pro`) without iCode; writes usage/time/model |
| `score_results.py` | Merge harness/baseline into named JSON (f2p/p2p, pass@1, tokens, duration, model) |
| `check_report.py` | Validate required report keys (no network) |
| `testdata/report-min.json` | Fixture for E3 / `check_report.sh` |

Local paid stages load `DEEPSEEK_API_KEY` from a gitignored `.env` (copy [`.env.example`](../.env.example)). Do not export the key. See [docs/secrets.md](../docs/secrets.md).

Docs: [docs/binary-initializer/workflow.md](../docs/binary-initializer/workflow.md), [testing-eval-pipeline.md](../docs/binary-initializer/testing-eval-pipeline.md).
