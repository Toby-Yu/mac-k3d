#!/usr/bin/env bash
# P4 — Pier agent icode package present
set -euo pipefail
source "$(cd "$(dirname "$0")" && pwd)/_common.sh"

progress 45 "P4: Pier agent icode"

[ -d "$PIER_AGENT_DIR" ] || die "missing $PIER_AGENT_DIR"
[ -f "$PIER_AGENT_DIR/install.sh" ] || die "missing install.sh"
[ -f "$PIER_AGENT_DIR/agent.toml" ] || die "missing agent.toml"
chmod +x "$PIER_AGENT_DIR/install.sh" "$PIER_AGENT_DIR/run.sh" 2>/dev/null || true

# Prefer Pier's agent discovery if available; otherwise validate package layout.
if pier agents 2>/dev/null | grep -qi icode; then
  echo "OK pier lists agent icode"
elif pier --help 2>&1 | grep -qi agent; then
  echo "OK pier present; agent package at $PIER_AGENT_DIR (register via --agent-dir or PIER_AGENTS_PATH)"
else
  echo "OK agent package layout validated at $PIER_AGENT_DIR"
fi

echo "$PIER_AGENT_DIR" >"$WORKDIR/pier_agent_dir.txt"
progress 50 "P4 complete"
