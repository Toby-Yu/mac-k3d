#!/usr/bin/env bash
# evaluate/canary: the isolation canary. One CanaryAgent trial per target task,
# with iCode's exact flags, mounts and env, probes what the agent can reach and
# read. A failure stops the build before any rollout. Never calls the model.
set -euo pipefail
# shellcheck source=../_common.sh
source "$(cd "$(dirname "$0")/.." && pwd)/_common.sh"
# shellcheck source=./harbor_cmd.sh
source "$(cd "$(dirname "$0")" && pwd)/harbor_cmd.sh"

check_canary_settings
CANARY_MODE="$(canary_mode)"
if [ "$CANARY_MODE" = off ]; then
  echo "canary: off (CANARY=${CANARY:-official}, OFFICIAL=${OFFICIAL:-0})"
  exit 0
fi
load_eval_state
write_harbor_env

# Host lists and gold file names for one task; never gold content.
canary_spec() {
  local tid="$1" spec="$CANARY_DIR/$1/canary_spec.json"
  mkdir -p "$CANARY_DIR/$tid"
  python3 "$PIPELINE_LIB/canary_verdict.py" spec \
    --task-dir "$TASKS_DIR/$tid" \
    --benchmark "${BENCHMARK:-deepswe}" \
    --allow-host "${CANARY_ALLOW_HOST:-}" \
    --out "$spec" >&2 || die "canary spec failed for $tid"
  printf '%s\n' "$spec"
}

if harbor_dry_run; then
  build_canary_cmd "${TASK_IDS[0]}" "$(canary_spec "${TASK_IDS[0]}")"
  echo "canary dry-run (cwd $unit_run_dir): $(masked_cmd)"
  exit 0
fi

ensure_harbor_egress
warn_docker_mtu
if [ "$CANARY_MODE" = only ]; then
  targets=("${TASK_IDS[@]}")
else
  targets=("${TASK_IDS[0]}")
fi
rm -rf "$CANARY_DIR"
mkdir -p "$CANARY_DIR"
if [ -n "${CANARY_ALLOW_HOST:-}" ]; then
  echo "WARNING: CANARY_ALLOW_HOST=$CANARY_ALLOW_HOST opens that host for the canary only; the canary must fail"
fi
expect=()
for tid in "${targets[@]}"; do
  spec="$(canary_spec "$tid")"
  build_canary_cmd "$tid" "$spec"
  echo "canary: task=$tid -a canary_harbor_agent:CanaryAgent (iCode's flags, mounts and env) log=$unit_log"
  set +e
  run_cmd >"$unit_log" 2>&1
  rc=$?
  set -e
  if [ "$rc" -ne 0 ]; then
    echo "WARNING: canary harbor run exited $rc for $tid (see $unit_log)"
  fi
  expect+=(--task "$tid")
done
set +e
python3 "$PIPELINE_LIB/canary_verdict.py" summarize \
  --jobs-dir "$CANARY_DIR" \
  --out-dir "$CANARY_DIR" \
  "${expect[@]}"
rc=$?
set -e
case "$rc" in
  0 | 2) ;;
  *) die "canary verdict failed (exit $rc)" ;;
esac
python3 "$PIPELINE_LIB/provenance.py" record-canary --inputs "$PROTOCOL_INPUTS" --summary "$CANARY_DIR/summary.json"
[ "$rc" = 0 ] || die "isolation canary failed (see $CANARY_DIR/report.md); the run stops here"
echo "canary: pass (${#targets[@]} tasks, report $CANARY_DIR/report.md)"
