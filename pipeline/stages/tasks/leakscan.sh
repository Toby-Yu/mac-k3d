#!/usr/bin/env bash
# tasks/leakscan: the mounted iCode tree must not contain any selected task's
# gold patch. Smoke runs warn; OFFICIAL=1 stops here.
set -euo pipefail
# shellcheck source=../_common.sh
source "$(cd "$(dirname "$0")/.." && pwd)/_common.sh"

TASKS_DIR="$(benchmark_tasks_dir)"
ensure_selected_tasks
HOST_ICODE="$(icode_host_root)"
PROTOCOL_INPUTS="$WORKDIR/eval_protocol_inputs.json"
[ -f "$PROTOCOL_INPUTS" ] || die "run tasks/isolation first (missing $PROTOCOL_INPUTS)"

LEAKSCAN_OUT="$WORKDIR/anticheat_leakscan.json"
set +e
python3 "$PIPELINE_LIB/anticheat_leakscan.py" \
  --tree "$HOST_ICODE" \
  --tasks-dir "$TASKS_DIR" \
  --selected "$WORKDIR/selected_tasks.txt" \
  --out "$LEAKSCAN_OUT"
leak_rc=$?
set -e
if [ "$leak_rc" = 0 ] || [ "$leak_rc" = 2 ]; then
  python3 "$PIPELINE_LIB/provenance.py" record-leakscan --inputs "$PROTOCOL_INPUTS" --report "$LEAKSCAN_OUT"
fi
case "$leak_rc" in
  0) echo "OK leak scan: no task gold in $HOST_ICODE" ;;
  2)
    if official_run; then
      die "leak scan found task gold in the mounted iCode tree (see $LEAKSCAN_OUT); OFFICIAL=1 stops here"
    fi
    echo "WARNING: leak scan found task gold in the mounted iCode tree (see $LEAKSCAN_OUT). Smoke run continues; OFFICIAL=1 would stop."
    ;;
  *) die "leak scanner failed (exit $leak_rc)" ;;
esac
