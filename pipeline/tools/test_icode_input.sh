#!/usr/bin/env bash
# No-network URL/ref validation for get_bin_icode (pipeline/lib/icode_input.sh).
set -euo pipefail

LIB="$(cd "$(dirname "$0")/../lib" && pwd)"
IN="$LIB/icode_input.sh"
[ -f "$IN" ] || {
  echo "FAIL: missing $IN" >&2
  exit 1
}

fail() {
  echo "FAIL: $*" >&2
  exit 1
}

ok() {
  echo "OK $*"
}

must_fail() {
  local msg="$1"
  shift
  if bash "$IN" "$@" >/dev/null 2>&1; then
    fail "expected failure: $msg"
  fi
  ok "reject $msg"
}

must_ok() {
  local msg="$1"
  shift
  bash "$IN" "$@" >/dev/null || fail "expected OK: $msg"
  ok "accept $msg"
}

must_fail "file://" --validate-url "file:///tmp/icode"
must_fail "ssh://" --validate-url "ssh://github.com/org/icode.git"
must_fail "http://" --validate-url "http://github.com/org/icode.git"
must_fail "userinfo" --validate-url "https://user:token@github.com/org/icode.git"
must_fail "evil host" --validate-url "https://github.com.evil.com/org/icode.git"
must_ok "github https" --validate-url "https://github.com/org/icode.git"
must_ok "gitcode https" --validate-url "https://gitcode.com/org/icode.git"

must_fail "leading dash" --validate-ref "--upload-pack"
must_fail "semicolon" --validate-ref "foo;rm"
must_fail "dotdot" --validate-ref "foo/../bar"
must_ok "tag" --validate-ref "v0.1.41"
must_ok "branch" --validate-ref "main"
must_ok "commit" --validate-ref "0123456789abcdef0123456789abcdef01234567"

bash "$IN" --normalize-ref-kind auto main | grep -qx branch || fail "auto main → branch"
bash "$IN" --normalize-ref-kind auto 0123456789abcdef0123456789abcdef01234567 | grep -qx commit || fail "auto sha → commit"
bash "$IN" --normalize-ref-kind TAG v0.1.41 | grep -qx tag || fail "TAG → tag"
must_ok "commit kind + sha" --validate-ref-kind commit deadbee
must_fail "commit kind + branch" --validate-ref-kind commit main
must_ok "pr kind + number" --validate-ref-kind pr 7
must_fail "pr kind + branch" --validate-ref-kind pr main
must_fail "bad kind" --normalize-ref-kind sha main
bash "$IN" --normalize-ref-kind pr 7 | grep -qx pr || fail "pr kind"
ok "ref kind auto/tag/commit"

