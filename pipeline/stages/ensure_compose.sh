#!/usr/bin/env bash
# Ensure `docker compose` (v2 plugin) and `docker buildx`. Safe to source after _common.sh or run alone.
# shellcheck disable=SC2034

_compose_die() {
  if command -v die >/dev/null 2>&1; then
    die "$@"
  else
    echo "ERROR: $*" >&2
    exit 1
  fi
}

_cli_plugin_arch() {
  case "$(uname -s)-$(uname -m)" in
    Darwin-arm64 | Darwin-aarch64) echo "darwin-arm64" ;;
    Darwin-x86_64) echo "darwin-amd64" ;;
    Linux-aarch64 | Linux-arm64) echo "linux-arm64" ;;
    *) echo "linux-amd64" ;;
  esac
}

ensure_docker_compose() {
  if docker compose version >/dev/null 2>&1; then
    echo "OK docker compose $(docker compose version 2>/dev/null | head -n 1)"
    return 0
  fi

  echo "Installing Docker Compose v2 plugin under ~/.docker/cli-plugins (no sudo)…"
  mkdir -p "${HOME}/.docker/cli-plugins"
  local ver="v2.40.3" url asset
  case "$(uname -s)-$(uname -m)" in
    Darwin-arm64 | Darwin-aarch64) asset="docker-compose-darwin-aarch64" ;;
    Darwin-x86_64) asset="docker-compose-darwin-x86_64" ;;
    Linux-aarch64 | Linux-arm64) asset="docker-compose-linux-aarch64" ;;
    *) asset="docker-compose-linux-x86_64" ;;
  esac
  url="https://github.com/docker/compose/releases/download/${ver}/${asset}"
  if ! curl -fsSL "$url" -o "${HOME}/.docker/cli-plugins/docker-compose"; then
    _compose_die "failed to download Docker Compose from $url"
  fi
  chmod +x "${HOME}/.docker/cli-plugins/docker-compose"
  docker compose version >/dev/null 2>&1 \
    || _compose_die "docker compose still missing after plugin install. Install Docker Desktop (macOS) or docker-compose-v2 (Linux apt)."
  echo "OK docker compose $(docker compose version 2>/dev/null | head -n 1)"
}

ensure_docker_buildx() {
  if docker buildx version >/dev/null 2>&1; then
    echo "OK docker buildx $(docker buildx version 2>/dev/null | head -n 1)"
    return 0
  fi

  echo "Installing Docker buildx plugin under ~/.docker/cli-plugins (Harbor allowlist sidecar needs it)…"
  mkdir -p "${HOME}/.docker/cli-plugins"
  local ver="v0.29.1" arch url
  arch="$(_cli_plugin_arch)"
  url="https://github.com/docker/buildx/releases/download/${ver}/buildx-${ver}.${arch}"
  if ! curl -fsSL "$url" -o "${HOME}/.docker/cli-plugins/docker-buildx"; then
    _compose_die "failed to download Docker buildx from $url"
  fi
  chmod +x "${HOME}/.docker/cli-plugins/docker-buildx"
  docker buildx version >/dev/null 2>&1 \
    || _compose_die "docker buildx still missing after plugin install. Harbor LoLBench tasks need buildx to build the egress sidecar."
  echo "OK docker buildx $(docker buildx version 2>/dev/null | head -n 1)"
}

if [[ "${BASH_SOURCE[0]}" == "${0}" ]]; then
  set -euo pipefail
  command -v docker >/dev/null || _compose_die "docker not on PATH"
  ensure_docker_compose
  ensure_docker_buildx
fi
