#!/usr/bin/env bash
# P7 — score harness vs baseline into temp JSON
set -euo pipefail
source "$(cd "$(dirname "$0")" && pwd)/_common.sh"

progress 90 "P7: scoring f2p/p2p"

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
