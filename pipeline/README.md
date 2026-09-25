# Eval pipeline

| Path | What |
|------|------|
| [`stages/`](stages/) | P0–P8 bash (`run_all.sh`, `check_report.sh`) |
| [`lib/`](lib/) | Pier adapter, baseline, scoring, testdata |

Runtime (gitignored): **`eval-runs/`** and **`output/`**. P5 is a `CPU_LOCK_QTY` work-unit pool (one question's rollouts, then the next question) — [optimization.md](../docs/optimization.md). P8 writes `eval-runs/output/<benchmark>/<run>/artifact.json` plus `summary.md` and `report.html`, and stores the same run as `output/<benchmark>/<run>.tar.gz`. A full eval does not run the no-harness P6 stage.

A Release binary extracts this tree to `~/.local/share/mac-k3d/pipeline`. Users provide iCode via a `*-full-*` drop or a git clone — [icode-harness-inputs.md](../docs/icode-harness-inputs.md).

Operator commands: [docs/user-guide.md](../docs/user-guide.md).
