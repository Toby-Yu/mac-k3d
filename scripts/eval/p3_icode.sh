#!/usr/bin/env bash
# P3 — resolve iCode binary or source
set -euo pipefail
source "$(cd "$(dirname "$0")" && pwd)/_common.sh"

progress 35 "P3: resolving iCode ($ICODE_MODE)"

ICODE_BIN=""
case "$ICODE_MODE" in
  binary)
    [ -n "$ICODE_RELEASE" ] || die "ICODE_RELEASE required for ICODE_MODE=binary"
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
    [ -e "$src" ] || die "ICODE_RELEASE not found: $src"
    case "$(basename "$src")" in
      *.tar.gz|*.tgz)
        tar -xzf "$src" -C "$unpack"
        ;;
      *)
        cp "$src" "$unpack/icode"
        chmod +x "$unpack/icode" || true
        ;;
    esac
    ICODE_BIN="$(find "$unpack" -type f -name icode | head -n 1)"
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
