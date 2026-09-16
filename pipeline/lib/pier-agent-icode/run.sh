#!/usr/bin/env bash
# Run iCode inside the Pier sandbox using the Harbor/LoLBench CLI that works:
#   icode -p <project> run -t <task.md> -C <repo> -a code --json
set -euo pipefail

INSTRUCTION="${INSTRUCTION_FILE:-/tmp/instruction.md}"
if [ ! -f "$INSTRUCTION" ]; then
  for cand in \
    /instruction.md \
    /task/instruction.md \
    "$PWD/instruction.md" \
    /workspace/instruction.md \
    /app/instruction.md
  do
    if [ -f "$cand" ]; then
      INSTRUCTION="$cand"
      break
    fi
  done
fi

[ -f "$INSTRUCTION" ] || {
  echo "instruction.md not found" >&2
  exit 1
}

ICODE_BIN="${ICODE_BIN:-icode}"
OUT_DIR="${AGENT_OUTPUT_DIR:-/logs/agent}"
mkdir -p "$OUT_DIR" "$OUT_DIR/icode-project" /logs/artifacts 2>/dev/null || mkdir -p "$OUT_DIR" "$OUT_DIR/icode-project"

# Harbor defaults: without these, the binary cannot talk to DeepSeek.
export DEEPSEEK_API_KEY="${DEEPSEEK_API_KEY:-}"
export DEEPSEEK_MODEL="${DEEPSEEK_MODEL:-deepseek-v4-pro}"
if [ -z "${ICODE_API_BASE:-}" ]; then
  export ICODE_API_BASE="https://api.deepseek.com"
fi
if [ -z "${ICODE_PROVIDER:-}" ]; then
  export ICODE_PROVIDER="DeepSeek"
fi
if [ -z "${ICODE_MODEL:-}" ]; then
  export ICODE_MODEL="$DEEPSEEK_MODEL"
fi
[ -n "$DEEPSEEK_API_KEY" ] || echo "WARNING: DEEPSEEK_API_KEY empty" >&2

REPO=""
for cand in /app /workspace "$PWD"; do
  if [ -d "$cand/.git" ]; then
    REPO="$cand"
    break
  fi
done
if [ -z "$REPO" ]; then
  found="$(find /app /workspace . -maxdepth 2 -type d -name .git 2>/dev/null | head -1 || true)"
  if [ -n "$found" ]; then
    REPO="$(dirname "$found")"
  fi
fi
REPO="${REPO:-$PWD}"
cd "$REPO"

STARTED_AT="$(date -u +%Y-%m-%dT%H:%M:%SZ)"
SECONDS=0

set +e
"$ICODE_BIN" -p "$OUT_DIR/icode-project" \
  run -t "$INSTRUCTION" -C "$REPO" -a code --json \
  >"$OUT_DIR/icode.txt" 2>"$OUT_DIR/icode.stderr"
rc=$?
set -e

if [ -f "$OUT_DIR/icode.txt" ]; then
  cp -f "$OUT_DIR/icode.txt" "$OUT_DIR/icode.json" 2>/dev/null || true
fi

USAGE_PROMPT=0
USAGE_COMPLETION=0
USAGE_TOTAL=0
if command -v python3 >/dev/null 2>&1 && [ -f "$OUT_DIR/icode.txt" ]; then
  usage_eval="$(
    python3 - "$OUT_DIR/icode.txt" <<'PY' || true
import json, sys
from pathlib import Path
text = Path(sys.argv[1]).read_text(encoding="utf-8", errors="replace")
tail = text[-262144:]
last = None
dec = json.JSONDecoder()
i = 0
while i < len(tail):
    start = tail.find("{", i)
    if start < 0:
        break
    try:
        obj, end = dec.raw_decode(tail, start)
    except json.JSONDecodeError:
        i = start + 1
        continue
    usage = obj.get("usage") if isinstance(obj, dict) else None
    if isinstance(usage, dict) and ("input_tokens" in usage or "prompt_tokens" in usage):
        last = usage
    i = end if end > start else start + 1
prompt = completion = total = 0
if last:
    prompt = int(last.get("input_tokens") or last.get("prompt_tokens") or 0)
    completion = int(last.get("output_tokens") or last.get("completion_tokens") or 0)
    total = int(last.get("total_tokens") or last.get("total") or (prompt + completion))
print(f"USAGE_PROMPT={prompt}")
print(f"USAGE_COMPLETION={completion}")
print(f"USAGE_TOTAL={total}")
PY
  )"
  eval "$usage_eval" || true
fi

DURATION="$SECONDS"
FINISHED_AT="$(date -u +%Y-%m-%dT%H:%M:%SZ)"
cat >"$OUT_DIR/timing.json" <<EOF
{"started_at":"$STARTED_AT","finished_at":"$FINISHED_AT","duration_seconds":$DURATION,"llm_model_id":"$ICODE_MODEL","exit_code":$rc,"token_usage":{"prompt":$USAGE_PROMPT,"completion":$USAGE_COMPLETION,"total":$USAGE_TOTAL}}
EOF
cat >"$OUT_DIR/icode-usage.json" <<EOF
{"prompt":$USAGE_PROMPT,"completion":$USAGE_COMPLETION,"total":$USAGE_TOTAL,"input_tokens":$USAGE_PROMPT,"output_tokens":$USAGE_COMPLETION,"total_tokens":$USAGE_TOTAL}
EOF
if [ -d /logs/artifacts ]; then
  for f in icode.stderr icode.txt icode.json icode-usage.json timing.json; do
    [ -f "$OUT_DIR/$f" ] && cp -f "$OUT_DIR/$f" "/logs/artifacts/$f" 2>/dev/null || true
  done
fi
echo "icode_usage prompt_tokens=$USAGE_PROMPT completion_tokens=$USAGE_COMPLETION total_tokens=$USAGE_TOTAL"

if command -v git >/dev/null 2>&1 && git rev-parse --is-inside-work-tree >/dev/null 2>&1; then
  git diff >"$OUT_DIR/agent.patch" || true
  if [ -d /logs/artifacts ]; then
    git diff >"/logs/artifacts/agent.patch" || true
  fi
fi

echo "icode agent finished rc=$rc model=$ICODE_MODEL dur_s=$DURATION"
exit "$rc"
