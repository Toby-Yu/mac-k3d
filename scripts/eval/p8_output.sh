#!/usr/bin/env bash
# P8 — write named output JSON
set -euo pipefail
source "$(cd "$(dirname "$0")" && pwd)/_common.sh"

progress 98 "P8: named output JSON"

[ -f "$RESULTS_DIR/score-temp.json" ] || bash "$(dirname "$0")/p7_score.sh"

UTC="$(date -u +%Y%m%dT%H%M%SZ)"
OUT="$OUTPUT_DIR/eval-${HARNESS}-${LLM}-${BENCHMARK}-n${N_TASKS}-${UTC}.json"

python3 "$MAC_K3D_ROOT/eval/score_results.py" \
  --harness-dir "$HARNESS_DIR" \
  --baseline-dir "$BASELINE_DIR" \
  --tasks-dir "$DEEPSWE_DIR/tasks" \
  --n-tasks "$N_TASKS" \
  --out "$OUT" \
  --finalize

echo "OK wrote $OUT"
echo "$OUT" >"$WORKDIR/last_output.txt"
progress 100 "P8 complete — evaluation output ready"
