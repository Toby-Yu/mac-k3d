#!/usr/bin/env bash
# Full local eval: P0–P8
set -euo pipefail
DIR="$(cd "$(dirname "$0")" && pwd)"
source "$DIR/_common.sh"

progress 0 "full local eval starting (n=$N_TASKS harness=$HARNESS llm=$LLM benchmark=$BENCHMARK)"
case "${BENCHMARK:-deepswe}" in
  swebenchpro)
    if [ ! -f "$PIPELINE_LIB/swebenchpro_tasks.py" ]; then
      die "pipeline at $MAC_K3D_ROOT is too old for swebenchpro (missing pipeline/lib/swebenchpro_tasks.py). On the worker install this CLI then: mac-k3d config -c ~/.config/mac-k3d/worker.yaml. Or set Jenkins MAC_K3D_ROOT to a checkout that contains those files."
    fi
    ;;
esac
bash "$DIR/p0_prereqs.sh"
bash "$DIR/p1_pier.sh"
bash "$DIR/p2_deepswe.sh"
bash "$DIR/p3_icode.sh"
bash "$DIR/p4_agent.sh"
bash "$DIR/p5_harness.sh"
bash "$DIR/p6_baseline.sh"
bash "$DIR/p7_score.sh"
bash "$DIR/p8_output.sh"
progress 100 "full local eval finished"
if [ -f "$WORKDIR/last_output.txt" ]; then
  echo "RESULT $(cat "$WORKDIR/last_output.txt")"
fi
