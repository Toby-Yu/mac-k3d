#!/usr/bin/env bash
# env/host: RAM, disk, Docker access, VPN MTU and the mac-k3d binary on this worker.
set -euo pipefail
# shellcheck source=../_common.sh
source "$(cd "$(dirname "$0")/.." && pwd)/_common.sh"

case "${BENCHMARK:-deepswe}" in
  deepswe | lolbench | swebenchpro | "") ;;
  *) die "unknown BENCHMARK=${BENCHMARK} (use deepswe, lolbench, or swebenchpro)" ;;
esac
check_canary_settings
ensure_eval_preflight

have docker || die "docker not on PATH"
docker info >/dev/null 2>&1 || die "docker info failed (no Server / permission). Log out/in on Linux or open Docker Desktop on macOS."
warn_docker_mtu

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
