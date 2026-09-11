#!/usr/bin/env bash
# Install iCode into the Pier sandbox image / environment.
set -euo pipefail

echo "icode agent install: preparing iCode in sandbox"

mkdir -p /opt/icode /usr/local/bin

if [ -n "${ICODE_RELEASE_URL:-}" ]; then
  curl -fsSL "$ICODE_RELEASE_URL" -o /tmp/icode.tgz
  tar -xzf /tmp/icode.tgz -C /opt/icode
  BIN="$(find /opt/icode -type f -name icode | head -n 1)"
  ln -sf "$BIN" /usr/local/bin/icode
elif [ -n "${ICODE_BIN_HOST:-}" ] && [ -f "${ICODE_BIN_HOST}" ]; then
  cp "$ICODE_BIN_HOST" /usr/local/bin/icode
  chmod +x /usr/local/bin/icode
elif [ -d "${ICODE_SOURCE_HOST:-}" ]; then
  # Source tree mounted or copied by Pier/host wrapper
  if [ -x "${ICODE_SOURCE_HOST}/.venv/bin/icode" ]; then
    ln -sf "${ICODE_SOURCE_HOST}/.venv/bin/icode" /usr/local/bin/icode
  else
    echo "ICODE_SOURCE_HOST present but no .venv/bin/icode; ensure uv sync on host first" >&2
    exit 1
  fi
elif command -v icode >/dev/null 2>&1; then
  echo "icode already on PATH"
else
  echo "No ICODE_RELEASE_URL / ICODE_BIN_HOST / ICODE_SOURCE_HOST; expecting host to inject icode" >&2
fi

command -v icode >/dev/null 2>&1 || [ -x /usr/local/bin/icode ] || {
  echo "icode not installed after install.sh" >&2
  exit 1
}

echo "icode agent install complete"
