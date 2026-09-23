#!/usr/bin/env bash
# P4 — Harbor agent icode imports (no LLM)
set -euo pipefail
source "$(cd "$(dirname "$0")" && pwd)/_common.sh"

progress 45 "P4: Harbor agent icode"

[ -f "$PIPELINE_LIB/icode_harbor_agent.py" ] || die "missing $PIPELINE_LIB/icode_harbor_agent.py"
have harbor || die "harbor not on PATH (run P1)"

py="python3"
shebang="$(head -n 1 "$(command -v harbor)" 2>/dev/null || true)"
case "$shebang" in
  "#!"*)
    py="${shebang:2}"
    py="${py#"${py%%[![:space:]]*}"}"
    ;;
esac
PYTHONPATH="$PIPELINE_LIB${PYTHONPATH:+:$PYTHONPATH}" "$py" -c \
  "from icode_harbor_agent import ICodeAgent; assert ICodeAgent.name() == 'icode'" \
  || die "icode_harbor_agent:ICodeAgent did not import"

echo "OK Harbor adapter $PIPELINE_LIB/icode_harbor_agent.py (icode_harbor_agent:ICodeAgent)"
progress 50 "P4 complete"
