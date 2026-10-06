#!/usr/bin/env bash
# tasks/icode: resolve the iCode under test (release drop, or git clone + build).
# Reads the checkout; never changes iCode's source.
set -euo pipefail
# shellcheck source=../_common.sh
source "$(cd "$(dirname "$0")/.." && pwd)/_common.sh"
# shellcheck source=../../lib/icode_input.sh
source "$PIPELINE_LIB/icode_input.sh"

ICODE_MODE="$(icode_normalize_mode)"
export ICODE_MODE
icode_apply_official_pin
echo "icode: resolving iCode ($ICODE_MODE)"

# Do not reuse git identity from an earlier git-mode run in this WORKDIR.
rm -f "$WORKDIR/icode_git.json"

ICODE_BIN=""
case "$ICODE_MODE" in
  release)
    ICODE_BIN="$(get_release_icode)"
    ;;
  git)
    ICODE_BIN="$(get_bin_icode)"
    ;;
  *)
    die "unknown ICODE_MODE=$ICODE_MODE (use git clone or release binary drop)"
    ;;
esac

[ -n "$ICODE_BIN" ] || die "tasks/icode did not resolve ICODE_BIN"
export ICODE_BIN
echo "$ICODE_BIN" >"$WORKDIR/icode_bin_path.txt"

HOST_ROOT=""
if [ -s "$WORKDIR/icode_host_root.txt" ]; then
  HOST_ROOT="$(cat "$WORKDIR/icode_host_root.txt")"
fi

# Git wrapper execs /opt/icode-host/.venv/bin/icode (sandbox path). Probe the venv on the host.
if [ -n "$HOST_ROOT" ] && [ -x "$HOST_ROOT/.venv/bin/icode" ]; then
  "$HOST_ROOT/.venv/bin/icode" --help >/dev/null
elif [[ "$ICODE_BIN" == uv\ run* ]]; then
  # shellcheck disable=SC2086
  eval "$ICODE_BIN --help" >/dev/null
else
  "$ICODE_BIN" --help >/dev/null
fi
echo "OK icode helps ($ICODE_BIN)"
if [ -s "$WORKDIR/icode_git.json" ]; then
  python3 -c 'import json,sys; d=json.load(open(sys.argv[1],encoding="utf-8")); print("OK iCode git kind={kind} ref={ref} sha={sha}".format(**d))' "$WORKDIR/icode_git.json" || true
  cp "$WORKDIR/icode_git.json" "$OUTPUT_DIR/icode_git.json"
fi
if [ "$ICODE_MODE" = "git" ] && [ -n "${HOST_ROOT:-}" ]; then
  icode_assert_clean_checkout "$HOST_ROOT"
fi
icode_assert_recorded_sha
