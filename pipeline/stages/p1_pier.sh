#!/usr/bin/env bash
# P1 — ensure Harbor for DeepSWE, LoLBench, and SWE-bench Pro
set -euo pipefail
source "$(cd "$(dirname "$0")" && pwd)/_common.sh"

ensure_uv() {
  have uv && return 0
  echo "Installing uv…"
  curl -LsSf https://astral.sh/uv/install.sh | sh
  export PATH="${HOME}/.local/bin:${PATH}"
  have uv || die "uv still not on PATH after install"
}

case "${BENCHMARK:-deepswe}" in
  deepswe | lolbench | swebenchpro | "")
    ;;
  *)
    die "unknown BENCHMARK=${BENCHMARK} (use deepswe, lolbench, or swebenchpro)"
    ;;
esac

progress 15 "P1: ensuring harbor"
if ! have harbor; then
  ensure_uv
  echo "Installing harbor via uv tool…"
  uv tool install harbor
  export PATH="${HOME}/.local/bin:${PATH}"
fi
have harbor || die "harbor still not on PATH after install. Try: uv tool install harbor"
harbor --help >/dev/null || true
echo "OK harbor=$(command -v harbor)"
progress 20 "P1 complete"
