#!/usr/bin/env bash
# P2 — clone the selected benchmark (DeepSWE, LoLBench-Preview, or SWE-bench Pro)
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
  swebenchpro)
    if [ ! -f "$SWEBENCHPRO_DIR/src/swe_bench_pro_eval.py" ] \
      && [ ! -f "$SWEBENCHPRO_DIR/swe_bench_pro_eval.py" ]; then
      mkdir -p "$SWEBENCHPRO_DIR"
      rm -rf "$SWEBENCHPRO_DIR/src"
      git clone --depth 1 "$SWEBENCHPRO_GIT_URL" "$SWEBENCHPRO_DIR/src"
    fi
    python3 "$PIPELINE_LIB/swebenchpro_tasks.py" \
      --src-dir "$SWEBENCHPRO_DIR" \
      --out-dir "$SWEBENCHPRO_DIR/tasks" \
      --task "${TASK:-}" \
      --tasks "${TASKS:-}" \
      --n-tasks "${N_TASKS:-1}"
    [ -d "$SWEBENCHPRO_DIR/tasks" ] || die "missing $SWEBENCHPRO_DIR/tasks after materialize"
    ;;
  deepswe | "")
    if [ ! -d "$DEEPSWE_DIR/tasks" ]; then
      rm -rf "$DEEPSWE_DIR"
      git clone --depth 1 https://github.com/datacurve-ai/deep-swe "$DEEPSWE_DIR"
    fi
    [ -d "$DEEPSWE_DIR/tasks" ] || die "missing $DEEPSWE_DIR/tasks after clone"
    ;;
  *)
    die "unknown BENCHMARK=${BENCHMARK} (use deepswe, lolbench, or swebenchpro)"
    ;;
esac

TASKS_DIR="$(benchmark_tasks_dir)"
TASK_COUNT="$(find "$TASKS_DIR" -mindepth 1 -maxdepth 1 -type d | wc -l | tr -d ' ')"
write_selected_tasks
echo "OK $BENCHMARK tasks dir ($TASK_COUNT task dirs); selected $(tr '\n' ' ' <"$WORKDIR/selected_tasks.txt")"
progress 30 "P2 complete"
