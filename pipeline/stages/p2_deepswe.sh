#!/usr/bin/env bash
# P2 — clone DeepSWE
set -euo pipefail
source "$(cd "$(dirname "$0")" && pwd)/_common.sh"

progress 25 "P2: DeepSWE clone"

have git || die "git required"
if [ ! -d "$DEEPSWE_DIR/tasks" ]; then
  rm -rf "$DEEPSWE_DIR"
  git clone --depth 1 https://github.com/datacurve-ai/deep-swe "$DEEPSWE_DIR"
fi
[ -d "$DEEPSWE_DIR/tasks" ] || die "missing $DEEPSWE_DIR/tasks after clone"
TASK_COUNT="$(find "$DEEPSWE_DIR/tasks" -mindepth 1 -maxdepth 1 -type d | wc -l | tr -d ' ')"
write_selected_tasks
echo "OK deep-swe tasks dir ($TASK_COUNT task dirs); selected $(tr '\n' ' ' <"$WORKDIR/selected_tasks.txt")"
progress 30 "P2 complete"