# Local git checkout by kind (no network). protocol.file is blocked unless this flag.
KIND_TMP="$(mktemp -d)"
trap 'rm -rf "$KIND_TMP"' EXIT
ORIGIN="$KIND_TMP/origin"
git init -q "$ORIGIN"
git -C "$ORIGIN" symbolic-ref HEAD refs/heads/main
git -C "$ORIGIN" config user.email t@t
git -C "$ORIGIN" config user.name t
echo a >"$ORIGIN/f"
git -C "$ORIGIN" add f
git -C "$ORIGIN" commit -q -m base
BASE_SHA="$(git -C "$ORIGIN" rev-parse HEAD)"
git -C "$ORIGIN" tag v9.9.9
git -C "$ORIGIN" checkout -q -b feature
echo b >>"$ORIGIN/f"
git -C "$ORIGIN" commit -q -am pr
FEAT_SHA="$(git -C "$ORIGIN" rev-parse HEAD)"
git -C "$ORIGIN" checkout -q main
export MAC_K3D_ICODE_GIT_ALLOW_FILE=1
export WORKDIR="$KIND_TMP/work"
mkdir -p "$WORKDIR"
# shellcheck disable=SC1090
source "$IN"
icode_git_checkout "$KIND_TMP/c-branch" "$ORIGIN" feature branch
[ "$(git -C "$KIND_TMP/c-branch" rev-parse HEAD)" = "$FEAT_SHA" ] || fail "branch checkout sha"
icode_git_checkout "$KIND_TMP/c-tag" "$ORIGIN" v9.9.9 tag
[ "$(git -C "$KIND_TMP/c-tag" rev-parse HEAD)" = "$BASE_SHA" ] || fail "tag checkout sha"
icode_git_checkout "$KIND_TMP/c-commit" "$ORIGIN" "$FEAT_SHA" commit
[ "$(git -C "$KIND_TMP/c-commit" rev-parse HEAD)" = "$FEAT_SHA" ] || fail "commit checkout sha"
git -C "$ORIGIN" update-ref refs/merge-requests/7/head "$FEAT_SHA"
icode_git_checkout "$KIND_TMP/c-pr" "$ORIGIN" 7 pr 2>"$KIND_TMP/pr.err"
[ "$(git -C "$KIND_TMP/c-pr" rev-parse HEAD)" = "$FEAT_SHA" ] || fail "pr checkout sha"
grep -q 'DEBUG icode pr' "$KIND_TMP/pr.err" && fail "pr debug line still present"
icode_git_pr_refs "https://github.com/org/icode.git" 12 | grep -qx 'refs/pull/12/head' || fail "github pr ref"
icode_git_pr_refs "https://gitcode.com/org/icode.git" 12 | head -n 1 | grep -qx 'refs/merge-requests/12/head' || fail "gitcode pr ref"
SHORT="$(git -C "$ORIGIN" rev-parse --short=8 "$FEAT_SHA")"
icode_git_checkout "$KIND_TMP/c-abbr" "$ORIGIN" "$SHORT" commit
[ "$(git -C "$KIND_TMP/c-abbr" rev-parse HEAD)" = "$FEAT_SHA" ] || fail "abbrev commit checkout sha"
icode_record_git_meta "$KIND_TMP/c-commit" "https://github.com/org/icode.git" commit "$FEAT_SHA"
python3 -c 'import json,sys; d=json.load(open(sys.argv[1])); assert d["kind"]=="commit" and d["sha"]==sys.argv[2]' "$WORKDIR/icode_git.json" "$FEAT_SHA" || fail "icode_git.json"
ok "checkout branch/tag/commit + icode_git.json"
grep -q 'icode_force_rm' "$IN" || fail "missing icode_force_rm"
if command -v docker >/dev/null 2>&1 && docker info >/dev/null 2>&1; then
  plat="$(icode_docker_platform)"
  LEFT="$KIND_TMP/docker-left"
  mkdir -p "$LEFT"
  docker run --rm --platform "$plat" -v "$LEFT:/w" alpine:3.20 sh -c 'mkdir -p /w/icode-src/.venv && echo x>/w/icode-src/.venv/r && mkdir -p /w/icode-src/__pycache__ && echo p>/w/icode-src/__pycache__/x.pyc && chown -R 0:0 /w/icode-src/.venv && chown -R 65534:65534 /w/icode-src/__pycache__'
  icode_git_checkout "$LEFT/icode-src" "$ORIGIN" main branch
  [ "$(git -C "$LEFT/icode-src" rev-parse HEAD)" = "$BASE_SHA" ] || fail "docker leftover wipe then checkout sha"
  ok "docker leftover wipe then checkout"
  RELF="$KIND_TMP/ICODE_RELEASE_FILE"
  docker run --rm --platform "$plat" -v "$KIND_TMP:/w" alpine:3.20 sh -c 'echo leftover >/w/ICODE_RELEASE_FILE && chown 0:0 /w/ICODE_RELEASE_FILE'
  bash "$IN" --force-rm "$RELF" >/dev/null || fail "force-rm leftover ICODE_RELEASE_FILE"
  [ ! -e "$RELF" ] || fail "ICODE_RELEASE_FILE still present after force-rm"
  ok "force-rm leftover ICODE_RELEASE_FILE"
