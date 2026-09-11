#!/usr/bin/env bash
# P5 — one DeepSWE task through Pier + iCode agent
set -euo pipefail
source "$(cd "$(dirname "$0")" && pwd)/_common.sh"

progress 55 "P5: Pier+iCode harness arm (n=$N_TASKS)"

[ -d "$DEEPSWE_DIR/tasks" ] || die "run P2 first (missing deep-swe/tasks)"
[ -f "$WORKDIR/icode_bin_path.txt" ] || bash "$(dirname "$0")/p3_icode.sh"
[ -n "${DEEPSEEK_API_KEY:-}" ] || die "DEEPSEEK_API_KEY required for P5 (local) or bind deepseek-api-key in Jenkins"

export ICODE_BIN
ICODE_BIN="$(cat "$WORKDIR/icode_bin_path.txt")"
export PIER_AGENTS_PATH="${PIER_AGENTS_PATH:-$PIER_AGENT_DIR}"
mkdir -p "$HARNESS_DIR"

FIRST_TASK="$(find "$DEEPSWE_DIR/tasks" -mindepth 1 -maxdepth 1 -type d | sort | head -n 1)"
[ -n "$FIRST_TASK" ] || die "no tasks under $DEEPSWE_DIR/tasks"

set +e
if pier run --help 2>&1 | grep -q -- '--n-tasks'; then
  pier run -p "$DEEPSWE_DIR/tasks" \
    --agent icode \
    --model deepseek \
    --n-tasks "$N_TASKS" \
    --agent-dir "$PIER_AGENT_DIR" \
    -o "$HARNESS_DIR" 2>&1 | tee "$HARNESS_DIR/pier.log"
  rc=${PIPESTATUS[0]:-1}
else
  pier run -p "$FIRST_TASK" \
    --agent icode \
    --model deepseek \
    --agent-dir "$PIER_AGENT_DIR" \
    -o "$HARNESS_DIR" 2>&1 | tee "$HARNESS_DIR/pier.log"
  rc=${PIPESTATUS[0]:-1}
fi
set -e

if [ "$rc" -ne 0 ]; then
  echo "WARNING: pier run exited $rc (see $HARNESS_DIR/pier.log). Stage still recorded."
fi
echo "$rc" >"$HARNESS_DIR/exit_code.txt"
progress 70 "P5 complete (exit=$rc)"
