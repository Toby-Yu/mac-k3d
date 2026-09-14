#!/usr/bin/env bash
# Run iCode harness against the task instruction inside the Pier sandbox.
set -euo pipefail

INSTRUCTION="${INSTRUCTION_FILE:-/tests/../instruction.md}"
if [ ! -f "$INSTRUCTION" ]; then
  # Harbor/Pier layouts vary; search common paths
  for cand in \
    /instruction.md \
    /task/instruction.md \
    "$PWD/instruction.md" \
    /workspace/instruction.md
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

PROMPT="$(cat "$INSTRUCTION")"
ICODE_BIN="${ICODE_BIN:-icode}"
OUT_DIR="${AGENT_OUTPUT_DIR:-/logs/agent}"
mkdir -p "$OUT_DIR"

export DEEPSEEK_API_KEY="${DEEPSEEK_API_KEY:-}"
export DEEPSEEK_MODEL="${DEEPSEEK_MODEL:-deepseek-v4-pro}"
[ -n "$DEEPSEEK_API_KEY" ] || echo "WARNING: DEEPSEEK_API_KEY empty" >&2

STARTED_AT="$(date -u +%Y-%m-%dT%H:%M:%SZ)"
SECONDS=0

# Headless-style invocation: pass instruction as prompt / task.
# Flags differ across iCode versions; try common forms then fall back.
set +e
HELP_RUN="$("$ICODE_BIN" run --help 2>/dev/null || true)"
HELP_BIN="$("$ICODE_BIN" --help 2>/dev/null || true)"
if echo "$HELP_RUN" | grep -q -- '--json'; then
  if echo "$HELP_RUN" | grep -q -- '--model'; then
    "$ICODE_BIN" run "$PROMPT" --model "$DEEPSEEK_MODEL" --json >"$OUT_DIR/icode.json" 2>"$OUT_DIR/icode.stderr"
  else
    "$ICODE_BIN" run "$PROMPT" --json >"$OUT_DIR/icode.json" 2>"$OUT_DIR/icode.stderr"
  fi
  rc=$?
elif echo "$HELP_BIN" | grep -q -- '--task'; then
  if echo "$HELP_BIN" | grep -q -- '--model'; then
    "$ICODE_BIN" --task "$PROMPT" --model "$DEEPSEEK_MODEL" >"$OUT_DIR/icode.out" 2>"$OUT_DIR/icode.stderr"
  else
    "$ICODE_BIN" --task "$PROMPT" >"$OUT_DIR/icode.out" 2>"$OUT_DIR/icode.stderr"
  fi
  rc=$?
else
  printf '%s\n' "$PROMPT" | "$ICODE_BIN" >"$OUT_DIR/icode.out" 2>"$OUT_DIR/icode.stderr"
  rc=$?
fi
set -e

DURATION="$SECONDS"
FINISHED_AT="$(date -u +%Y-%m-%dT%H:%M:%SZ)"
cat >"$OUT_DIR/timing.json" <<EOF
{"started_at":"$STARTED_AT","finished_at":"$FINISHED_AT","duration_seconds":$DURATION,"llm_model_id":"$DEEPSEEK_MODEL","exit_code":$rc}
EOF

# Best-effort: if the agent wrote a git diff, save as patch
if command -v git >/dev/null 2>&1 && git rev-parse --is-inside-work-tree >/dev/null 2>&1; then
  git diff >"$OUT_DIR/agent.patch" || true
fi

echo "icode agent finished rc=$rc model=$DEEPSEEK_MODEL"
exit "$rc"
