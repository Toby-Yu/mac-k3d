# Eval pipeline

| Path | What |
|------|------|
| [`stages/`](stages/) | P0–P8 bash (`run_all.sh`, `check_report.sh`) |
| [`lib/`](lib/) | Pier adapter, baseline, scoring, testdata |

Runtime (gitignored): **`eval-runs/`** with reports in `eval-runs/reports/`.

A Release binary extracts this tree to `~/.local/share/mac-k3d/pipeline`. Users drop iCode at `~/.local/share/mac-k3d/icode` (sibling of `pipeline/`).

Operator commands: [docs/binary-initializer/user-guide.md](../docs/binary-initializer/user-guide.md).
