#!/usr/bin/env bash
# Host-side iCode inputs for P3 (sourced by p3_icode.sh).
# get_release_icode: official *-full-* drop / URL (not a git clone of Release assets).
# get_bin_icode: allow-listed https git URL + tag/commit/branch, uv sync, Docker wrapper.
# Does not print tokens.

if ! declare -F die >/dev/null 2>&1; then
  die() {
    echo "ERROR: $*" >&2
    exit 1
  }
fi
if ! declare -F have >/dev/null 2>&1; then
  have() { command -v "$1" >/dev/null 2>&1; }
fi

icode_normalize_mode() {
  case "${ICODE_MODE:-binary}" in
    binary | release) printf '%s\n' release ;;
    git) printf '%s\n' git ;;
    source)
      die "ICODE_MODE=source is not supported. Push to GitHub/GitCode and use git (clone) or download a *-full-* binary and use release."
      ;;
    *) die "unknown ICODE_MODE=${ICODE_MODE} (use release or git; binary is an alias of release)" ;;
  esac
}

icode_git_host_allowed() {
  case "$1" in
    github.com | www.github.com | gitcode.com) return 0 ;;
    *) return 1 ;;
  esac
}

icode_validate_git_url() {
  local url="${1:-}" rest host
  [ -n "$url" ] || die "ICODE_GIT_URL is empty"
  case "$url" in
    https://*) ;;
    *) die "ICODE_GIT_URL must be https:// (not file://, ssh://, or http://)" ;;
  esac
  rest="${url#https://}"
  case "$rest" in
    *@*) die "ICODE_GIT_URL must not contain userinfo (no tokens in the URL)" ;;
  esac
  host="${rest%%/*}"
  host="${host%%:*}"
  icode_git_host_allowed "$host" || die "ICODE_GIT_URL host '${host}' is not allow-listed (github.com, www.github.com, gitcode.com)"
}

icode_validate_git_ref() {
  local ref="${1:-}"
  [ -n "$ref" ] || die "ICODE_GIT_REF is empty"
  case "$ref" in
    -*) die "ICODE_GIT_REF must not start with -" ;;
  esac
  case "$ref" in
    *..*) die "ICODE_GIT_REF must not contain .." ;;
  esac
  [[ "$ref" =~ ^[A-Za-z0-9._/+\-]+$ ]] || die "ICODE_GIT_REF is not a tag, commit, or branch name"
}

icode_looks_like_commit() {
  [[ "${1:-}" =~ ^[0-9a-fA-F]{7,40}$ ]]
}

icode_normalize_ref_kind() {
  local k="${1:-auto}" ref="${2:-}"
  k="$(printf '%s' "$k" | tr 'A-Z' 'a-z')"
  case "$k" in
    branch | tag | commit)
      printf '%s\n' "$k"
      ;;
    auto | "")
      if icode_looks_like_commit "$ref"; then
        printf '%s\n' commit
      else
        printf '%s\n' branch
      fi
      ;;
    *)
      die "ICODE_GIT_REF_KIND must be branch, tag, or commit (got ${k})"
      ;;
  esac
}

icode_validate_ref_for_kind() {
  local kind="$1" ref="$2"
  case "$kind" in
    commit)
      icode_looks_like_commit "$ref" || die "ICODE_GIT_REF_KIND=commit requires a git SHA (7–40 hex chars), not a branch name. Use kind=branch for '${ref}'."
      ;;
  esac
}

icode_record_git_meta() {
  local dest="$1" url="$2" kind="$3" ref="$4" sha subject out
  sha="$(git -C "$dest" rev-parse HEAD 2>/dev/null || true)"
  subject="$(git -C "$dest" log -1 --format=%s 2>/dev/null || true)"
  [ -n "${WORKDIR:-}" ] || die "WORKDIR is unset; cannot write icode_git.json"
  out="$WORKDIR/icode_git.json"
  python3 - "$out" "$url" "$kind" "$ref" "$sha" "$subject" <<'PY'
import json, sys
path, url, kind, ref, sha, subject = sys.argv[1:7]
doc = {
    "url": url,
    "kind": kind,
    "ref": ref,
    "sha": sha,
    "subject": subject,
}
open(path, "w", encoding="utf-8").write(json.dumps(doc, indent=2) + "\n")
PY
}

