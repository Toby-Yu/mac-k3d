#!/usr/bin/env bash
# P8 — write this run's artifact.json, summary.md, and report.html, and back them up.
set -euo pipefail
source "$(cd "$(dirname "$0")" && pwd)/_common.sh"

progress 98 "P8: evaluation report"

UTC="$(date -u +%Y%m%dT%H%M%SZ)"
eval_parallel_degree || die "N_ROLLOUTS and CPU_LOCK_QTY must be integers >= 1"
RUN_ID="local-${UTC}"
if [ -n "${BUILD_NUMBER:-}" ]; then
  RUN_ID="jenkins-${BUILD_NUMBER}"
fi

repo_output_root() {
  if [ -n "${MAC_K3D_BACKUP_ROOT:-}" ]; then
    printf '%s\n' "$MAC_K3D_BACKUP_ROOT"
    return
  fi
  if [ -n "${MAC_K3D_OUTPUT_ROOT:-}" ]; then
    printf '%s\n' "$MAC_K3D_OUTPUT_ROOT"
    return
  fi
  if [ -n "${MAC_K3D_ROOT:-}" ]; then
    printf '%s/output\n' "$MAC_K3D_ROOT"
    return
  fi
  local script_root
  script_root="$(cd "$(dirname "$0")/../.." && pwd)"
  printf '%s/output\n' "$script_root"
}

FOLDER="$(
  PYTHONPATH="$PIPELINE_LIB${PYTHONPATH:+:$PYTHONPATH}" python3 - "$UTC" "$WORKDIR/selected_tasks.txt" <<'PY'
import os
import sys
from pathlib import Path

from render_report import run_folder_name

utc, task_file = sys.argv[1], sys.argv[2]
ids = []
path = Path(task_file)
if path.is_file():
    ids = [ln.strip() for ln in path.read_text(encoding="utf-8").splitlines() if ln.strip()]
print(run_folder_name(utc, ids, os.environ.get("BUILD_NUMBER", "")))
PY
)"

ART_DIR="$WORKDIR/output/${BENCHMARK}/${FOLDER}"
BACKUP_ROOT="$(repo_output_root)"
export MAC_K3D_OUTPUT_ROOT="$BACKUP_ROOT"

API_BASE="${ICODE_API_BASE:-https://api.deepseek.com/v1}"
if [ -f "$WORKDIR/eval_protocol_inputs.json" ]; then
  protocol_base="$(
    python3 - "$WORKDIR/eval_protocol_inputs.json" <<'PY'
import json, sys
doc = json.load(open(sys.argv[1], encoding="utf-8"))
base = doc.get("api_base") if isinstance(doc, dict) else ""
print(base if isinstance(base, str) else "")
PY
  )"
  if [ -n "$protocol_base" ]; then
    API_BASE="$protocol_base"
  fi
fi

python3 "$PIPELINE_LIB/render_report.py" \
  --harness-dir "$HARNESS_DIR" \
  --baseline-dir "$BASELINE_DIR" \
  --task-file "$WORKDIR/selected_tasks.txt" \
  --suite "$BENCHMARK" \
  --model "${DEEPSEEK_MODEL:-deepseek-v4-pro}" \
  --api-base "$API_BASE" \
  --run-id "$RUN_ID" \
  --n-rollouts "${N_ROLLOUTS:-1}" \
  --concurrency "${EVAL_PARALLEL:-1}" \
  --cpus-each "${EVAL_CPUS_EACH:-1}" \
  --workdir "$WORKDIR" \
  --utc "$UTC" \
  --run-folder "$FOLDER" \
  --backup-root "$BACKUP_ROOT" \
  --out-dir "$ART_DIR"

rel="$ART_DIR"
if [ -n "${WORKSPACE:-}" ] && [[ "$ART_DIR" == "$WORKSPACE/"* ]]; then
  rel="${ART_DIR#"$WORKSPACE"/}"
fi
printf '%s/**\n' "$rel" >"$WORKDIR/last_output.txt"
echo "OK wrote $ART_DIR/artifact.json"
echo "OK backup $BACKUP_ROOT/${BENCHMARK}/${FOLDER}.tar.gz"
progress 100 "P8 complete — evaluation output ready"
