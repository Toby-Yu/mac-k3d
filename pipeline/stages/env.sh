#!/usr/bin/env bash
# env phase: is this worker fit to run any evaluation? Nothing here depends on
# the selected tasks. No CPU lock.
set -euo pipefail
source "$(cd "$(dirname "$0")" && pwd)/_common.sh"

STEPS=(env/host env/compose env/harbor env/egress env/model_api)

# Written only by report/render and archive/backup; Jenkins archives whatever they name.
rm -f "$WORKDIR/last_output.txt" "$WORKDIR/last_backup.txt"
# A model key an older pipeline left behind; evaluate now removes its own copy.
rm -f "$WORKDIR/.harbor-env"

progress 0 "env: checking this worker (benchmark=$BENCHMARK n=$N_TASKS harness=$HARNESS llm=$LLM)"
run_steps "${STEPS[@]}"
progress 10 "env complete"
