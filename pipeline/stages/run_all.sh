#!/usr/bin/env bash
# Full local eval: P0–P8
set -euo pipefail
DIR="$(cd "$(dirname "$0")" && pwd)"
source "$DIR/_common.sh"

progress 0 "full local eval starting (n=$N_TASKS harness=$HARNESS llm=$LLM benchmark=$BENCHMARK)"
bash "$DIR/p0_prereqs.sh"
bash "$DIR/p1_pier.sh"
bash "$DIR/p2_deepswe.sh"
if [ "${BENCHMARK:-deepswe}" = "lolbench" ]; then
  bash "$DIR/p3_icode.sh"
  echo "P4 skipped: LoLBench uses Harbor, not the Pier icode adapter"
else
  bash "$DIR/p3_icode.sh"
  bash "$DIR/p4_agent.sh"
fi
bash "$DIR/p5_harness.sh"
bash "$DIR/p6_baseline.sh"
bash "$DIR/p7_score.sh"
bash "$DIR/p8_output.sh"
progress 100 "full local eval finished"
if [ -f "$WORKDIR/last_output.txt" ]; then
  echo "RESULT $(cat "$WORKDIR/last_output.txt")"
fi
