# Eval helpers (Pier agent + baseline + scoring)

Used by `mac-k3d eval` and Jenkins job `icode_eval`.

| Path | Role |
|------|------|
| `pier-agent-icode/` | Pier agent package: install iCode into the sandbox, run against `instruction.md` |
| `baseline_deepseek.py` | Arm B: DeepSeek chat API without iCode |
| `score_results.py` | Merge harness/baseline into named JSON (f2p/p2p best-effort) |

Docs: [docs/binary-initializer/workflow.md](../docs/binary-initializer/workflow.md), [testing-eval-pipeline.md](../docs/binary-initializer/testing-eval-pipeline.md).
