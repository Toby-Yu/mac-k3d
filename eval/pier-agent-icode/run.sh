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
[ -n "$DEEPSEEK_API_KEY" ] || echo "WARNING: DEEPSEEK_API_KEY empty" >&2

# Headless-style invocation: pass instruction as prompt / task.
# Flags differ across iCode versions; try common forms then fall back.
set +e
if "$ICODE_BIN" run --help >/dev/null 2>&1; then
  "$ICODE_BIN" run "$PROMPT" --json >"$OUT_DIR/icode.json" 2>"$OUT_DIR/icode.stderr"
  rc=$?
elif "$ICODE_BIN" --help 2>&1 | grep -q -- '--task'; then
  "$ICODE_BIN" --task "$PROMPT" >"$OUT_DIR/icode.out" 2>"$OUT_DIR/icode.stderr"
  rc=$?
else
  printf '%s\n' "$PROMPT" | "$ICODE_BIN" >"$OUT_DIR/icode.out" 2>"$OUT_DIR/icode.stderr"
  rc=$?
fi
set -e

# Best-effort: if the agent wrote a git diff, save as patch
if command -v git >/dev/null 2>&1 && git rev-parse --is-inside-work-tree >/dev/null 2>&1; then
  git diff >"$OUT_DIR/agent.patch" || true
fi

echo "icode agent finished rc=$rc"
exit "$rc"
