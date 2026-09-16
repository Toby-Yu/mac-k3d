#!/usr/bin/env bash
# P2 — clone the selected benchmark (DeepSWE or LoLBench-Preview)
set -euo pipefail
source "$(cd "$(dirname "$0")" && pwd)/_common.sh"

progress 25 "P2: clone benchmark=$BENCHMARK"

have git || die "git required"
case "${BENCHMARK:-deepswe}" in
  lolbench)
    if [ ! -d "$LOLBENCH_DIR/harbor_tasks" ]; then
      rm -rf "$LOLBENCH_DIR"
      git clone --depth 1 "$LOLBENCH_GIT_URL" "$LOLBENCH_DIR"
    fi
    [ -d "$LOLBENCH_DIR/harbor_tasks" ] || die "missing $LOLBENCH_DIR/harbor_tasks after clone"
    ;;
  deepswe | "")
    if [ ! -d "$DEEPSWE_DIR/tasks" ]; then
      rm -rf "$DEEPSWE_DIR"
      git clone --depth 1 https://github.com/datacurve-ai/deep-swe "$DEEPSWE_DIR"
    fi
    [ -d "$DEEPSWE_DIR/tasks" ] || die "missing $DEEPSWE_DIR/tasks after clone"
    ;;
  *)
    die "unknown BENCHMARK=${BENCHMARK} (use deepswe or lolbench)"
    ;;
esac

TASKS_DIR="$(benchmark_tasks_dir)"
TASK_COUNT="$(find "$TASKS_DIR" -mindepth 1 -maxdepth 1 -type d | wc -l | tr -d ' ')"
write_selected_tasks
echo "OK $BENCHMARK tasks dir ($TASK_COUNT task dirs); selected $(tr '\n' ' ' <"$WORKDIR/selected_tasks.txt")"
progress 30 "P2 complete"
