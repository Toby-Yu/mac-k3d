#!/usr/bin/env bash
# tasks/agent: Harbor's Python can import the iCode and patch agents (no LLM).
set -euo pipefail
# shellcheck source=../_common.sh
source "$(cd "$(dirname "$0")/.." && pwd)/_common.sh"

[ -f "$PIPELINE_LIB/icode_harbor_agent.py" ] || die "missing $PIPELINE_LIB/icode_harbor_agent.py"
[ -f "$PIPELINE_LIB/icode_capture.sh" ] || die "missing $PIPELINE_LIB/icode_capture.sh"
have harbor || die "harbor not on PATH (run the env phase)"

py="python3"
shebang="$(head -n 1 "$(command -v harbor)" 2>/dev/null || true)"
case "$shebang" in
  "#!"*)
    py="${shebang:2}"
    py="${py#"${py%%[![:space:]]*}"}"
    ;;
esac
PYTHONPATH="$PIPELINE_LIB${PYTHONPATH:+:$PYTHONPATH}" "$py" -c \
  "from icode_harbor_agent import CAPTURE_SRC, ICodeAgent; from patch_harbor_agent import PatchAgent; assert ICodeAgent.name() == 'icode' and CAPTURE_SRC.is_file()" \
  || die "icode_harbor_agent:ICodeAgent, patch_harbor_agent:PatchAgent or icode_capture.sh missing"

echo "OK Harbor adapter $PIPELINE_LIB/icode_harbor_agent.py (icode_harbor_agent:ICodeAgent)"
