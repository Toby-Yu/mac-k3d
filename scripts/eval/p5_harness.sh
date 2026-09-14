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
export DEEPSEEK_MODEL="${DEEPSEEK_MODEL:-deepseek-v4-pro}"
mkdir -p "$HARNESS_DIR"

FIRST_TASK="$(find "$DEEPSWE_DIR/tasks" -mindepth 1 -maxdepth 1 -type d | sort | head -n 1)"
[ -n "$FIRST_TASK" ] || die "no tasks under $DEEPSWE_DIR/tasks"

STARTED_AT="$(date -u +%Y-%m-%dT%H:%M:%SZ)"
SECONDS=0
set +e
if pier run --help 2>&1 | grep -q -- '--n-tasks'; then
  pier run -p "$DEEPSWE_DIR/tasks" \
    --agent icode \
    --model "${DEEPSEEK_MODEL}" \
    --n-tasks "$N_TASKS" \
    --agent-dir "$PIER_AGENT_DIR" \
    -o "$HARNESS_DIR" 2>&1 | tee "$HARNESS_DIR/pier.log"
  rc=${PIPESTATUS[0]:-1}
else
  pier run -p "$FIRST_TASK" \
    --agent icode \
    --model "${DEEPSEEK_MODEL}" \
    --agent-dir "$PIER_AGENT_DIR" \
    -o "$HARNESS_DIR" 2>&1 | tee "$HARNESS_DIR/pier.log"
  rc=${PIPESTATUS[0]:-1}
fi
set -e
DURATION="$SECONDS"
FINISHED_AT="$(date -u +%Y-%m-%dT%H:%M:%SZ)"

if [ "$rc" -ne 0 ]; then
  echo "WARNING: pier run exited $rc (see $HARNESS_DIR/pier.log). Stage still recorded."
fi
echo "$rc" >"$HARNESS_DIR/exit_code.txt"

python3 - "$HARNESS_DIR" "$DEEPSEEK_MODEL" "$STARTED_AT" "$FINISHED_AT" "$DURATION" "$rc" <<'PY'
import json, re, sys
from pathlib import Path

harness, model, started, finished, duration, rc = sys.argv[1:7]
root = Path(harness)
blob = ""
for p in root.rglob("*"):
    if p.is_file() and p.suffix in {".log", ".json", ".jsonl", ".txt"} and p.stat().st_size < 2_000_000:
        try:
            blob += p.read_text(encoding="utf-8", errors="ignore") + "\n"
        except OSError:
            pass

def first_int(*pats):
    for pat in pats:
        m = re.search(pat, blob, re.I)
        if m:
            return int(m.group(1))
    return 0

prompt = first_int(r'"prompt_tokens"\s*:\s*(\d+)', r"\bprompt_tokens[=:\s]+(\d+)")
completion = first_int(r'"completion_tokens"\s*:\s*(\d+)', r"\bcompletion_tokens[=:\s]+(\d+)")
total = first_int(r'"total_tokens"\s*:\s*(\d+)', r"\btotal_tokens[=:\s]+(\d+)")
if total == 0:
    total = prompt + completion
served = None
m = re.search(r'"model"\s*:\s*"([^"]+)"', blob)
if m:
    served = m.group(1)
meta = {
    "access_date_utc": started,
    "started_at": started,
    "finished_at": finished,
    "duration_seconds": int(duration),
    "llm_model_id": model,
    "llm_model_served": served,
    "exit_code": int(rc),
    "token_usage": {"prompt": prompt, "completion": completion, "total": total},
}
(root / "meta.json").write_text(json.dumps(meta, indent=2), encoding="utf-8")
PY

progress 70 "P5 complete (exit=$rc)"
