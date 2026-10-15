#!/usr/bin/env bash
# Runs inside ICODE_LINUX_BUILD_IMAGE (jiuwenswarm_input.sh starts it through
# hgit_linux_run, which mounts the uv cache and sets UV_PYTHON_INSTALL_DIR to
# it): builds the JiuwenSwarm runtime at /opt/jiuwenswarm-host, the path a
# trial mounts it at, so every link and script in it is valid there.
#
#   jiuwenswarm_build.sh git                 the checkout is /mac-k3d-src: its own
#                                            Linux offline pack (OFFLINE_CI_PLATFORMS=
#                                            JIUWENSWARM_OFFLINE_PLATFORM make offline-tarballs-ci)
#                                            when its Makefile has that target, else uv sync
#   jiuwenswarm_build.sh wheel WHEEL         one workswarm wheel; dependencies from PyPI
#   jiuwenswarm_build.sh wheelhouse DIR WHEEL  WHEEL and its dependencies from DIR only
#
# JIUWENSWARM_PYTHON is the CPython minor version and JIUWENSWARM_PYTHON_PLATFORM
# the oldest glibc the wheels may need (uv --python-platform), so the runtime
# also starts in task images older than this build image. The offline pack
# pins the same glibc itself (scripts/offline/lib.sh).
#
# /opt/jiuwenswarm-host is the container's own tmpfs and the checkout is
# copied to /tmp/src, so the build never writes to a macOS worker's
# case-insensitive disk. Once the probes pass, the tree is copied to
# JIUWENSWARM_OUT, the host directory the build mounts at its own path.
set -euo pipefail

ROOT=/opt/jiuwenswarm-host
OUT="${JIUWENSWARM_OUT:?}"
SRC=/tmp/src
PY_VERSION="${JIUWENSWARM_PYTHON:?}"
PY_PLATFORM="${JIUWENSWARM_PYTHON_PLATFORM:?}"
PY_CACHE="${UV_PYTHON_INSTALL_DIR:?}"
export UV_PYTHON_PREFERENCE=only-managed
export UV_COMPILE_BYTECODE=1
export UV_LINK_MODE=copy
unset VIRTUAL_ENV

# CPython comes from this worker's uv cache (downloaded once per platform) and
# is copied into the tree, where uv then finds it as its only managed python.
managed_python() {
  local cached prefix
  uv python install "$PY_VERSION"
  cached="$(uv python find "$PY_VERSION")"
  cached="$(python3 -c 'import os, sys; print(os.path.realpath(sys.argv[1]))' "$cached")"
  prefix="$(cd "$(dirname "$cached")/.." && pwd)"
  case "$prefix" in
    "$PY_CACHE"/*) ;;
    *) echo "ERROR: uv found python $PY_VERSION at $cached, not in its cache $PY_CACHE" >&2; exit 1 ;;
  esac
  mkdir -p "$ROOT/python"
  rm -rf "${ROOT:?}/python/$(basename "$prefix")"
  cp -a "$prefix" "$ROOT/python/"
  export UV_PYTHON_INSTALL_DIR="$ROOT/python"
}

# The checkout's offline pack for one platform, unpacked as the tree: BUNDLE,
# bin/ (launchers that run python -I, and rg) and python/ (CPython with
# workswarm and every locked dependency in its site-packages).
offline_pack() {
  local plat="${JIUWENSWARM_OFFLINE_PLATFORM:?}" pack tops log status
  local -a packs
  cd "$SRC"
  rm -rf dist/offline
  # The pack prunes __pycache__, so bytecode is compiled after unpacking.
  # setuptools lists every file it copies into the wheel, three times. The
  # Jenkins console keeps the pack's own [offline] lines, the tarball summary,
  # and uv's one-line totals. The full log is shown only when the build fails.
  log="$(mktemp)"
  set +e
  env -u UV_COMPILE_BYTECODE OFFLINE_CI_PLATFORMS="$plat" OFFLINE_PYTHON_VERSION="$PY_VERSION" \
    make offline-tarballs-ci 2>&1 \
    | tee "$log" \
    | grep -E --line-buffered \
      '^\[offline\] |^  (bundle|tarball|sha256|sums):|^Successfully built |^(Resolved|Prepared|Installed|Uninstalled|Using Python) '
  status=${PIPESTATUS[0]}
  set -e
  if [ "$status" -ne 0 ]; then
    echo "ERROR: make offline-tarballs-ci failed (exit ${status}); last 80 lines:" >&2
    tail -n 80 "$log" >&2
    rm -f "$log"
    exit "$status"
  fi
  rm -f "$log"
  packs=(dist/offline/*-process-"$plat"-*.tar.gz)
  if [ "${#packs[@]}" != 1 ] || [ ! -f "${packs[0]}" ]; then
    echo "ERROR: make offline-tarballs-ci left no single dist/offline/*-process-$plat-*.tar.gz" >&2
    exit 1
  fi
  pack="${packs[0]}"
  tops="$(tar -tzf "$pack" | awk -F/ 'NF && !seen[$1]++ { print $1 }')"
  if [ "$(printf '%s\n' "$tops" | wc -l)" != 1 ]; then
    echo "ERROR: $pack does not hold one top directory: $(printf '%s\n' "$tops" | tr '\n' ' ')" >&2
    exit 1
  fi
  tar -xzf "$pack" -C "$ROOT" --strip-components=1
  for f in BUNDLE bin/jiuwenswarm-process bin/jiuwenswarm-python python/bin/python3; do
    [ -e "$ROOT/$f" ] || { echo "ERROR: $pack has no $tops/$f" >&2; exit 1; }
  done
  # The trial mounts the tree read-only, and python -I ignores PYTHONDONTWRITEBYTECODE.
  "$ROOT/bin/jiuwenswarm-python" -m compileall -q -j 0 "$ROOT/python/lib" >/dev/null \
    || echo "note: compileall could not compile every file under $ROOT/python/lib" >&2
  echo "jiuwenswarm: offline pack $(basename "$pack") ($(sed -n 's/^version=//p' "$ROOT/BUNDLE"))" >&2
}

has_offline_pack() {
  [ -f "$SRC/Makefile" ] && grep -q '^offline-tarballs-ci:' "$SRC/Makefile" \
    && [ -f "$SRC/scripts/offline/build-ci-tarballs.sh" ]
}

# CPython's terminfo database holds names that differ only in case (E/Eterm
# and e/eterm); the headless runner never reads it. Any other such pair fails
# the build: a macOS worker would merge it.
drop_case_collisions() {
  local found
  find "$ROOT" -type d -path '*/share/terminfo' -prune -exec rm -rf {} +
  found="$(cd "$ROOT" && find . | python3 -c '
import sys
seen = {}
for line in sys.stdin:
    path = line.rstrip("\n")
    seen.setdefault(path.lower(), []).append(path)
for group in seen.values():
    if len(group) > 1:
        print(" vs ".join(sorted(group)))
')"
  if [ -n "$found" ]; then
    echo "ERROR: the JiuwenSwarm tree has paths that differ only in case, which a macOS worker would merge:" >&2
    printf '%s\n' "$found" | head -n 10 >&2
    exit 1
  fi
}

