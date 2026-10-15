#!/usr/bin/env bash
# Shared helpers for environment bootstrap checks.
set -euo pipefail

ENV_SETUP_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
export MAC_K3D_ROOT="$(cd "$ENV_SETUP_DIR/../.." && pwd)"

# The Release of this checkout's version, so the binary matches these scripts
# and the pipeline beside them.
checkout_version() {
  sed -n 's/^version = "\(.*\)"$/\1/p' "$MAC_K3D_ROOT/Cargo.toml" 2>/dev/null | head -n 1
}
_version="$(checkout_version)"
export MAC_K3D_RELEASE_TAG="${MAC_K3D_RELEASE_TAG:-v${_version:-0.5.4}}"
unset _version
export MAC_K3D_REPO="${MAC_K3D_REPO:-Toby-Yu/mac-k3d}"
export CONTROLLER_CONFIG="${CONTROLLER_CONFIG:-$HOME/.config/mac-k3d/config.yaml}"
export WORKER_CONFIG="${WORKER_CONFIG:-$HOME/.config/mac-k3d/worker.yaml}"
export INSTALL_DIR="${INSTALL_DIR:-$HOME/.local/bin}"
export MAC_K3D_BIN="${MAC_K3D_BIN:-}"

pass() { echo "OK $*"; }
fail() { echo "FAIL $*" >&2; return 1; }
die() { echo "ERROR: $*" >&2; exit 1; }
have() { command -v "$1" >/dev/null 2>&1; }
note() { echo "NOTE $*"; }

resolve_mac_k3d() {
  if [ -n "${MAC_K3D_BIN}" ] && [ -x "${MAC_K3D_BIN}" ]; then
    return 0
  fi
  if have mac-k3d; then
    MAC_K3D_BIN="$(command -v mac-k3d)"
    export MAC_K3D_BIN
    return 0
  fi
  if [ -x "$INSTALL_DIR/mac-k3d" ]; then
    MAC_K3D_BIN="$INSTALL_DIR/mac-k3d"
    export MAC_K3D_BIN
    return 0
  fi
  if [ -x "$MAC_K3D_ROOT/target/release/mac-k3d" ]; then
    MAC_K3D_BIN="$MAC_K3D_ROOT/target/release/mac-k3d"
    export MAC_K3D_BIN
    return 0
  fi
  return 1
}

require_mac_k3d() {
  resolve_mac_k3d || die "mac-k3d not found. Run 01_download_binary.sh or set MAC_K3D_BIN."
  pass "mac-k3d=$MAC_K3D_BIN"
}

detect_asset_name() {
  local os arch uname_s uname_m
  uname_s="$(uname -s)"
  uname_m="$(uname -m)"
  case "$uname_s" in
    Linux) os=linux ;;
    Darwin) os=darwin ;;
    *) die "unsupported OS: $uname_s (need Linux or Darwin)" ;;
  esac
  case "$uname_m" in
    x86_64|amd64) arch=x86_64 ;;
    aarch64|arm64) arch=aarch64 ;;
    *) die "unsupported arch: $uname_m" ;;
  esac
  echo "mac-k3d-${os}-${arch}"
}

jenkins_login_code() {
  local code
  code="$(curl -sS -o /dev/null -w '%{http_code}' --max-time 10 "${JENKINS_URL%/}/login")" || true
  echo "${code:-000}"
}

# JENKINS_USER and JENKINS_API_TOKEN (a Jenkins API token of that user) let the
# checks read job pages, plugins, credentials and nodes. Without them those
# checks are skipped with a NOTE.
have_jenkins_token() {
  [ -n "${JENKINS_USER:-}" ] && [ -n "${JENKINS_API_TOKEN:-}" ]
}

# GET a Jenkins path as JENKINS_USER. The token reaches curl on stdin, never
# on its command line.
jenkins_api() {
  printf 'user = "%s:%s"\n' "$JENKINS_USER" "$JENKINS_API_TOKEN" \
    | curl -sS -f --max-time 15 -K - "${JENKINS_URL%/}$1"
}

# One `key:` under a top-level `section:` of a YAML file (two-space indent), unquoted.
yaml_field() {
  awk -v section="$2" -v key="$3" '
    /^[^ #]/ { inside = ($0 ~ ("^" section ":")); next }
    inside && $0 ~ ("^  " key ":") {
      sub("^  " key ":[ ]*", ""); sub(/[ ]+#.*$/, ""); gsub(/["\047]/, ""); print; exit
    }
  ' "$1"
}

# A worker-only machine (a Mac beside a Linux controller) checks the controller
# its agent connects to.
if [ -z "${JENKINS_URL:-}" ] && [ ! -f "$CONTROLLER_CONFIG" ] && [ -f "$WORKER_CONFIG" ]; then
  JENKINS_URL="$(yaml_field "$WORKER_CONFIG" jenkins_agent controller_url)"
fi
export JENKINS_URL="${JENKINS_URL:-http://localhost:17070}"

# A KEY=value pin from the pipeline's toolchain.env.
toolchain_value() {
  local env="$MAC_K3D_ROOT/pipeline/config/toolchain.env"
  [ -f "$env" ] || env="$HOME/.local/share/mac-k3d/pipeline/config/toolchain.env"
  sed -n "s/^$1=//p" "$env" 2>/dev/null | head -n 1
}

# linux/<arch> of the containers Docker runs natively (Docker Desktop's VM on a Mac).
docker_platform() {
  case "$(docker info --format '{{.Architecture}}' 2>/dev/null || uname -m)" in
    aarch64 | arm64) echo linux/arm64 ;;
    *) echo linux/amd64 ;;
  esac
}

# 0 when version $1 (X.Y[.Z]) is at least $2 (X.Y).
version_at_least() {
  local have_major have_minor want_major want_minor
  have_major="${1%%.*}"
  have_minor="${1#*.}"
  have_minor="${have_minor%%.*}"
  want_major="${2%%.*}"
  want_minor="${2#*.}"
  [ "$have_major" -gt "$want_major" ] 2>/dev/null && return 0
  [ "$have_major" -eq "$want_major" ] 2>/dev/null && [ "$have_minor" -ge "$want_minor" ] 2>/dev/null
}

# 0 = job $1 exists, 1 = it does not, 2 = cannot tell: a controller that hides
# itself from anonymous users answers 403 for any job, real or not.
job_state() {
  local code url="${JENKINS_URL%/}/job/$1/api/json?tree=name"
  if have_jenkins_token; then
    code="$(printf 'user = "%s:%s"\n' "$JENKINS_USER" "$JENKINS_API_TOKEN" \
      | curl -sS -o /dev/null -w '%{http_code}' --max-time 10 -K - "$url")" || true
  else
    code="$(curl -sS -o /dev/null -w '%{http_code}' --max-time 10 "$url")" || true
  fi
  case "${code:-000}" in
    200) return 0 ;;
    401 | 403) return 2 ;;
    *) return 1 ;;
  esac
}
