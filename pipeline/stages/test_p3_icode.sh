#!/usr/bin/env bash
# Fixture tests for P3 binary-mode official *-full-* names (no network).
set -euo pipefail

STAGES="$(cd "$(dirname "$0")" && pwd)"
TMP="$(mktemp -d)"
trap 'rm -rf "$TMP"' EXIT

fail() {
  echo "FAIL: $*" >&2
  exit 1
}

ok() {
  echo "OK $*"
}

make_stub() {
  cat >"$1" <<'EOF'
#!/bin/sh
echo "usage: icode [--help]"
exit 0
EOF
  chmod +x "$1"
}

run_p3() {
  env \
    MAC_K3D_EVAL_WORKDIR="$1" \
    MAC_K3D_SHARE="$2" \
    ICODE_MODE=binary \
    ICODE_RELEASE="${3-}" \
    ICODE_SOURCE="" \
    bash "$STAGES/p3_icode.sh"
}

assert_p3_ok() {
  local work="$1"
  run_p3 "$work" "$2" "${3-}" >/dev/null
  [ -s "$work/icode_bin_path.txt" ] || fail "missing $work/icode_bin_path.txt"
  local bin
  bin="$(cat "$work/icode_bin_path.txt")"
  [ -f "$bin" ] || fail "ICODE_BIN not a file: $bin"
  "$bin" --help >/dev/null || fail "icode --help failed: $bin"
}

pack_full_tarball() {
  local dest="$1"
  local pack="$TMP/pack-$$"
  mkdir -p "$pack"
  make_stub "$pack/icode"
  tar -C "$pack" -czf "$dest" icode
  rm -rf "$pack"
}

SHARE="$TMP/share"
mkdir -p "$SHARE"

# 1. Official .tar.gz via ICODE_RELEASE
TARBALL="$TMP/icode-linux-x86_64-full-v0.1.41.tar.gz"
pack_full_tarball "$TARBALL"
W1="$TMP/w1"
assert_p3_ok "$W1" "$SHARE" "$TARBALL"
ok "ICODE_RELEASE tarball icode-linux-x86_64-full-v0.1.41.tar.gz"

# 2. Same archive, extensionless official name
BARE="$TMP/icode-linux-x86_64-full-v0.1.41"
cp "$TARBALL" "$BARE"
W2="$TMP/w2"
assert_p3_ok "$W2" "$SHARE" "$BARE"
ok "ICODE_RELEASE extensionless icode-linux-x86_64-full-v0.1.41"

# 3. Unpacked official directory (keep sibling files)
DIR="$TMP/icode-linux-x86_64-full-v0.1.41-dir"
mkdir -p "$DIR"
make_stub "$DIR/icode"
echo sibling >"$DIR/libicode.so"
W3="$TMP/w3"
assert_p3_ok "$W3" "$SHARE" "$DIR"
BIN3="$(cat "$W3/icode_bin_path.txt")"
[ -f "$(dirname "$BIN3")/libicode.so" ] || fail "sibling lib not copied next to icode"
ok "ICODE_RELEASE unpacked *-full-* directory"

# 4. Discover official tarball in MAC_K3D_SHARE (ICODE_RELEASE empty)
cp "$TARBALL" "$SHARE/icode-linux-x86_64-full-v0.1.41.tar.gz"
W4="$TMP/w4"
assert_p3_ok "$W4" "$SHARE" ""
ok "discover MAC_K3D_SHARE/icode-linux-x86_64-full-v0.1.41.tar.gz"

# 5. Random file is not a release
RAND="$TMP/notes.txt"
echo not-icode >"$RAND"
W5="$TMP/w5"
if run_p3 "$W5" "$TMP/empty-share" "$RAND" >/dev/null 2>&1; then
  fail "expected P3 to reject $RAND"
fi
ok "reject random file that is not icode or *-full-*"

# 6. Leftover named icode + official tarball → discover prefers *-full-*
SHARE6="$TMP/share6"
mkdir -p "$SHARE6"
cat >"$SHARE6/icode" <<'EOF'
#!/bin/sh
echo WRAPPER
exit 0
EOF
chmod +x "$SHARE6/icode"
cp "$TARBALL" "$SHARE6/icode-linux-x86_64-full-v0.1.41.tar.gz"
W6="$TMP/w6"
assert_p3_ok "$W6" "$SHARE6" ""
BIN6="$(cat "$W6/icode_bin_path.txt")"
help6="$("$BIN6" --help)"
echo "$help6" | grep -q "usage: icode" || fail "expected tarball stub, got: $help6"
echo "$help6" | grep -q WRAPPER && fail "leftover wrapper won over *-full-*"
ok "discover prefers *-full-* over leftover icode"

echo "OK test_p3_icode.sh"
