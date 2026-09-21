# Eval pipeline

| Path | What |
|------|------|
| [`stages/`](stages/) | P0–P8 bash (`run_all.sh`, `check_report.sh`) |
| [`lib/`](lib/) | Pier adapter, baseline, scoring, testdata |

Runtime (gitignored): **`eval-runs/`** with reports in `eval-runs/reports/`.

A Release binary extracts this tree to `~/.local/share/mac-k3d/pipeline`. Users provide iCode via a `*-full-*` drop or a git clone — [icode-harness-inputs.md](../docs/icode-harness-inputs.md).

Operator commands: [docs/user-guide.md](../docs/user-guide.md).
