#!/usr/bin/env bash
# Shared env for eval stage scripts.
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
export MAC_K3D_ROOT="$ROOT"
export WORKDIR="${MAC_K3D_EVAL_WORKDIR:-$ROOT/eval-work}"
export OUTPUT_DIR="${MAC_K3D_EVAL_OUTPUT:-$WORKDIR/output}"
export DEEPSWE_DIR="${DEEPSWE_DIR:-$WORKDIR/deep-swe}"
export ICODE_MODE="${ICODE_MODE:-source}"
export ICODE_RELEASE="${ICODE_RELEASE:-}"
export N_TASKS="${N_TASKS:-1}"
export HARNESS="${HARNESS:-icode}"
export LLM="${LLM:-deepseek}"
export BENCHMARK="${BENCHMARK:-deepswe}"
export LLM_NAME="${LLM_NAME:-DeepSeek V4 Pro}"
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

missing_deepseek_key_hint() {
  echo "DEEPSEEK_API_KEY missing. Copy .env.example to .env (gitignored), set the key there, chmod 600 .env. Do not export the key in the terminal or paste it into chat. For Jenkins E7, bind credential deepseek-api-key."
}

_env_key_allowed() {
  case "$1" in
    DEEPSEEK_API_KEY | DEEPSEEK_MODEL | MAC_K3D_DEEPSEEK_API_KEY) return 0 ;;
    *) return 1 ;;
  esac
}

# KEY=value only. Does not source the file as shell.
_apply_env_line() {
  local line="$1" key value
  [[ "$line" =~ ^[[:space:]]*# ]] && return 0
  [[ -z "${line//[[:space:]]/}" ]] && return 0
  [[ "$line" == *=* ]] || return 0
  key="${line%%=*}"
  key="${key#"${key%%[![:space:]]*}"}"
  key="${key%"${key##*[![:space:]]}"}"
  _env_key_allowed "$key" || return 0
  value="${line#*=}"
  value="${value%$'\r'}"
  if [[ "$value" == \"*\" && "$value" == *\" ]]; then
    value="${value#\"}"
    value="${value%\"}"
  elif [[ "$value" == \'*\' && "$value" == *\' ]]; then
    value="${value#\'}"
    value="${value%\'}"
  fi
  if [ -z "${!key:-}" ]; then
    export "${key}=${value}"
  fi
}

# If DEEPSEEK_API_KEY is already set (Jenkins / rare export), leave it.
# Else load the first existing allowlisted file. Never print secret values.
load_local_env() {
  local f mode
  local candidates=()
  [ -n "${MAC_K3D_ENV_FILE:-}" ] && candidates+=("$MAC_K3D_ENV_FILE")
  candidates+=("$ROOT/.env")
  candidates+=("${XDG_CONFIG_HOME:-$HOME/.config}/mac-k3d/.env")

  for f in "${candidates[@]}"; do
    [ -n "$f" ] && [ -f "$f" ] || continue
    mode="$(stat -c '%a' "$f" 2>/dev/null || stat -f '%OLp' "$f" 2>/dev/null || echo "")"
    if [ -n "$mode" ] && [ "$mode" != "600" ] && [ "$mode" != "400" ]; then
      echo "WARNING: $f is mode $mode (expected 600). chmod 600 recommended." >&2
    fi
    while IFS= read -r line || [ -n "$line" ]; do
      _apply_env_line "$line"
    done <"$f"
    if [ -z "${DEEPSEEK_API_KEY:-}" ] && [ -n "${MAC_K3D_DEEPSEEK_API_KEY:-}" ]; then
      export DEEPSEEK_API_KEY="$MAC_K3D_DEEPSEEK_API_KEY"
    fi
    echo "OK loaded local env from $f (values not printed)"
    return 0
  done
}

load_local_env
export DEEPSEEK_MODEL="${DEEPSEEK_MODEL:-deepseek-v4-pro}"

_icode_tree_ok() {
  [ -d "$1" ] || return 1
  [ -x "$1/.venv/bin/icode" ] || [ -f "$1/pyproject.toml" ]
}

# Per-machine iCode path. Toby's lab tree is a candidate only if it exists.
discover_icode_source() {
  local c
  if [ -n "${ICODE_SOURCE:-}" ] && [ -d "$ICODE_SOURCE" ]; then
    echo "$ICODE_SOURCE"
    return 0
  fi
  for c in \
    "$HOME/Documents/iCode-main" \
    "$HOME/iCode-main" \
    "$HOME/src/iCode-main" \
    "$(dirname "$ROOT")/iCode-main" \
    "$HOME/Documents/Toby/iCode-main"
  do
    if _icode_tree_ok "$c"; then
      echo "$c"
      return 0
    fi
  done
  echo "$HOME/Documents/iCode-main"
}

if [ -z "${ICODE_SOURCE:-}" ]; then
  ICODE_SOURCE="$(discover_icode_source)"
fi
export ICODE_SOURCE

write_selected_tasks() {
  local list="$WORKDIR/selected_tasks.txt" n
  [ -d "$DEEPSWE_DIR/tasks" ] || die "run P2 first (missing deep-swe/tasks)"
  n="${N_TASKS:-1}"
  find "$DEEPSWE_DIR/tasks" -mindepth 1 -maxdepth 1 -type d | sort | head -n "$n" \
    | xargs -n1 basename >"$list"
  [ -s "$list" ] || die "no DeepSWE tasks to select under $DEEPSWE_DIR/tasks"
}

ensure_selected_tasks() {
  local list="$WORKDIR/selected_tasks.txt" have_n want
  want="${N_TASKS:-1}"
  if [ -f "$list" ]; then
    have_n="$(grep -c . "$list" || true)"
    if [ "$have_n" = "$want" ]; then
      return 0
    fi
  fi
  write_selected_tasks
}
