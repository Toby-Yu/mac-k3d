#!/usr/bin/env bash
# Shared env for pipeline stage scripts (pipeline/stages).
set -euo pipefail

# Repo or share dir that contains pipeline/stages + pipeline/lib.
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
export MAC_K3D_ROOT="$ROOT"
export PIPELINE_LIB="$ROOT/pipeline/lib"
export PIPELINE_STAGES="$ROOT/pipeline/stages"
export WORKDIR="${MAC_K3D_EVAL_WORKDIR:-$ROOT/eval-runs}"
export OUTPUT_DIR="${MAC_K3D_EVAL_OUTPUT:-$WORKDIR/reports}"
export DEEPSWE_DIR="${DEEPSWE_DIR:-$WORKDIR/deep-swe}"
export ICODE_MODE="${ICODE_MODE:-binary}"
export ICODE_RELEASE="${ICODE_RELEASE:-}"
export N_TASKS="${N_TASKS:-1}"
export HARNESS="${HARNESS:-icode}"
export LLM="${LLM:-deepseek}"
export BENCHMARK="${BENCHMARK:-deepswe}"
export LLM_NAME="${LLM_NAME:-DeepSeek V4 Pro}"
export PIER_AGENT_DIR="$PIPELINE_LIB/pier-agent-icode"
export BASELINE_DIR="$WORKDIR/baseline"
export HARNESS_DIR="$WORKDIR/harness"
export RESULTS_DIR="$WORKDIR/results"
export MAC_K3D_SHARE="${MAC_K3D_SHARE:-${XDG_DATA_HOME:-$HOME/.local/share}/mac-k3d}"

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
  if [ -n "${JENKINS_URL:-}" ] || [ -n "${BUILD_ID:-}" ] || [ -n "${WORKSPACE:-}" ]; then
    echo "DEEPSEEK_API_KEY missing. On the Jenkins controller store credential id deepseek-api-key (mac-k3d setup / config). Workers do not use a local .env."
  else
    echo "DEEPSEEK_API_KEY missing. Developer local run: copy .env.example to .env (gitignored), chmod 600. Workers: use Jenkins job icode_eval (controller credential). Do not export the key or paste it into chat."
  fi
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
# Developer-only: workers should get the key from Jenkins credentials.
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

# Memory (GB) and disk (GB) fail-fast. Override with MAC_K3D_MIN_RAM_GB / MAC_K3D_MIN_DISK_GB.
ensure_eval_preflight() {
  local min_ram="${MAC_K3D_MIN_RAM_GB:-8}"
  local min_disk="${MAC_K3D_MIN_DISK_GB:-40}"
  local mem_kb mem_gb disk_kb disk_gb check_path
  mem_kb="$(awk '/MemTotal:/ {print $2}' /proc/meminfo 2>/dev/null || echo 0)"
  if [ "${mem_kb:-0}" -gt 0 ]; then
    mem_gb=$((mem_kb / 1024 / 1024))
    if [ "$mem_gb" -lt "$min_ram" ]; then
      die "only ${mem_gb} GB RAM; need at least ${min_ram} GB for eval (N=1). Free memory or use a larger machine."
    fi
    echo "OK RAM ${mem_gb} GB (min ${min_ram})"
  fi
  check_path="$WORKDIR"
  mkdir -p "$check_path"
  disk_kb="$(df -Pk "$check_path" 2>/dev/null | awk 'NR==2 {print $4}')"
  if [ -n "${disk_kb:-}" ] && [ "$disk_kb" -gt 0 ] 2>/dev/null; then
    disk_gb=$((disk_kb / 1024 / 1024))
    if [ "$disk_gb" -lt "$min_disk" ]; then
      die "only ${disk_gb} GB free at $check_path; need at least ${min_disk} GB. Free disk (docker system df) and keep N_TASKS=1."
    fi
    echo "OK disk ${disk_gb} GB free at $check_path (min ${min_disk})"
  fi
}

_icode_tree_ok() {
  [ -d "$1" ] || return 1
  [ -x "$1/.venv/bin/icode" ] || [ -f "$1/pyproject.toml" ]
}

