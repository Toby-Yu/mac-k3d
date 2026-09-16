#!/usr/bin/env bash
# P6 — baseline DeepSeek API (no iCode)
set -euo pipefail
source "$(cd "$(dirname "$0")" && pwd)/_common.sh"

progress 75 "P6: DeepSeek baseline arm (benchmark=$BENCHMARK n=$N_TASKS)"

TASKS_DIR="$(benchmark_tasks_dir)"
[ -d "$TASKS_DIR" ] || die "run P2 first (missing $TASKS_DIR)"
[ -n "${DEEPSEEK_API_KEY:-}" ] || die "$(missing_deepseek_key_hint)"

ensure_selected_tasks
mkdir -p "$BASELINE_DIR"
python3 "$PIPELINE_LIB/baseline_deepseek.py" \
  --tasks-dir "$TASKS_DIR" \
  --n-tasks "$N_TASKS" \
  --task-file "$WORKDIR/selected_tasks.txt" \
  --out-dir "$BASELINE_DIR" \
  --model "${DEEPSEEK_MODEL:-deepseek-v4-pro}" \
  | tee "$BASELINE_DIR/baseline.log"

progress 85 "P6 complete"
