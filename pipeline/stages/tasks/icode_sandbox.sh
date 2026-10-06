#!/usr/bin/env bash
# tasks/icode_sandbox: make the host iCode tree runnable inside a task container
# at /opt/icode-host. A git build gets the embedded CPython, the sanitized venv
# and the wrapper; a release drop keeps its own binary.
set -euo pipefail
# shellcheck source=../_common.sh
source "$(cd "$(dirname "$0")/.." && pwd)/_common.sh"
# shellcheck source=../../lib/icode_input.sh
source "$PIPELINE_LIB/icode_input.sh"

[ -s "$WORKDIR/icode_bin_path.txt" ] || die "no iCode in $WORKDIR; run tasks/icode first"
ICODE_BIN="$(cat "$WORKDIR/icode_bin_path.txt")"
[ -n "$ICODE_BIN" ] && [ -f "$ICODE_BIN" ] || die "tasks/icode did not resolve an icode binary (needed for Harbor bind-mount)"
HOST_ICODE="$(icode_host_root)"
[ -d "$HOST_ICODE" ] || die "icode host tree missing: $HOST_ICODE"
# Git clone has .venv/bin/icode. A release drop must keep its real binary:
# writing the git wrapper here overwrites it and Harbor then exits 127.
if [ -x "$HOST_ICODE/.venv/bin/icode" ]; then
  echo "icode_sandbox: git wrapper for $HOST_ICODE"
  icode_embed_sandbox_cpython "$HOST_ICODE"
  icode_write_sandbox_pth "$HOST_ICODE"
  icode_sanitize_host_tree "$HOST_ICODE"
  icode_write_wrapper "$HOST_ICODE/icode"
  icode_probe_sandbox "$HOST_ICODE"
else
  echo "icode_sandbox: keeping release binary at $HOST_ICODE/$(basename "${ICODE_BIN:-icode}") (no git wrapper)"
fi
