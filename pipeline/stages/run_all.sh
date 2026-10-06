#!/usr/bin/env bash
# Full local eval: P0–P8.
#
# MAC_K3D_PHASE splits the run so Jenkins can hold its CPU lock for the rollouts
# only. All state lives under $WORKDIR, so the phases can run as separate steps:
#   prepare   P0–P4  clone, build iCode, prepare tasks  (no lock needed)
#   evaluate  P5     canary + one harbor run            (locked to this node)
#   report    P7–P8  score, anti-cheat, artifact        (no lock needed)
#   all       every phase, the default and what a local run uses
set -euo pipefail
DIR="$(cd "$(dirname "$0")" && pwd)"
source "$DIR/_common.sh"

PHASE="$(printf '%s' "${MAC_K3D_PHASE:-all}" | tr '[:upper:]' '[:lower:]')"
case "$PHASE" in
  all | prepare | evaluate | report) ;;
  *) die "MAC_K3D_PHASE must be all, prepare, evaluate or report (got $PHASE)" ;;
esac

run_phase() {
  [ "$PHASE" = all ] || [ "$PHASE" = "$1" ]
}

if run_phase prepare; then
  progress 0 "full local eval starting (n=$N_TASKS harness=$HARNESS llm=$LLM benchmark=$BENCHMARK)"
  case "${BENCHMARK:-deepswe}" in
    swebenchpro)
      if [ ! -f "$PIPELINE_LIB/swebenchpro_tasks.py" ]; then
        die "pipeline at $MAC_K3D_ROOT is too old for swebenchpro (missing pipeline/lib/swebenchpro_tasks.py). This worker's mac-k3d binary predates it: redeploy the binary (bash scripts/redeploy.sh from your checkout) and rebuild."
      fi
      ;;
  esac
  bash "$DIR/p0_prereqs.sh"
  bash "$DIR/p1_pier.sh"
  bash "$DIR/p2_deepswe.sh"
  bash "$DIR/p3_icode.sh"
  bash "$DIR/p4_agent.sh"
fi

if run_phase evaluate; then
  bash "$DIR/p5_harness.sh"
  if [ "$(canary_mode || true)" = only ]; then
    progress 100 "canary only: no rollouts to score (report $WORKDIR/canary/jenkins-${BUILD_NUMBER:-local}/report.md)"
    exit 0
  fi
fi

if run_phase report; then
  # A canary-only evaluate phase exits above; a separate report phase has to
  # check for itself that there is something to score.
  if [ "$(canary_mode || true)" = only ]; then
    progress 100 "canary only: no rollouts to score (report $WORKDIR/canary/jenkins-${BUILD_NUMBER:-local}/report.md)"
    exit 0
  fi
  bash "$DIR/p7_score.sh"
  bash "$DIR/p8_output.sh"
  progress 100 "full local eval finished"
  if [ -f "$WORKDIR/last_output.txt" ]; then
    echo "RESULT $(cat "$WORKDIR/last_output.txt")"
  fi
fi
