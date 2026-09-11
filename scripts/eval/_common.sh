#!/usr/bin/env bash
# Shared env for eval stage scripts.
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
export MAC_K3D_ROOT="$ROOT"
export WORKDIR="${MAC_K3D_EVAL_WORKDIR:-$ROOT/eval-work}"
export OUTPUT_DIR="${MAC_K3D_EVAL_OUTPUT:-$WORKDIR/output}"
export DEEPSWE_DIR="${DEEPSWE_DIR:-$WORKDIR/deep-swe}"
export ICODE_MODE="${ICODE_MODE:-source}"
export ICODE_SOURCE="${ICODE_SOURCE:-$HOME/Documents/Toby/iCode-main}"
export ICODE_RELEASE="${ICODE_RELEASE:-}"
export N_TASKS="${N_TASKS:-1}"
export HARNESS="${HARNESS:-icode}"
export LLM="${LLM:-deepseek}"
export BENCHMARK="${BENCHMARK:-deepswe}"
export PIER_AGENT_DIR="$ROOT/eval/pier-agent-icode"
export BASELINE_DIR="$WORKDIR/baseline"
export HARNESS_DIR="$WORKDIR/harness"
export RESULTS_DIR="$WORKDIR/results"

mkdir -p "$WORKDIR" "$OUTPUT_DIR" "$BASELINE_DIR" "$HARNESS_DIR" "$RESULTS_DIR"

progress() {
  local pct="$1"
  shift
  echo "PROGRESS ${pct}% $*"
}

die() {
  echo "ERROR: $*" >&2
  exit 1
}

have() {
  command -v "$1" >/dev/null 2>&1
}
