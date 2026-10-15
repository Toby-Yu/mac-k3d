#!/usr/bin/env bash
# Run env_set_up checks 01 → 04, then 06 (harness builds, RUN_HARNESS_SMOKE=1).
set -euo pipefail
DIR="$(cd "$(dirname "$0")" && pwd)"
# shellcheck source=./_common.sh
source "$DIR/_common.sh"

echo "=== mac-k3d env_set_up (tag=${MAC_K3D_RELEASE_TAG} jenkins=${JENKINS_URL}) ==="

"$DIR/01_download_binary.sh"
"$DIR/02_check_controller.sh"
REQUIRE_WORKER="${REQUIRE_WORKER:-1}" "$DIR/03_check_worker.sh"
RUN_EVAL_SMOKE="${RUN_EVAL_SMOKE:-0}" "$DIR/04_check_eval_ready.sh"
RUN_HARNESS_SMOKE="${RUN_HARNESS_SMOKE:-0}" "$DIR/06_check_harness_prepare.sh"

echo
pass "ALL CHECKS PASSED"
echo "Next: mac-k3d eval --stage env   # then docs/testing/testing-eval-pipeline.md"