else
  ok "skip docker leftover wipe (docker unavailable)"
fi
if ( ICODE_EXPECT_SHA="$BASE_SHA" icode_git_checkout "$KIND_TMP/c-expect-bad" "$ORIGIN" "$FEAT_SHA" commit ); then
  fail "expected ICODE_EXPECT_SHA mismatch to fail"
fi
ok "reject ICODE_EXPECT_SHA mismatch"
ICODE_EXPECT_SHA="$FEAT_SHA" icode_git_checkout "$KIND_TMP/c-expect-ok" "$ORIGIN" "$FEAT_SHA" commit
[ "$(git -C "$KIND_TMP/c-expect-ok" rev-parse HEAD)" = "$FEAT_SHA" ] || fail "expect sha checkout"
unset ICODE_EXPECT_SHA
ok "ICODE_EXPECT_SHA match"
icode_git_checkout "$KIND_TMP/c-clean" "$ORIGIN" main branch
icode_assert_clean_checkout "$KIND_TMP/c-clean"
echo extra >"$KIND_TMP/c-clean/extra.txt"
if ( icode_assert_clean_checkout "$KIND_TMP/c-clean" ); then
  fail "dirty tree should fail"
fi
rm -f "$KIND_TMP/c-clean/extra.txt"
printf '%s\n' '#!/bin/sh' >"$KIND_TMP/c-clean/icode"
icode_assert_clean_checkout "$KIND_TMP/c-clean"
ok "porcelain allows only the icode launcher"
unset MAC_K3D_ICODE_GIT_ALLOW_FILE

PTH_TREE="$KIND_TMP/pth-tree"
mkdir -p "$PTH_TREE/.venv/lib/python3.13/site-packages" "$PTH_TREE/.venv/sandbox-cpython/lib/python3.13"
icode_write_wrapper "$PTH_TREE/icode"
grep -q 'export PYTHONPATH' "$PTH_TREE/icode" && fail "launcher must not export PYTHONPATH"
grep -q 'export VIRTUAL_ENV' "$PTH_TREE/icode" && fail "launcher must not export VIRTUAL_ENV"
grep -qF 'exec "$PY" "$ROOT/.venv/bin/icode" "$@"' "$PTH_TREE/icode" || fail "launcher exec line"
icode_write_sandbox_pth "$PTH_TREE"
PTH="$PTH_TREE/.venv/sandbox-cpython/lib/python3.13/site-packages/icode-host.pth"
[ "$(cat "$PTH")" = "$(printf '%s\n%s' /opt/icode-host /opt/icode-host/.venv/lib/python3.13/site-packages)" ] \
  || fail "icode-host.pth must list exactly the clone root and its venv site-packages"
ok "launcher exports nothing; icode-host.pth lists the two iCode paths"

