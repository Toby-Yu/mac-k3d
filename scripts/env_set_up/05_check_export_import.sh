#!/usr/bin/env bash
# File-only export/import check. Never --force onto ~/.config/mac-k3d/*.
set -euo pipefail
# shellcheck source=./_common.sh
source "$(cd "$(dirname "$0")" && pwd)/_common.sh"

require_mac_k3d

"$MAC_K3D_BIN" export --help >/dev/null 2>&1 \
  || die "this mac-k3d has no export (need feat/config-export-import or a later Release; not GitHub v0.5.2)"
pass "CLI lists export"

src=""
if [ -f "$WORKER_CONFIG" ]; then
  src="$WORKER_CONFIG"
elif [ -f "$CONTROLLER_CONFIG" ]; then
  src="$CONTROLLER_CONFIG"
else
  die "no live YAML at $WORKER_CONFIG or $CONTROLLER_CONFIG"
fi
pass "source $src"

out="$(mktemp /tmp/mac-k3d-export-XXXX.yaml)"
dest="$(mktemp /tmp/mac-k3d-import-XXXX.yaml)"
rm -f "$dest"

"$MAC_K3D_BIN" export -c "$src" -o "$out"
grep -E '^[[:space:]]*api_token:' "$out" && die "exported YAML still has api_token key: $out"
grep -qi 'sk-[A-Za-z0-9]\{16,\}' "$out" && die "exported YAML looks like it contains an API key"
pass "export wrote $out (no api_token key)"

"$MAC_K3D_BIN" import "$out" -c "$dest"
[ -f "$dest" ] || die "import did not write $dest"
grep -q '^role:' "$dest" || die "imported YAML missing role: $dest"
grep -E '^[[:space:]]*api_token:' "$dest" && die "imported YAML has api_token key: $dest"
pass "import wrote $dest (role kept, no api_token key)"

pass "05_check_export_import complete"
echo "NOTE scratch files: $out $dest (not live ~/.config/mac-k3d)"