_icode_gzip_magic() {
  local hex
  [ -f "$1" ] || return 1
  hex="$(od -An -tx1 -N2 "$1" 2>/dev/null | tr -d ' \n')"
  [ "$hex" = "1f8b" ]
}

_icode_ustar_magic() {
  [ -f "$1" ] || return 1
  dd if="$1" bs=1 skip=257 count=5 2>/dev/null | grep -q ustar
}

# Gzip/tar by suffix or magic (HTTP saves as icode-release.bin; browsers drop .tar.gz).
_icode_is_archive() {
  local base
  [ -f "$1" ] || return 1
  base="$(basename "$1")"
  case "$base" in
    *.tar.gz|*.tgz) return 0 ;;
  esac
  _icode_gzip_magic "$1" || _icode_ustar_magic "$1"
}

_icode_dir_has_icode() {
  local f
  [ -d "$1" ] || return 1
  [ -f "$1/icode" ] && return 0
  f="$(find "$1" -type f -name icode -print -quit 2>/dev/null || true)"
  [ -n "$f" ]
}

# Discoverable drop: icode file, archive, *-full-* file, or *-full-* dir with icode inside.
_icode_release_ok() {
  local p="$1" base
  [ -n "$p" ] || return 1
  if [ -d "$p" ]; then
    _icode_dir_has_icode "$p" || return 1
    [[ "$(basename "$p")" == *-full-* ]] || return 1
    return 0
  fi
  [ -f "$p" ] || return 1
  base="$(basename "$p")"
  [ "$base" = "icode" ] && return 0
  case "$base" in
    *.tar.gz|*.tgz) return 0 ;;
  esac
  [[ "$base" == *-full-* ]]
}

_icode_bin_or_tarball() {
  local p="$1"
  if _icode_release_ok "$p"; then
    echo "$p"
    return 0
  fi
  return 1
}

# Official user drop: icode-*-full-* first, then a file named icode. Fallback /opt/mac-k3d/.
discover_icode_release() {
  local c d
  if [ -n "${ICODE_RELEASE:-}" ]; then
    echo "$ICODE_RELEASE"
    return 0
  fi
  for d in "$MAC_K3D_SHARE" /opt/mac-k3d; do
    [ -d "$d" ] || continue
    for c in "$d"/*-full-*.tar.gz "$d"/*.tgz "$d"/*full*.tar.gz "$d"/*-full-*; do
      [ -e "$c" ] || continue
      if _icode_bin_or_tarball "$c"; then
        return 0
      fi
    done
    if _icode_bin_or_tarball "$d/icode"; then
      return 0
    fi
  done
  return 1
}

# Copy or unpack ICODE_RELEASE into $2. Explicit dirs need a file named icode (any folder name).
install_icode_release() {
  local src="$1" unpack="$2" base
  [ -e "$src" ] || die "ICODE_RELEASE not found: $src"
  if [ -d "$src" ]; then
    _icode_dir_has_icode "$src" || die "ICODE_RELEASE directory has no file named icode: $src"
    cp -a "$src"/. "$unpack"/
    return 0
  fi
  [ -f "$src" ] || die "ICODE_RELEASE not found: $src"
  if _icode_is_archive "$src"; then
    if _icode_gzip_magic "$src" || [[ "$(basename "$src")" == *.tar.gz || "$(basename "$src")" == *.tgz ]]; then
      tar -xzf "$src" -C "$unpack"
    else
      tar -xf "$src" -C "$unpack"
    fi
    return 0
  fi
  base="$(basename "$src")"
  if [ "$base" != "icode" ] && [[ "$base" != *-full-* ]]; then
    die "ICODE_RELEASE is not an icode binary or *-full-* release: $src"
  fi
  cp "$src" "$unpack/icode"
  chmod +x "$unpack/icode" || true
}

# Developer source tree. Toby's lab path is a candidate only if it exists.
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

if [ -z "${ICODE_RELEASE:-}" ]; then
  ICODE_RELEASE="$(discover_icode_release || true)"
fi
export ICODE_RELEASE

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
