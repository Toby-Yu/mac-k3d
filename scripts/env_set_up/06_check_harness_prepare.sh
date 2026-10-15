#!/usr/bin/env bash
# Opt-in (RUN_HARNESS_SMOKE=1): prepare each harness for each task platform
# this worker runs, as a dispatcher's "Prepare harness" stage does
# (pipeline/tools/prepare_harness.sh). These are real builds: they need the
# network, and a PAT in .env or the environment for a private repo. The
# bundles stay in this worker's harness store, so the first eval reuses them.
#
#   HARNESSES   default "icode jiuwenswarm"
#   PLATFORMS   default this host's platform; on arm64 also linux/amd64
#   iCode and JiuwenSwarm inputs as the pipeline reads them: ICODE_MODE,
#   ICODE_GIT_URL, ICODE_GIT_REF, ICODE_RELEASE, JIUWENSWARM_MODE,
#   JIUWENSWARM_GIT_URL, JIUWENSWARM_GIT_REF, JIUWENSWARM_RELEASE, ...
set -euo pipefail
# shellcheck source=./_common.sh
source "$(cd "$(dirname "$0")" && pwd)/_common.sh"

if [ "${RUN_HARNESS_SMOKE:-0}" != 1 ]; then
  note "skipping the harness builds (set RUN_HARNESS_SMOKE=1 to run them)"
  pass "06_check_harness_prepare skipped"
  exit 0
fi

have docker || die "docker not on PATH"
PREPARE="$MAC_K3D_ROOT/pipeline/tools/prepare_harness.sh"
[ -f "$PREPARE" ] || die "no $PREPARE (run this from a mac-k3d checkout)"
HOST="$(docker_platform)"
if [ -z "${PLATFORMS:-}" ]; then
  PLATFORMS="$HOST"
  [ "$HOST" != linux/arm64 ] || PLATFORMS="linux/arm64 linux/amd64"
fi
HARNESSES="${HARNESSES:-icode jiuwenswarm}"
WORK="$(mktemp -d "${TMPDIR:-/tmp}/mac-k3d-harness-smoke.XXXXXX")"

for harness in $HARNESSES; do
  for plat in $PLATFORMS; do
    run="$WORK/$harness-${plat#linux/}"
    log="$run.log"
    echo "prepare $harness for $plat (log $log) …"
    if ! MAC_K3D_EVAL_WORKDIR="$run" HARNESS="$harness" BENCHMARK=deepswe HARNESS_BUNDLE_PLATFORM="$plat" \
      bash "$PREPARE" >"$log" 2>&1; then
      tail -n 30 "$log" >&2
      die "$harness did not prepare for $plat (full log $log)"
    fi
    got="$(python3 -c 'import json, sys; print(json.load(open(sys.argv[1]))["platform"])' "$run/harness/bundle.json")"
    case "$got" in
      "$plat" | any) ;;
      *) die "$harness for $plat made a bundle for $got" ;;
    esac
    grep 'under test:' "$log" || true
    pass "$harness prepared for $plat: bundle $(cut -c1-16 "$run/harness/bundle.sha256") ($got)"
  done
done

chmod -R u+w "$WORK" 2>/dev/null || true
rm -rf "$WORK"
pass "06_check_harness_prepare complete"
