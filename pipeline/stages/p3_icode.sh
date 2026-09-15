#!/usr/bin/env bash
# P3 — resolve iCode binary or source
set -euo pipefail
source "$(cd "$(dirname "$0")" && pwd)/_common.sh"

progress 35 "P3: resolving iCode ($ICODE_MODE)"

ICODE_BIN=""
case "$ICODE_MODE" in
  binary)
    if [ -z "$ICODE_RELEASE" ]; then
      die "ICODE_MODE=binary: place icode or icode-<os>-<arch>-full-vX.Y.Z (.tar.gz, same name without suffix, or unpacked folder) at ${MAC_K3D_SHARE}/ (or /opt/mac-k3d/). Example: cp /path/to/icode-linux-x86_64-full-v0.1.41.tar.gz ${MAC_K3D_SHARE}/"
    fi
    unpack="$WORKDIR/icode-bin"
    rm -rf "$unpack"
    mkdir -p "$unpack"
    src="$ICODE_RELEASE"
    case "$src" in
      http://*|https://*)
        curl -fsSL "$src" -o "$WORKDIR/icode-release.bin"
        src="$WORKDIR/icode-release.bin"
        ;;
    esac
    install_icode_release "$src" "$unpack"
    ICODE_BIN="$(find "$unpack" -type f -name icode -print -quit)"
    [ -n "$ICODE_BIN" ] || die "could not find icode inside release"
    ;;
  source)
    [ -d "$ICODE_SOURCE" ] || die "ICODE_SOURCE missing: $ICODE_SOURCE. Clone or copy iCode there, or set ICODE_SOURCE."
    if [ -x "$ICODE_SOURCE/.venv/bin/icode" ]; then
      ICODE_BIN="$ICODE_SOURCE/.venv/bin/icode"
    elif have uv; then
      ( cd "$ICODE_SOURCE" && uv sync )
      if [ -x "$ICODE_SOURCE/.venv/bin/icode" ]; then
        ICODE_BIN="$ICODE_SOURCE/.venv/bin/icode"
      else
        ICODE_BIN="uv run --directory $ICODE_SOURCE icode"
      fi
    else
      die "need uv or $ICODE_SOURCE/.venv/bin/icode"
    fi
    ;;
  *)
    die "unknown ICODE_MODE=$ICODE_MODE"
    ;;
esac

export ICODE_BIN
echo "$ICODE_BIN" >"$WORKDIR/icode_bin_path.txt"
# shellcheck disable=SC2086
if [[ "$ICODE_BIN" == uv\ run* ]]; then
  eval "$ICODE_BIN --help" >/dev/null
else
  "$ICODE_BIN" --help >/dev/null
fi
echo "OK icode helps ($ICODE_BIN)"
progress 40 "P3 complete"
