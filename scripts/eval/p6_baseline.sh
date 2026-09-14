#!/usr/bin/env bash
# P6 — baseline DeepSeek API (no iCode)
set -euo pipefail
source "$(cd "$(dirname "$0")" && pwd)/_common.sh"

progress 75 "P6: DeepSeek baseline arm (n=$N_TASKS)"

[ -d "$DEEPSWE_DIR/tasks" ] || die "run P2 first"
[ -n "${DEEPSEEK_API_KEY:-}" ] || die "$(missing_deepseek_key_hint)"

mkdir -p "$BASELINE_DIR"
python3 "$MAC_K3D_ROOT/eval/baseline_deepseek.py" \
  --tasks-dir "$DEEPSWE_DIR/tasks" \
  --n-tasks "$N_TASKS" \
  --out-dir "$BASELINE_DIR" \
  --model "${DEEPSEEK_MODEL:-deepseek-v4-pro}" \
  | tee "$BASELINE_DIR/baseline.log"

progress 85 "P6 complete"