icode_docker_platform() {
  case "$(uname -m)" in
    aarch64 | arm64) printf '%s\n' linux/arm64 ;;
    *) printf '%s\n' linux/amd64 ;;
  esac
}

# Delete a leftover tree/file even when Docker left root/nobody-owned files.
# Plain rm first; if that fails, alpine as root on a parent bind-mount (no sudo).
icode_force_rm() {
  local path="${1:-}" parent base plat
  [ -n "$path" ] || return 0
  [ -e "$path" ] || [ -L "$path" ] || return 0
  rm -rf "$path" 2>/dev/null || true
  [ -e "$path" ] || [ -L "$path" ] || return 0
  have docker || die "cannot remove leftover ${path} (root/nobody files from a prior sandbox). Docker is required to delete it."
  parent="$(cd "$(dirname "$path")" && pwd)" || die "cannot resolve parent of ${path}"
  base="$(basename "$path")"
  case "$base" in
    '' | . | .. | /) die "refusing to docker-rm unsafe path ${path}" ;;
  esac
  [ "$parent" != "/" ] || die "refusing to docker-rm under /"
  plat="$(icode_docker_platform)"
  echo "removing leftover ${path} via docker (prior sandbox left root/nobody files)" >&2
  docker run --rm --platform "$plat" -v "${parent}:/mac-k3d-wipe" alpine:3.20 rm -rf "/mac-k3d-wipe/${base}"
  if [ -e "$path" ] || [ -L "$path" ]; then
    die "cannot remove leftover ${path} (docker rm of root/nobody sandbox files failed)"
  fi
}

# After Pier/Harbor, give the bind-mounted host tree back to the invoking user.
icode_reclaim_host_tree() {
  local dest="${1:-}" parent base plat uid gid
  [ -n "$dest" ] && [ -d "$dest" ] || return 0
  have docker || return 0
  parent="$(cd "$(dirname "$dest")" && pwd)" || return 0
  base="$(basename "$dest")"
  case "$base" in
    '' | . | .. | /) return 0 ;;
  esac
  [ "$parent" != "/" ] || return 0
  uid="$(id -u)"
  gid="$(id -g)"
  plat="$(icode_docker_platform)"
  docker run --rm --platform "$plat" -v "${parent}:/mac-k3d-wipe" alpine:3.20 chown -R "${uid}:${gid}" "/mac-k3d-wipe/${base}" >/dev/null 2>&1 || true
}

