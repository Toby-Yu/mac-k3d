#!/usr/bin/env bash
# Verify this machine is ready to run a DeepSWE evaluation with iCode or
# JiuwenSwarm: the host tools the pipeline runs, Docker (memory, emulation,
# file sharing, the harness build image), the eval CLI and the controller.
set -euo pipefail
# shellcheck source=./_common.sh
source "$(cd "$(dirname "$0")" && pwd)/_common.sh"

require_mac_k3d

SHARE="${MAC_K3D_SHARE:-${XDG_DATA_HOME:-$HOME/.local/share}/mac-k3d}"
DARWIN=0
[ "$(uname -s)" = Darwin ] && DARWIN=1

# The pipeline's steps run under the bash and python3 first on PATH.
BASH_WANT="$(toolchain_value BASH_MIN)"
PY_WANT="$(toolchain_value PYTHON_MIN)"
BASH_GOT="$(bash -c 'echo "${BASH_VERSINFO[0]}.${BASH_VERSINFO[1]}"')"
if ! version_at_least "$BASH_GOT" "${BASH_WANT:-4.4}"; then
  if [ "$DARWIN" = 1 ]; then
    die "bash $BASH_GOT at $(command -v bash) is older than ${BASH_WANT:-4.4}: brew install bash (mac-k3d setup does this) and put $(brew --prefix 2>/dev/null || echo /opt/homebrew)/bin before /bin on PATH"
  fi
  die "bash $BASH_GOT at $(command -v bash) is older than ${BASH_WANT:-4.4}"
fi
pass "bash $BASH_GOT at $(command -v bash)"
have python3 || die "python3 not on PATH (need ${PY_WANT:-3.11}+)"
PY_GOT="$(python3 -c 'import sys; print("%d.%d" % sys.version_info[:2])')"
version_at_least "$PY_GOT" "${PY_WANT:-3.11}" \
  || die "python3 $PY_GOT at $(command -v python3) is older than ${PY_WANT:-3.11} (macOS: brew install python@3.11; mac-k3d setup does this)"
pass "python3 $PY_GOT at $(command -v python3)"
python3 -c 'import yaml' 2>/dev/null \
  || die "python3 $PY_GOT has no PyYAML: the report loses its question labels. macOS: brew install pyyaml; Linux: sudo apt install python3-yaml (mac-k3d setup does this)"
pass "python3 has PyYAML"
for tool in curl git; do
  have "$tool" || die "$tool not on PATH"
done
pass "curl and git on PATH"

have docker || die "docker not on PATH"
docker info >/dev/null 2>&1 || die "docker info failed (no Server / permission). Linux: log out/in. macOS: open Docker Desktop."
pass "docker Server section present"
bash "$MAC_K3D_ROOT/pipeline/stages/env/compose.sh"
pass "docker compose and buildx present"

# On a Mac the trials run in Docker Desktop's VM, which has less memory than the
# Mac and reports a little less than its Memory Limit. The env phase checks the same.
MIN_GB="${MAC_K3D_MIN_RAM_GB:-$(toolchain_value MIN_RAM_GB)}"
MEM_BYTES="$(docker info --format '{{.MemTotal}}' 2>/dev/null || echo 0)"
MEM_GB=$((MEM_BYTES / 1024 / 1024 / 1024))
if [ "$MEM_GB" -lt "${MIN_GB:-8}" ]; then
  if [ "$DARWIN" = 1 ]; then
    die "Docker Desktop's VM has ${MEM_GB} GB of memory, below MIN_RAM_GB=${MIN_GB:-8}: raise Docker Desktop > Settings > Resources > Memory Limit to $((${MIN_GB:-8} + 2)) GB or more"
  fi
  die "Docker has ${MEM_GB} GB of memory, below MIN_RAM_GB=${MIN_GB:-8}"
fi
pass "Docker memory ${MEM_GB} GB (min ${MIN_GB:-8})"

PLATFORM="$(docker_platform)"
BUILD_PLATFORMS="$PLATFORM"
if [ "$PLATFORM" = linux/arm64 ]; then
  # Every question without a proven native build runs the published linux/amd64 image.
  GOT="$(docker run --rm --platform linux/amd64 alpine:3.20 uname -m 2>/dev/null || true)"
  if [ "$GOT" != x86_64 ]; then
    if [ "$DARWIN" = 1 ]; then
      die "Docker cannot run linux/amd64 containers (uname -m gave '${GOT:-nothing}'): Docker Desktop > Settings > General > \"Use Rosetta for x86_64/amd64 emulation on Apple Silicon\", Apply & restart"
    fi
    die "Docker cannot run linux/amd64 containers (uname -m gave '${GOT:-nothing}'): docker run --privileged --rm tonistiigi/binfmt --install amd64"
  fi
  pass "Docker runs linux/amd64 containers (emulated)"
  # The native one last: without the containerd image store the tag keeps the last pull.
  BUILD_PLATFORMS="linux/amd64 linux/arm64"
