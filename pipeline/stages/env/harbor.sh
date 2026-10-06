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
ensure_uv
echo "Installing harbor ${HARBOR_VERSION}…"
uv tool install "harbor==${HARBOR_VERSION}"
export PATH="${HOME}/.local/bin:${PATH}"
have harbor || die "harbor still not on PATH after install. Try: uv tool install \"harbor==${HARBOR_VERSION}\""
harbor --help >/dev/null || true
got="$(harbor --version 2>/dev/null | head -n 1 | sed 's/^[[:space:]]*//;s/[[:space:]]*$//')"
[ "$got" = "$HARBOR_VERSION" ] || die "harbor --version is '${got}', wanted ${HARBOR_VERSION}"
printf '%s\n' "$got" >"$WORKDIR/harbor_version.txt"
echo "OK harbor=$(command -v harbor) version=$got"
progress 20 "P1 complete"
