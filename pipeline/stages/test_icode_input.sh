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
must_fail "bad kind" --normalize-ref-kind sha main
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
unset MAC_K3D_ICODE_GIT_ALLOW_FILE

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

COMMON="$(cd "$(dirname "$0")" && pwd)/_common.sh"
grep -q 'icode_release_is_uploaded' "$COMMON" || fail "missing icode_release_is_uploaded"
grep -q 'ICODE_RELEASE_UPLOADED' "$COMMON" || fail "missing ICODE_RELEASE_UPLOADED"
ok "Jenkins unnamed upload helper"

echo "OK test_icode_input.sh"