else
  pass "Docker runs $PLATFORM natively; DeepSWE uses the published images"
fi

# JiuwenSwarm always builds in this image, iCode on a Mac or for another CPU type;
# a JiuwenSwarm offline pack needs make.
BUILD_IMAGE="$(toolchain_value ICODE_LINUX_BUILD_IMAGE)"
[ -n "$BUILD_IMAGE" ] || die "ICODE_LINUX_BUILD_IMAGE missing from pipeline/config/toolchain.env"
for plat in $BUILD_PLATFORMS; do
  docker pull --quiet --platform "$plat" "$BUILD_IMAGE" >/dev/null \
    || die "cannot pull $BUILD_IMAGE for $plat (network or registry login)"
  docker run --rm --platform "$plat" "$BUILD_IMAGE" sh -c 'make --version >/dev/null && uv --version && git --version >/dev/null' >/dev/null \
    || die "$BUILD_IMAGE for $plat has no make, uv or git"
  pass "harness build image runs for $plat ($BUILD_IMAGE)"
done

# Harness builds mount the workspace and the share at their own paths; Docker
# Desktop only mounts folders it shares (Settings > Resources > File sharing).
share_probe() {
  local dir="$1" mark
  mkdir -p "$dir"
  dir="$(cd "$dir" && pwd -P)"
  mark="$dir/.mac-k3d-share-probe.$$"
  echo ok >"$mark"
  if [ "$(docker run --rm -v "$dir:$dir:ro" alpine:3.20 cat "$mark" 2>/dev/null || true)" != ok ]; then
    rm -f "$mark"
    if [ "$DARWIN" = 1 ]; then
      die "Docker cannot mount $dir: add it (or a parent) in Docker Desktop > Settings > Resources > File sharing"
    fi
    die "Docker cannot mount $dir"
  fi
  rm -f "$mark"
  pass "Docker mounts $dir"
}
share_probe "$SHARE"
if [ -f "$WORKER_CONFIG" ]; then
  REMOTE_FS="$(yaml_field "$WORKER_CONFIG" jenkins_agent remote_fs)"
  [ -z "$REMOTE_FS" ] || share_probe "$REMOTE_FS"
fi

"$MAC_K3D_BIN" --help | grep -q eval || die "mac-k3d --help does not list eval"
pass "mac-k3d lists eval"

if [ "$PLATFORM" = linux/arm64 ]; then
  STORE="$SHARE/env-check/deepswe"
  PROVEN="$(python3 - "$STORE" <<'PY'
import json, sys
from pathlib import Path

n = 0
for path in Path(sys.argv[1]).glob("*.json"):
    try:
        doc = json.loads(path.read_text(encoding="utf-8"))
    except (OSError, ValueError):
        continue
    entry = (doc.get("results") or {}).get("built linux/arm64") or {}
    n += bool((entry.get("verdict") or {}).get("pass"))
print(n)
PY
)"
  if [ "$PROVEN" -gt 0 ]; then
    pass "$PROVEN DeepSWE questions have a proven native linux/arm64 build ($STORE)"
  else
    note "no DeepSWE question has a proven native build: each runs the published linux/amd64 image emulated. For native: $MAC_K3D_BIN eval prepare --benchmark deepswe --tasks <id,...>"
  fi
fi

CODE="$(jenkins_login_code)"
[ "$CODE" = "200" ] || die "Jenkins UI ${JENKINS_URL}/login HTTP $CODE — controller must be up before Jenkins eval"
pass "Jenkins UI reachable for eval"

# Prefer an online agent when worker config exists
if [ -f "$WORKER_CONFIG" ]; then
  if have systemctl && systemctl --user is-active mac-k3d-jenkins-agent.service >/dev/null 2>&1; then
    pass "worker agent unit active (Jenkins jobs can run on this host)"
  elif [ "$DARWIN" = 1 ] && launchctl print "gui/$(id -u)/com.mac-k3d.jenkins-agent" 2>/dev/null | grep -q 'state = running'; then
    pass "macOS agent LaunchAgent running"
  else
    note "worker.yaml present but agent not active — Jenkins eval needs an online node; --local may still work"
  fi
else
  note "no worker.yaml — Jenkins-queued eval needs a worker somewhere; --local uses this host Docker only"
fi

if [ "${RUN_EVAL_SMOKE:-0}" = "1" ]; then
  echo "Running mac-k3d eval --stage env (RUN_EVAL_SMOKE=1)…"
  "$MAC_K3D_BIN" eval --stage env
  pass "eval --stage env"
else
  note "skipping eval --stage env (set RUN_EVAL_SMOKE=1 to run)"
fi

pass "eval-ready — safe to start an evaluation task (ensure deepseek-api-key in Jenkins for LLM stages)"
pass "04_check_eval_ready complete"