# icode_uv_sync: a fake uv records the git env it was given and fails on the
# call numbers listed in FAKE_UV_FAIL.
FAKE_UV_DIR="$KIND_TMP/fake-uv"
mkdir -p "$FAKE_UV_DIR/bin"
cat >"$FAKE_UV_DIR/bin/uv" <<'UV'
#!/usr/bin/env bash
n=$(( $(cat "$FAKE_UV_DIR/calls" 2>/dev/null || echo 0) + 1 ))
echo "$n" >"$FAKE_UV_DIR/calls"
{
  env | grep -E '^(GIT_CONFIG_(COUNT|KEY_|VALUE_)|GIT_ASKPASS=|GIT_SSH_COMMAND=)' | sort
  [ -n "${MAC_K3D_GIT_TOKEN:-}" ] && echo token_set=1
} >"$FAKE_UV_DIR/env.$n"
echo "fake uv stdout"
case " ${FAKE_UV_FAIL:-} " in *" $n "*) exit 1 ;; esac
exit 0
UV
chmod 755 "$FAKE_UV_DIR/bin/uv"
export FAKE_UV_DIR
UV_TREE="$KIND_TMP/uv-tree"
mkdir -p "$UV_TREE"
SENTINEL="gc-sentinel-not-for-logs"
run_uv_sync() {
  rm -f "$FAKE_UV_DIR"/calls "$FAKE_UV_DIR"/env.*
  # get_bin_icode unsets these after the clone; an editor's git askpass must not leak in.
  ( unset GIT_ASKPASS MAC_K3D_GIT_TOKEN; export PATH="$FAKE_UV_DIR/bin:$PATH" FAKE_UV_FAIL="$1"
    icode_uv_sync "$UV_TREE" gitcode.com "$2" ) \
    >"$FAKE_UV_DIR/out" 2>"$FAKE_UV_DIR/err"
}
uv_env() { printf '%s\n' "$FAKE_UV_DIR/env.$1"; }
token_leaked() { grep -qF "$SENTINEL" "$FAKE_UV_DIR/out" "$FAKE_UV_DIR/err"; }

run_uv_sync "" "$SENTINEL" || fail "uv sync with a PAT should pass"
[ ! -s "$FAKE_UV_DIR/out" ] || fail "icode_uv_sync stdout must stay empty: the caller captures ICODE_BIN"
[ "$(cat "$FAKE_UV_DIR/calls")" = 1 ] || fail "PAT success: expected one uv call"
grep -qx 'GIT_CONFIG_KEY_0=url.https://gitcode.com/.insteadOf' "$(uv_env 1)" || fail "PAT: insteadOf key"
grep -qx 'GIT_CONFIG_VALUE_0=ssh://git@gitcode.com/' "$(uv_env 1)" || fail "PAT: ssh:// rewritten"
grep -qx 'GIT_CONFIG_VALUE_1=git@gitcode.com:' "$(uv_env 1)" || fail "PAT: scp-style rewritten"
grep -q '^GIT_ASKPASS=' "$(uv_env 1)" || fail "PAT: askpass set"
grep -qx 'token_set=1' "$(uv_env 1)" || fail "PAT: token reaches askpass"
token_leaked && fail "PAT printed by icode_uv_sync"
grep -q 'OK icode dependencies fetched over https with the gitcode.com PAT' "$FAKE_UV_DIR/err" \
  || fail "PAT: OK line"
ok "uv sync rewrites ssh:// deps to https with the iCode host PAT"

run_uv_sync "1" "$SENTINEL" || fail "SSH retry should pass"
[ "$(cat "$FAKE_UV_DIR/calls")" = 2 ] || fail "PAT failure: expected an SSH retry"
grep -q '^GIT_CONFIG_' "$(uv_env 2)" && fail "SSH retry must not rewrite to https"
grep -q 'token_set=1' "$(uv_env 2)" && fail "SSH retry must not carry the PAT"
grep -qx 'GIT_SSH_COMMAND=ssh -o BatchMode=yes' "$(uv_env 2)" || fail "SSH retry must not prompt"
grep -q 'retrying with this machine.s SSH key' "$FAKE_UV_DIR/err" || fail "SSH retry warning"
token_leaked && fail "PAT printed on retry"
ok "uv sync falls back to this machine's SSH key when the PAT fails"

run_uv_sync "" "" || fail "uv sync without a PAT should pass"
[ "$(cat "$FAKE_UV_DIR/calls")" = 1 ] || fail "no PAT: expected one uv call"
grep -q '^GIT_CONFIG_' "$(uv_env 1)" && fail "no PAT: nothing to rewrite with"
grep -q '^GIT_ASKPASS=' "$(uv_env 1)" && fail "no PAT: no askpass"
grep -qx 'GIT_SSH_COMMAND=ssh -o BatchMode=yes' "$(uv_env 1)" || fail "no PAT: SSH must not prompt"
ok "uv sync without a PAT uses SSH only"

