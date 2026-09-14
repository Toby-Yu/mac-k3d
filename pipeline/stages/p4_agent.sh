#!/usr/bin/env bash
# P4 — Pier agent icode package present
set -euo pipefail
source "$(cd "$(dirname "$0")" && pwd)/_common.sh"

progress 45 "P4: Pier agent icode"

[ -d "$PIER_AGENT_DIR" ] || die "missing $PIER_AGENT_DIR"
[ -f "$PIER_AGENT_DIR/install.sh" ] || die "missing install.sh"
[ -f "$PIER_AGENT_DIR/run.sh" ] || die "missing run.sh"
[ -f "$PIER_AGENT_DIR/agent.toml" ] || die "missing agent.toml"
[ -f "$PIPELINE_LIB/icode_pier_agent.py" ] || die "missing pipeline/lib/icode_pier_agent.py"
chmod +x "$PIER_AGENT_DIR/install.sh" "$PIER_AGENT_DIR/run.sh" 2>/dev/null || true

PIER_RUN_HELP="$(pier run --help 2>&1 || true)"
if echo "$PIER_RUN_HELP" | grep -q -- '--agent-import-path'; then
  echo "OK Pier 0.3.x adapter $PIPELINE_LIB/icode_pier_agent.py (--agent-import-path icode_pier_agent:ICodeAgent)"
elif pier agents 2>/dev/null | grep -qi icode; then
  echo "OK pier lists agent icode"
else
  echo "OK agent package layout validated at $PIER_AGENT_DIR"
fi

echo "$PIER_AGENT_DIR" >"$WORKDIR/pier_agent_dir.txt"
progress 50 "P4 complete"
