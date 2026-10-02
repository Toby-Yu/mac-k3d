# Integration log

| Date | Step | Branch @ commit | Automated tests | Manual test and evidence | Result | Notes / follow-ups |
|---|---|---|---|---|---|---|
| 2026-09-30 | P0.1 | int/P0.1-freeze @ 79f7165 | cargo test ✓ (102 unit + 16 cli, 0 failed); unittest ✓ 73, no failures; bash -n on pipeline/stages/*.sh and pipeline/lib/*.sh ✓; shellcheck not installed | deepswe_one_task #40: N_TASKS=1 N_ROLLOUTS=1 CPU_LOCK_QTY=1; SUCCESS; output/deepswe/jenkins-40-20261002T012401Z; no new `.cursor/debug-*.log` | pass | Tag `baseline-2026-09-30` is `5b93d7b` (tree frozen before this cleanup). Removed the hard-coded `.cursor` debug log from `eval_slots.py`, including the H3 and H4 call sites. |
