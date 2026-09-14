#!/usr/bin/env bash
# P1 — ensure Pier on PATH
set -euo pipefail
source "$(cd "$(dirname "$0")" && pwd)/_common.sh"

progress 15 "P1: ensuring pier"

if ! have pier; then
  have uv || {
    echo "Installing uv…"
    curl -LsSf https://astral.sh/uv/install.sh | sh
    export PATH="${HOME}/.local/bin:${PATH}"
  }
  echo "Installing datacurve-pier via uv tool…"
  uv tool install datacurve-pier 2>/dev/null \
    || uv tool install "git+https://github.com/datacurve-ai/pier"
  export PATH="${HOME}/.local/bin:${PATH}"
fi

have pier || die "pier still not on PATH after install"
pier --help >/dev/null || pier -h >/dev/null || true
PIER_RUN_HELP="$(pier run --help 2>&1 || true)"
echo "$PIER_RUN_HELP" | grep -q -- '--agent-import-path' \
  || die "this pier has no --agent-import-path; need datacurve-pier 0.3.x (uv tool install datacurve-pier)"
echo "OK pier=$(command -v pier) (--agent-import-path present)"
progress 20 "P1 complete"