icode_git_checkout() {
  local dest="$1" url="$2" ref="$3" kind="$4" tok fetched
  local -a git_cmd
  tok="${MAC_K3D_GIT_TOKEN:-}"
  git_cmd=(git -c core.hooksPath=/dev/null -c init.defaultBranch=main -c advice.defaultBranchName=false -c credential.helper= -c http.lowSpeedLimit=1000 -c http.lowSpeedTime=30)
  if [ "${MAC_K3D_ICODE_GIT_ALLOW_FILE:-}" != 1 ]; then
    git_cmd+=(-c protocol.file.allow=never)
  fi
  if [ -e "$dest" ]; then
    icode_force_rm "$dest"
  fi
  mkdir -p "$dest"
  icode_git "${git_cmd[@]}" init "$dest" >/dev/null || icode_git_clone_fail "$url" "$ref" "$tok"
  icode_git "${git_cmd[@]}" -C "$dest" remote add origin "$url" || icode_git_clone_fail "$url" "$ref" "$tok"
  case "$kind" in
    commit)
      fetched=0
      if icode_git "${git_cmd[@]}" -C "$dest" fetch --depth 1 origin "$ref" >/dev/null 2>&1 \
        || icode_git "${git_cmd[@]}" -C "$dest" fetch origin "$ref" >/dev/null 2>&1 \
        || icode_git "${git_cmd[@]}" -C "$dest" fetch origin "+${ref}:refs/commits/${ref}" >/dev/null 2>&1; then
        fetched=1
      fi
      if [ "$fetched" -eq 0 ]; then
        # Abbreviated SHA: fetch advertised heads/tags, then resolve the object locally.
        icode_git "${git_cmd[@]}" -C "$dest" fetch origin '+refs/heads/*:refs/remotes/origin/*' '+refs/tags/*:refs/tags/*' || icode_git_clone_fail "$url" "$ref" "$tok"
      fi
      icode_git "${git_cmd[@]}" -C "$dest" checkout --detach "$ref" || icode_git "${git_cmd[@]}" -C "$dest" checkout --detach FETCH_HEAD || icode_git_clone_fail "$url" "$ref" "$tok"
      ;;
    tag)
      if ! icode_git "${git_cmd[@]}" -C "$dest" fetch --depth 1 origin "refs/tags/${ref}:refs/tags/${ref}"; then
        icode_git "${git_cmd[@]}" -C "$dest" fetch --depth 1 origin tag "$ref" || icode_git "${git_cmd[@]}" -C "$dest" fetch origin "refs/tags/${ref}:refs/tags/${ref}" || icode_git_clone_fail "$url" "$ref" "$tok"
      fi
      icode_git "${git_cmd[@]}" -C "$dest" checkout --detach "refs/tags/${ref}" || icode_git "${git_cmd[@]}" -C "$dest" checkout --detach FETCH_HEAD || icode_git_clone_fail "$url" "$ref" "$tok"
      ;;
    *)
      if ! icode_git "${git_cmd[@]}" -C "$dest" fetch --depth 1 origin "${ref}:refs/remotes/origin/${ref}"; then
        icode_git "${git_cmd[@]}" -C "$dest" fetch --depth 1 origin "$ref" || icode_git "${git_cmd[@]}" -C "$dest" fetch origin "$ref" || icode_git_clone_fail "$url" "$ref" "$tok"
      fi
      icode_git "${git_cmd[@]}" -C "$dest" checkout -B "$ref" FETCH_HEAD || icode_git "${git_cmd[@]}" -C "$dest" checkout --detach FETCH_HEAD || icode_git_clone_fail "$url" "$ref" "$tok"
      ;;
  esac
}

icode_write_wrapper() {
  local dest="$1"
  cat >"$dest" <<'WRAP'
#!/bin/sh
# Bind-mounted git venv: host uv CPython is not in the sandbox. P3 copies it to
# .venv/sandbox-cpython. Always use the mount path so Harbor/Pier `cp` of this
# file to /usr/local/bin/icode still works.
ROOT="${ICODE_SOURCE_HOST:-/opt/icode-host}"
PY="$ROOT/.venv/sandbox-cpython/bin/python3.13"
[ -x "$PY" ] || PY="$ROOT/.venv/sandbox-cpython/bin/python3"
[ -x "$PY" ] || PY="$ROOT/.venv/sandbox-cpython/bin/python"
if [ ! -x "$PY" ]; then
  echo "icode git sandbox python missing under $ROOT/.venv/sandbox-cpython" >&2
  exit 127
fi
SITE="$ROOT/.venv/lib/python3.13/site-packages"
if [ ! -d "$SITE" ]; then
  SITE="$(ls -d "$ROOT/.venv/lib"/python3.*/site-packages 2>/dev/null | head -1)"
fi
# Clone root first: uv editable installs map to the host path, which Docker does not have.
export VIRTUAL_ENV="$ROOT/.venv"
if [ -n "$SITE" ]; then
  export PYTHONPATH="${ROOT}:${SITE}${PYTHONPATH:+:$PYTHONPATH}"
else
  export PYTHONPATH="${ROOT}${PYTHONPATH:+:$PYTHONPATH}"
fi
exec "$PY" "$ROOT/.venv/bin/icode" "$@"
WRAP
  chmod +x "$dest"
}

