#!/usr/bin/env bash
# P5 canary only (P0.6): run the isolation canary on every selected task, no rollouts.
# Uses an existing eval workspace (P1-P3 already ran there). Needs DEEPSEEK_API_KEY
# in the environment or .local.env, because the canary gets iCode's exact environment;
# it never calls the model.
#
#   MAC_K3D_EVAL_WORKDIR=~/jenkins-agent/workspace/lolbench_one_task/eval-runs \
#     BENCHMARK=lolbench TASKS=cpython_5,fastapi_1 bash pipeline/stages/p5c_canary.sh
#
# CANARY_ALLOW_HOST=github.com opens one host for the canary alone; it must then fail.
set -euo pipefail
export CANARY=only
exec bash "$(cd "$(dirname "$0")" && pwd)/p5_harness.sh" "$@"
