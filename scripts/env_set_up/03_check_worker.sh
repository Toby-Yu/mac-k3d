#!/usr/bin/env bash
# Verify Jenkins worker agent config and daemon (optional if REQUIRE_WORKER=0 and no worker.yaml).
set -euo pipefail
# shellcheck source=./_common.sh
source "$(cd "$(dirname "$0")" && pwd)/_common.sh"

require_mac_k3d

REQUIRE_WORKER="${REQUIRE_WORKER:-0}"

if [ ! -f "$WORKER_CONFIG" ]; then
  if [ "$REQUIRE_WORKER" = "1" ]; then
    die "missing worker config: $WORKER_CONFIG (REQUIRE_WORKER=1)"
  fi
  note "no worker config at $WORKER_CONFIG — SKIP worker checks"
  pass "03_check_worker skipped (no worker.yaml)"
  exit 0
fi

# start on worker YAML must be rejected
set +e
START_OUT="$("$MAC_K3D_BIN" start -c "$WORKER_CONFIG" 2>&1)"
START_RC=$?
set -e
if [ "$START_RC" -eq 0 ]; then
  die "start -c worker.yaml unexpectedly succeeded (workers must not start k3d)"
fi
echo "$START_OUT" | grep -qi 'Workers use' || echo "$START_OUT" | grep -qi 'worker' \
  || note "start failed (good) but message did not mention worker — exit was $START_RC"
pass "worker start rejected (exit $START_RC)"

STATUS_OUT="$("$MAC_K3D_BIN" status -c "$WORKER_CONFIG" 2>&1)" || true
echo "$STATUS_OUT"
echo "$STATUS_OUT" | grep -qi 'Role:.*worker' || die "status does not report worker role"
pass "worker status role=worker"

AGENT_OK=0
if have systemctl; then
  if systemctl --user is-active mac-k3d-jenkins-agent.service >/dev/null 2>&1; then
    pass "Linux agent unit mac-k3d-jenkins-agent.service active"
    AGENT_OK=1
  fi
fi
if [ "$(uname -s)" = "Darwin" ]; then
  if launchctl print "gui/$(id -u)/com.mac-k3d.jenkins-agent" >/dev/null 2>&1 \
    || launchctl list 2>/dev/null | grep -q 'com.mac-k3d.jenkins-agent'; then
    pass "macOS LaunchAgent com.mac-k3d.jenkins-agent loaded"
    AGENT_OK=1
  fi
fi

if [ "$AGENT_OK" -ne 1 ]; then
  if [ "$REQUIRE_WORKER" = "1" ]; then
    die "Jenkins agent daemon not active (set api_token and run: mac-k3d config -c $WORKER_CONFIG)"
  fi
  note "agent daemon not active — paste API token and re-run worker config"
  exit 1
fi

pass "03_check_worker complete"