printf '%s\n' 'source = { git = "ssh://git@gitcode.com/michaelling/agent-core.git?branch=icode#0f25d4d" }' >"$UV_TREE/uv.lock"
if run_uv_sync "1 2" "$SENTINEL"; then
  fail "both uv sync attempts failing must fail"
fi
grep -q 'iCode pins a git dependency over SSH' "$FAKE_UV_DIR/err" || fail "SSH-pinned dependency message"
token_leaked && fail "PAT printed in the failure message"
rm -f "$UV_TREE/uv.lock"
if run_uv_sync "1" ""; then
  fail "a failing uv sync must fail"
fi
grep -q "uv sync failed in $UV_TREE" "$FAKE_UV_DIR/err" || fail "plain uv sync failure message"
grep -q 'over SSH' "$FAKE_UV_DIR/err" && fail "no SSH pin: do not blame SSH"
ok "uv sync failure names the SSH-pinned dependency only when there is one"
for bench in deepswe lolbench swebenchpro; do
  BENCHMARK="$bench" icode_sourceless_enabled || fail "sourceless must default on for $bench"
done
ICODE_SOURCELESS_STDLIB=yes-please icode_sourceless_enabled || fail "unrecognized ICODE_SOURCELESS_STDLIB must keep sourceless on"
if OFFICIAL=0 ICODE_SOURCELESS_STDLIB=0 icode_sourceless_enabled; then
  fail "ICODE_SOURCELESS_STDLIB=0 must turn sourceless off for a non-official run"
fi
if (OFFICIAL=1 ICODE_SOURCELESS_STDLIB=off icode_sourceless_enabled) 2>"$KIND_TMP/sourceless-official.err"; then
  fail "OFFICIAL=1 must refuse ICODE_SOURCELESS_STDLIB=off"
fi
grep -q 'OFFICIAL=1 requires the sourceless stdlib' "$KIND_TMP/sourceless-official.err" \
  || fail "OFFICIAL=1 sourceless refusal message"
ok "sourceless stdlib on for every benchmark; OFFICIAL=1 refuses turning it off"

ICODE_MODE=binary bash "$IN" --normalize-mode | grep -qx release || fail "binary alias"
ICODE_MODE=git bash "$IN" --normalize-mode | grep -qx git || fail "git mode"
if ICODE_MODE=source bash "$IN" --normalize-mode >/dev/null 2>&1; then
  fail "expected source mode to fail"
fi
ok "normalize binary→release git→git; reject source"

bash "$IN" --token-env-for-host gitcode.com | grep -qx GITCODE_TOKEN || fail "gitcode token env"
bash "$IN" --token-env-for-host github.com | grep -qx GITHUB_TOKEN || fail "github token env"
must_fail "evil token host" --token-env-for-host github.com.evil.com
grep -q 'sandbox-cpython' "$IN" || fail "missing sandbox-cpython git wrapper"
grep -q 'icode_embed_sandbox_cpython' "$IN" || fail "missing embed helper"
grep -q 'gitignored .env' "$IN" || fail "missing private-clone PAT hint"
ok "token env names + fail-fast clone hints"

if grep -q 'get_local_icode' "$IN"; then
  fail "get_local_icode must be removed"
fi
grep -q 'icode_paths_get' "$IN" || fail "missing icode_paths_get"
ok "no local source helper; persist remains for release"

COMMON="$(cd "$(dirname "$0")/../stages" && pwd)/_common.sh"
grep -q 'icode_release_is_uploaded' "$COMMON" || fail "missing icode_release_is_uploaded"
grep -q 'ICODE_RELEASE_UPLOADED' "$COMMON" || fail "missing ICODE_RELEASE_UPLOADED"
ok "Jenkins unnamed upload helper"

echo "OK test_icode_input.sh"