# Host uv venv python is a symlink to ~/.local/share/uv/python/... which Docker
# does not have. Copy that CPython prefix into the clone for bind-mount eval.
icode_embed_sandbox_cpython() {
  local dest="$1" py prefix sandbox
  [ -n "$dest" ] && [ -d "$dest" ] || return 0
  dest="$(cd "$dest" && pwd)"
  [ -x "$dest/.venv/bin/icode" ] || return 0
  if [ -x "$dest/.venv/sandbox-cpython/bin/python3.13" ] || [ -x "$dest/.venv/sandbox-cpython/bin/python3" ] || [ -x "$dest/.venv/sandbox-cpython/bin/python" ]; then
    return 0
  fi
  py="$(readlink -f "$dest/.venv/bin/python" 2>/dev/null || true)"
  [ -n "$py" ] && [ -x "$py" ] || return 0
  case "$py" in
    "$dest"/*) return 0 ;;
  esac
  prefix="$(cd "$(dirname "$py")/.." && pwd)"
  [ -d "$prefix/lib" ] && [ -d "$prefix/bin" ] || return 0
  sandbox="$dest/.venv/sandbox-cpython"
  mkdir -p "$sandbox"
  cp -a "$prefix/bin" "$sandbox/bin"
  cp -a "$prefix/lib" "$sandbox/lib"
  [ -d "$prefix/include" ] && cp -a "$prefix/include" "$sandbox/include" || true
}

icode_write_host_root() {
  printf '%s\n' "$1" >"$WORKDIR/icode_host_root.txt"
}

icode_write_askpass() {
  local f="$WORKDIR/.git-askpass"
  umask 077
  cat >"$f" <<'ASK'
#!/bin/sh
case "$1" in
  *[Uu]sername*) printf '%s\n' "${MAC_K3D_GIT_USERNAME:-x-access-token}" ;;
  *) printf '%s\n' "${MAC_K3D_GIT_TOKEN:-}" ;;
esac
ASK
  chmod 700 "$f"
  printf '%s\n' "$f"
}

icode_token_env_for_host() {
  case "$1" in
    github.com | www.github.com) printf '%s\n' GITHUB_TOKEN ;;
    gitcode.com) printf '%s\n' GITCODE_TOKEN ;;
    *) die "ICODE_GIT_URL host '${1}' is not allow-listed (github.com, www.github.com, gitcode.com)" ;;
  esac
}

icode_resolve_git_token() {
  local host="$1"
  case "$host" in
    github.com | www.github.com) printf '%s\n' "${GITHUB_TOKEN:-${MAC_K3D_GITHUB_PAT:-}}" ;;
    gitcode.com) printf '%s\n' "${GITCODE_TOKEN:-${MAC_K3D_GITCODE_PAT:-}}" ;;
    *) printf '%s\n' "" ;;
  esac
}

icode_git_username_for_host() {
  if [ -n "${MAC_K3D_GIT_USERNAME:-}" ]; then
    printf '%s\n' "$MAC_K3D_GIT_USERNAME"
    return 0
  fi
  case "$1" in
    gitcode.com) printf '%s\n' oauth2 ;;
    *) printf '%s\n' x-access-token ;;
  esac
}

# Fail-fast: no credential helper prompt, abort stalled HTTPS instead of hanging.
icode_git() {
  local rc
  if have timeout; then
    timeout 90 "$@"
    rc=$?
    if [ "$rc" -eq 124 ]; then
      echo "ERROR: git timed out after 90s (often a private repo with no PAT)" >&2
      return 124
    fi
    return "$rc"
  fi
  "$@"
}

icode_git_clone_fail() {
  local url="$1" ref="$2" token="$3"
  if [ -z "$token" ]; then
    die "git clone failed for ${url}. If the repo is private, store GITCODE_TOKEN (gitcode.com) or GITHUB_TOKEN (github.com) in gitignored .env (chmod 600), or Jenkins credential gitcode-pat / github-pat. Do not put the token in the URL or paste it into chat. GitCode username override: MAC_K3D_GIT_USERNAME in .env."
  fi
  die "git clone failed for ${url} (ref ${ref}). Check URL/ref and that the PAT can read the repo. Token not printed."
}

icode_paths_file() {
  if [ -n "${MAC_K3D_ICODE_PATHS:-}" ]; then
    printf '%s\n' "$MAC_K3D_ICODE_PATHS"
    return 0
  fi
  printf '%s\n' "${XDG_CONFIG_HOME:-$HOME/.config}/mac-k3d/icode-paths.yaml"
}

# Read source|release from worker icode-paths.yaml. Empty / missing → return 1.
icode_paths_get() {
  local key="$1" file val
  file="$(icode_paths_file)"
  [ -f "$file" ] || return 1
  val="$(python3 - "$file" "$key" <<'PY'
import sys
path, key = sys.argv[1], sys.argv[2]
try:
    text = open(path, encoding="utf-8").read()
except OSError:
    sys.exit(1)
for line in text.splitlines():
    s = line.strip()
    if not s or s.startswith("#") or ":" not in s:
        continue
    k, _, v = s.partition(":")
    if k.strip() != key:
        continue
    v = v.strip().strip("'").strip('"')
    if v:
        print(v)
        sys.exit(0)
    sys.exit(1)
sys.exit(1)
PY
)" || return 1
  [ -n "$val" ] || return 1
  printf '%s\n' "$val"
}

# Parent folder of a *-full-* drop (e.g. iCode-binary/) → the child drop.
icode_resolve_release_src() {
  local src="$1" c
  [ -n "$src" ] || return 1
  if [ -d "$src" ]; then
    if declare -F _icode_dir_has_icode >/dev/null 2>&1 && _icode_dir_has_icode "$src"; then
      printf '%s\n' "$src"
      return 0
    fi
    if [ -f "$src/icode" ]; then
      printf '%s\n' "$src"
      return 0
    fi
    for c in "$src"/*-full-*.tar.gz "$src"/*.tgz "$src"/*-full-*; do
      [ -e "$c" ] || continue
      if declare -F _icode_release_ok >/dev/null 2>&1; then
        _icode_release_ok "$c" || continue
      fi
      printf '%s\n' "$c"
      return 0
    done
    die "ICODE_RELEASE directory has no icode and no *-full-* drop: $src. Point at the *-full-* file or folder, or run: mac-k3d set --icode-release <path>"
  fi
  printf '%s\n' "$src"
}

# Official *-full-* path/URL/persist/discover. Prints ICODE_BIN path.
get_release_icode() {
  local unpack src bin
  if [ -z "${ICODE_RELEASE:-}" ] && [ "${ICODE_RELEASE_UPLOADED:-}" != 1 ]; then
    ICODE_RELEASE="$(icode_paths_get release || true)"
  fi
  if [ -z "${ICODE_RELEASE:-}" ] && [ "${ICODE_RELEASE_UPLOADED:-}" != 1 ] && declare -F discover_icode_release >/dev/null 2>&1; then
    ICODE_RELEASE="$(discover_icode_release || true)"
  fi
  if [ -z "${ICODE_RELEASE:-}" ]; then
    die "ICODE_MODE=release: Jenkins: upload ICODE_RELEASE_FILE on Build with Parameters. Local: set ICODE_RELEASE, run mac-k3d set --icode-release <path>, or place icode / icode-<os>-<arch>-full-vX.Y.Z at ${MAC_K3D_SHARE:-$HOME/.local/share/mac-k3d}/ (or /opt/mac-k3d/)"
  fi
  unpack="$WORKDIR/icode-bin"
  src="$ICODE_RELEASE"
  rm -rf "$unpack"
  mkdir -p "$unpack"
  case "$src" in
    http://* | https://*)
      curl -fsSL "$src" -o "$WORKDIR/icode-release.bin"
      src="$WORKDIR/icode-release.bin"
      ;;
    *)
      src="$(icode_resolve_release_src "$src")"
      ;;
  esac
  install_icode_release "$src" "$unpack"
  bin="$(find "$unpack" -type f -name icode -print -quit)"
  [ -n "$bin" ] || die "could not find icode inside release"
  icode_write_host_root "$(cd "$(dirname "$bin")" && pwd)"
  printf '%s\n' "$bin"
}

# Clone allow-listed URL at tag/commit/branch, uv sync, wrapper. Prints ICODE_BIN path.
# Tests: MAC_K3D_ICODE_FETCH_DIR skips clone (must already contain .venv/bin/icode or pyproject).
get_bin_icode() {
  local dest="$WORKDIR/icode-src" url ref host token kind
  if [ -n "${MAC_K3D_ICODE_FETCH_DIR:-}" ]; then
    [ -d "$MAC_K3D_ICODE_FETCH_DIR" ] || die "MAC_K3D_ICODE_FETCH_DIR is not a directory"
    dest="$(cd "$MAC_K3D_ICODE_FETCH_DIR" && pwd)"
  else
    url="${ICODE_GIT_URL:-}"
    icode_validate_git_url "$url"
    ref="${ICODE_GIT_REF:-main}"
    [ -n "$ref" ] || ref=main
    icode_validate_git_ref "$ref"
    kind="$(icode_normalize_ref_kind "${ICODE_GIT_REF_KIND:-auto}" "$ref")"
    icode_validate_ref_for_kind "$kind" "$ref"
    host="${url#https://}"
    host="${host%%/*}"
    host="${host%%:*}"
    mkdir -p "$(dirname "$dest")"
    export GIT_TERMINAL_PROMPT=0
    export GIT_CONFIG_NOSYSTEM=1
    token="$(icode_resolve_git_token "$host")"
    export MAC_K3D_GIT_USERNAME="$(icode_git_username_for_host "$host")"
    if [ -n "$token" ]; then
      export MAC_K3D_GIT_TOKEN="$token"
      GIT_ASKPASS="$(icode_write_askpass)"
      export GIT_ASKPASS
    fi
    icode_git_checkout "$dest" "$url" "$ref" "$kind"
    unset MAC_K3D_GIT_TOKEN GIT_ASKPASS || true
    icode_record_git_meta "$dest" "$url" "$kind" "$ref"
  fi
  [ -d "$dest" ] || die "icode git checkout missing: $dest"
  if [ ! -x "$dest/.venv/bin/icode" ]; then
    have uv || die "need uv or $dest/.venv/bin/icode"
    (cd "$dest" && uv sync --no-editable)
  fi
  [ -x "$dest/.venv/bin/icode" ] || die "uv sync did not produce $dest/.venv/bin/icode"
  icode_embed_sandbox_cpython "$dest"
  icode_write_wrapper "$dest/icode"
  icode_write_host_root "$dest"
  printf '%s\n' "$dest/icode"
}

if [ "${BASH_SOURCE[0]}" = "$0" ]; then
  case "${1:-}" in
    --validate-url)
      icode_validate_git_url "${2:-}"
      echo OK
      ;;
    --validate-ref)
      icode_validate_git_ref "${2:-}"
      echo OK
      ;;
    --normalize-mode)
      icode_normalize_mode
      ;;
    --token-env-for-host)
      icode_token_env_for_host "${2:-}"
      ;;
    --normalize-ref-kind)
      ICODE_GIT_REF_KIND="${2:-auto}"
      icode_normalize_ref_kind "${2:-auto}" "${3:-}"
      ;;
      --validate-ref-kind)
      kind="$(icode_normalize_ref_kind "${2:-}" "${3:-}")"
      icode_validate_ref_for_kind "$kind" "${3:-}"
      echo OK
      ;;
    --force-rm)
      icode_force_rm "${2:-}"
      echo OK
      ;;
    *)
      echo "usage: icode_input.sh --validate-url URL | --validate-ref REF | --normalize-mode | --token-env-for-host HOST | --normalize-ref-kind KIND [REF] | --validate-ref-kind KIND REF | --force-rm PATH" >&2
      exit 2
      ;;
  esac
fi