case "${1:-}" in
  git)
    cp -a /mac-k3d-src "$SRC"
    if has_offline_pack; then
      BUILD=offline-tarballs-ci
      offline_pack
    else
      echo "jiuwenswarm: this commit has no make offline-tarballs-ci; building with uv sync --frozen" >&2
      BUILD=uv-sync
      managed_python
      cd "$SRC"
      UV_PROJECT_ENVIRONMENT="$ROOT/venv" uv sync --frozen --no-dev --no-editable \
        --python "$PY_VERSION" --python-platform "$PY_PLATFORM"
    fi
    ;;
  wheel)
    BUILD=wheel
    managed_python
    uv venv "$ROOT/venv" --python "$PY_VERSION"
    uv pip install --python "$ROOT/venv/bin/python" --python-platform "$PY_PLATFORM" "$2"
    ;;
  wheelhouse)
    BUILD=wheelhouse
    managed_python
    uv venv "$ROOT/venv" --python "$PY_VERSION"
    uv pip install --python "$ROOT/venv/bin/python" --python-platform "$PY_PLATFORM" \
      --no-index --find-links "$2" "$3"
    ;;
  *)
    echo "usage: jiuwenswarm_build.sh git | wheel WHEEL | wheelhouse DIR WHEEL" >&2
    exit 2
    ;;
esac

mkdir -p "$ROOT/bin"
if [ "$BUILD" = offline-tarballs-ci ]; then
  PY="$ROOT/python/bin/python3"
  RUNNER="/opt/jiuwenswarm-host/bin/jiuwenswarm-process"
else
  PY="$ROOT/venv/bin/python"
  RUNNER="/opt/jiuwenswarm-host/venv/bin/jiuwenswarm-process"
  [ -x "$ROOT/venv/bin/jiuwenswarm-process" ] || {
    echo "ERROR: the install has no jiuwenswarm-process console script" >&2
    exit 1
  }
  cat >"$ROOT/bin/jiuwenswarm-python" <<'WRAP'
#!/bin/sh
exec /opt/jiuwenswarm-host/venv/bin/python "$@"
WRAP
fi
cat >"$ROOT/bin/jiuwenswarm-run" <<WRAP
#!/bin/sh
# JiuwenSwarm's headless runner. It keeps the trial's working directory off
# sys.path, and nothing here is exported to the agent's tools.
exec $RUNNER "\$@"
WRAP
chmod 755 "$ROOT/bin/jiuwenswarm-run" "$ROOT/bin/jiuwenswarm-python"

drop_case_collisions
cd "$(mktemp -d)"
uv pip freeze --python "$PY" >"$ROOT/freeze.txt"
"$PY" -I - "$ROOT/runtime.json" "$PY_PLATFORM" "$BUILD" "$ROOT/BUNDLE" <<'PY'
import importlib.metadata as md
import json
import platform
import sys
from pathlib import Path

out, wheel_platform, build, bundle = sys.argv[1:5]
doc = {
    "package": "workswarm",
    "version": md.version("workswarm"),
    "python": platform.python_version(),
    "python_platform": wheel_platform,
    "machine": platform.machine(),
    "build": build,
}
if Path(bundle).is_file():
    doc["bundle"] = dict(
        ln.split("=", 1) for ln in Path(bundle).read_text(encoding="utf-8").splitlines() if "=" in ln
    )
with open(out, "w", encoding="utf-8") as fh:
    json.dump(doc, fh, indent=2)
    fh.write("\n")
PY

# The probes run from an empty directory with nothing exported, as in a trial.
env -u PYTHONPATH PYTHONDONTWRITEBYTECODE=1 "$ROOT/bin/jiuwenswarm-run" --help >/dev/null
env -u PYTHONPATH PYTHONDONTWRITEBYTECODE=1 "$ROOT/bin/jiuwenswarm-python" -c \
  'import jiuwenswarm.channels.process_cli.machine_entry, jiuwenswarm.common.utils'
echo "OK JiuwenSwarm runtime $("$PY" -I -c 'import json,sys; print(json.load(open(sys.argv[1]))["version"])' "$ROOT/runtime.json") ($BUILD) starts in $ROOT"
cp -a "$ROOT"/. "$OUT"/
