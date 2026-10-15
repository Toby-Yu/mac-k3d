#!/usr/bin/env bash
# Download the mac-k3d Release asset for this OS/arch and install to ~/.local/bin.
set -euo pipefail
# shellcheck source=./_common.sh
source "$(cd "$(dirname "$0")" && pwd)/_common.sh"

# DeepSWE with iCode or JiuwenSwarm needs `eval --harness` and, on Apple
# silicon, `eval prepare`. An older Release has neither.
check_features() {
  local version
  version="$("$MAC_K3D_BIN" --version 2>/dev/null || echo "mac-k3d ?")"
  "$MAC_K3D_BIN" --help | grep -q setup || die "mac-k3d --help missing setup"
  "$MAC_K3D_BIN" --help | grep -q eval || die "mac-k3d --help missing eval"
  pass "help lists setup and eval"
  if ! "$MAC_K3D_BIN" eval --help 2>/dev/null | grep -q -- '--harness' \
    || ! "$MAC_K3D_BIN" eval prepare --help >/dev/null 2>&1; then
    die "${version} at ${MAC_K3D_BIN} has no \`eval --harness\` or \`eval prepare\` (DeepSWE with iCode or JiuwenSwarm). Install a newer Release (MAC_K3D_RELEASE_TAG=v$(checkout_version), this checkout's version), or build this checkout (cargo build --release) and rerun with SKIP_DOWNLOAD=1 MAC_K3D_BIN=$MAC_K3D_ROOT/target/release/mac-k3d"
  fi
  pass "eval lists --harness and prepare"
}

if [ "${SKIP_DOWNLOAD:-0}" = "1" ]; then
  require_mac_k3d
  pass "SKIP_DOWNLOAD=1 — using existing binary"
  "$MAC_K3D_BIN" --version
  check_features
  pass "01_download_binary complete"
  exit 0
fi

ASSET="$(detect_asset_name)"
TMPDIR_DL="${TMPDIR:-/tmp}/mac-k3d-env-setup-$$"
mkdir -p "$TMPDIR_DL" "$INSTALL_DIR"
cleanup() { rm -rf "$TMPDIR_DL"; }
trap cleanup EXIT

echo "Downloading ${MAC_K3D_REPO}@${MAC_K3D_RELEASE_TAG} asset ${ASSET} …"

NOT_FOUND="no Release ${MAC_K3D_RELEASE_TAG} asset ${ASSET} on ${MAC_K3D_REPO}. Set MAC_K3D_RELEASE_TAG to a published tag, or build this checkout (cargo build --release) and rerun with SKIP_DOWNLOAD=1 MAC_K3D_BIN=$MAC_K3D_ROOT/target/release/mac-k3d"
if have gh; then
  gh release download "$MAC_K3D_RELEASE_TAG" \
    --repo "$MAC_K3D_REPO" \
    --pattern "$ASSET" \
    --dir "$TMPDIR_DL" \
    --clobber || die "$NOT_FOUND"
else
  URL="https://github.com/${MAC_K3D_REPO}/releases/download/${MAC_K3D_RELEASE_TAG}/${ASSET}"
  curl -fsSL -o "$TMPDIR_DL/$ASSET" "$URL" || die "$NOT_FOUND"
fi

test -f "$TMPDIR_DL/$ASSET" || die "download missing $ASSET"
chmod +x "$TMPDIR_DL/$ASSET"

# macOS: clear quarantine when present
if [ "$(uname -s)" = "Darwin" ] && have xattr; then
  xattr -d com.apple.quarantine "$TMPDIR_DL/$ASSET" 2>/dev/null || true
fi

cp "$TMPDIR_DL/$ASSET" "$INSTALL_DIR/mac-k3d"
chmod +x "$INSTALL_DIR/mac-k3d"
export MAC_K3D_BIN="$INSTALL_DIR/mac-k3d"

# Prefer install dir on PATH for this process
export PATH="$INSTALL_DIR:$PATH"

pass "downloaded $ASSET → $MAC_K3D_BIN"
"$MAC_K3D_BIN" --version
check_features
pass "01_download_binary complete"
