#!/usr/bin/env bash
# P0 — Docker + mac-k3d CLI readiness
set -euo pipefail
# shellcheck source=./_common.sh
source "$(cd "$(dirname "$0")" && pwd)/_common.sh"

progress 5 "P0: checking Docker and mac-k3d"

have docker || die "docker not on PATH"
docker info >/dev/null 2>&1 || die "docker info failed (no Server / permission). Log out/in on Linux or open Docker Desktop on macOS."

MAC_K3D_BIN="${MAC_K3D_BIN:-}"
if [ -z "$MAC_K3D_BIN" ]; then
  if have mac-k3d; then
    MAC_K3D_BIN="$(command -v mac-k3d)"
  elif [ -x "$MAC_K3D_ROOT/target/release/mac-k3d" ]; then
    MAC_K3D_BIN="$MAC_K3D_ROOT/target/release/mac-k3d"
  else
    die "mac-k3d not found; build with cargo build --release or put Release asset on PATH"
  fi
fi

"$MAC_K3D_BIN" --help | grep -q eval || die "mac-k3d --help does not list eval (rebuild from this branch)"

echo "OK docker Server section present"
echo "OK mac-k3d=$MAC_K3D_BIN lists eval"

if have systemctl; then
  if systemctl --user is-active mac-k3d-jenkins-agent.service >/dev/null 2>&1; then
    echo "OK Jenkins agent unit active"
  else
    echo "NOTE: Jenkins agent unit not active (OK for --local stage tests)"
  fi
fi

progress 10 "P0 complete"
