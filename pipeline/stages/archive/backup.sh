#!/usr/bin/env bash
# archive/backup: copy the run folder, anti-cheat verdicts and per-trial files
# into the repo's output/<benchmark>/ and pack them as <run folder>.tar.gz.
# MAC_K3D_BACKUP_ROOT or MAC_K3D_OUTPUT_ROOT moves the backup elsewhere.
set -euo pipefail
# shellcheck source=../_common.sh
source "$(cd "$(dirname "$0")/.." && pwd)/_common.sh"

ART_DIR="$(report_dir)"
BACKUP_ROOT="${MAC_K3D_BACKUP_ROOT:-${MAC_K3D_OUTPUT_ROOT:-$ROOT/output}}"
python3 "$PIPELINE_LIB/archive_run.py" \
  --report-dir "$ART_DIR" \
  --harness-dir "$HARNESS_DIR" \
  --task-file "$WORKDIR/selected_tasks.txt" \
  --suite "$BENCHMARK" \
  --backup-root "$BACKUP_ROOT"
echo "OK backup $BACKUP_ROOT/${BENCHMARK}/$(basename "$ART_DIR").tar.gz"
