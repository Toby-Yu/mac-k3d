#!/usr/bin/env bash
# P7 — score harness vs baseline into temp JSON
set -euo pipefail
source "$(cd "$(dirname "$0")" && pwd)/_common.sh"

progress 90 "P7: scoring f2p/p2p"

# Same check for every suite: the patch the grader read must match the capture receipt.
python3 "$PIPELINE_LIB/capture_receipt.py" annotate \
  --harness-dir "$HARNESS_DIR" \
  --tasks-dir "$(benchmark_tasks_dir)" \
  --task-file "$WORKDIR/selected_tasks.txt" \
  --benchmark "${BENCHMARK:-deepswe}" \
  || echo "WARNING: capture receipt check failed; scoring continues"

# Anti-cheat verdict per rollout. P8 scores rejected rollouts as unresolved.
rm -rf "$HARNESS_DIR/anticheat"
python3 "$PIPELINE_LIB/anticheat_verdict.py" \
  --harness-dir "$HARNESS_DIR" \
  --tasks-dir "$(benchmark_tasks_dir)" \
  --task-file "$WORKDIR/selected_tasks.txt" \
  --benchmark "${BENCHMARK:-deepswe}" \
  || echo "WARNING: anti-cheat verdicts failed; artifact.json records anticheat not_run"

python3 "$PIPELINE_LIB/score_results.py" \
  --harness-dir "$HARNESS_DIR" \
  --baseline-dir "$BASELINE_DIR" \
  --tasks-dir "$(benchmark_tasks_dir)" \
  --n-tasks "$N_TASKS" \
  --task-file "$WORKDIR/selected_tasks.txt" \
  --out "$RESULTS_DIR/score-temp.json"

[ -f "$RESULTS_DIR/score-temp.json" ] || die "score-temp.json not written"
echo "OK wrote $RESULTS_DIR/score-temp.json"
progress 95 "P7 complete"
