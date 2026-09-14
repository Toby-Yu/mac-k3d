# Eval helpers (Pier agent + baseline + scoring)

Used by `mac-k3d eval` and Jenkins job `icode_eval`.

| Path | Role |
|------|------|
| `pier-agent-icode/` | Pier agent package: install iCode into the sandbox, run against `instruction.md`; writes `timing.json` |
| `baseline_deepseek.py` | Arm B: DeepSeek chat API (`DEEPSEEK_MODEL`, default `deepseek-v4-pro`) without iCode; writes usage/time/model |
| `score_results.py` | Merge harness/baseline into named JSON (f2p/p2p, pass@1, tokens, duration, model) |
| `check_report.py` | Validate required report keys (no network) |
| `testdata/report-min.json` | Fixture for E3 / `check_report.sh` |

Docs: [docs/binary-initializer/workflow.md](../docs/binary-initializer/workflow.md), [testing-eval-pipeline.md](../docs/binary-initializer/testing-eval-pipeline.md).
