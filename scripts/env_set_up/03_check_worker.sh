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

# The agent runs dependencies.java.binary from worker.yaml. launch-agent.sh is
# never read here: it holds the agent secret.
worker_java_binary() {
  awk '
    /^[^ #]/ { in_deps = ($0 ~ /^dependencies:/); in_java = 0; next }
    in_deps && /^  [^ ]/ { in_java = ($0 ~ /^  java:/); next }
    in_java && /^    binary:/ { sub(/^    binary:[ ]*/, ""); gsub(/["\047]/, ""); print; exit }
  ' "$1"
}
java_major() {
  local v
  v="$("$1" -version 2>&1 | grep -v '^Picked up' | head -n 1 | sed -E 's/^[^0-9"]*"?([0-9][0-9._]*).*/\1/')"
  case "$v" in 1.*) v="${v#1.}" ;; esac
  echo "${v%%[._]*}"
}
TOOLCHAIN_ENV="$MAC_K3D_ROOT/pipeline/config/toolchain.env"
[ -f "$TOOLCHAIN_ENV" ] || TOOLCHAIN_ENV="$HOME/.local/share/mac-k3d/pipeline/config/toolchain.env"
[ -f "$TOOLCHAIN_ENV" ] || die "no pipeline/config/toolchain.env; run: $MAC_K3D_BIN config -c $WORKER_CONFIG"
JAVA_WANT="$(sed -n 's/^JAVA_MAJOR=//p' "$TOOLCHAIN_ENV" | head -n 1)"
[ -n "$JAVA_WANT" ] || die "JAVA_MAJOR missing from $TOOLCHAIN_ENV"
JAVA_FIX="run: $MAC_K3D_BIN setup -c $WORKER_CONFIG and choose \"Use existing config\""
JAVA_BIN="$(worker_java_binary "$WORKER_CONFIG")"
case "$JAVA_BIN" in
  '' | null | '~') die "worker.yaml has no dependencies.java.binary; $JAVA_FIX" ;;
esac
[ -x "$JAVA_BIN" ] || die "agent java $JAVA_BIN not found; $JAVA_FIX"
JAVA_GOT="$(java_major "$JAVA_BIN")"
case "$JAVA_GOT" in
  '' | *[!0-9]*) die "could not read the Java version of $JAVA_BIN; Jenkins agents need Java $JAVA_WANT" ;;
esac
if [ "$JAVA_GOT" -lt "$JAVA_WANT" ]; then
  die "agent java $JAVA_GOT at $JAVA_BIN; the controller needs Java $JAVA_WANT (UnsupportedClassVersionError); $JAVA_FIX"
fi
pass "agent java $JAVA_GOT at $JAVA_BIN (need $JAVA_WANT+)"

AGENT_OK=0
if have systemctl; then
  if systemctl --user is-active mac-k3d-jenkins-agent.service >/dev/null 2>&1; then
    pass "Linux agent unit mac-k3d-jenkins-agent.service active"
    AGENT_OK=1
  fi
fi
if [ "$(uname -s)" = "Darwin" ]; then
  # Loaded is not enough: a LaunchAgent whose java exited is loaded and not running.
  LAUNCHD="$(launchctl print "gui/$(id -u)/com.mac-k3d.jenkins-agent" 2>/dev/null || true)"
  if printf '%s\n' "$LAUNCHD" | grep -q 'state = running'; then
    pass "macOS LaunchAgent com.mac-k3d.jenkins-agent running"
    AGENT_OK=1
  elif [ -n "$LAUNCHD" ]; then
    note "LaunchAgent com.mac-k3d.jenkins-agent is loaded but not running (see $(yaml_field "$WORKER_CONFIG" jenkins_agent remote_fs)/jenkins-agent.stderr.log)"
  fi
  # The agent and Docker Desktop live in the login session.
  if [ -z "$(defaults read /Library/Preferences/com.apple.loginwindow autoLoginUser 2>/dev/null || true)" ]; then
    note "automatic login is off: after a restart the agent and Docker Desktop stay down until someone logs in (System Settings > Users & Groups > Automatically log in as)"
  else
    pass "automatic login on"
  fi
  SLEEP_MIN="$(pmset -g 2>/dev/null | awk '$1 == "sleep" { print $2; exit }')"
  if [ -n "$SLEEP_MIN" ] && [ "$SLEEP_MIN" != 0 ]; then
    note "the Mac sleeps after ${SLEEP_MIN} minutes idle and drops its Jenkins connection mid-build (sudo pmset -a sleep 0)"
  else
    pass "the Mac does not sleep when idle"
  fi
fi

if [ "$AGENT_OK" -ne 1 ]; then
  if [ "$REQUIRE_WORKER" = "1" ]; then
    die "Jenkins agent daemon not active (set api_token and run: mac-k3d config -c $WORKER_CONFIG)"
  fi
  note "agent daemon not active — paste API token and re-run worker config"
  exit 1
fi

# The controller's view: the node is online and big enough for DeepSWE.
NODE="$(yaml_field "$WORKER_CONFIG" jenkins_agent name)"
if ! have_jenkins_token; then
  note "JENKINS_USER / JENKINS_API_TOKEN unset — skipping the node ${NODE:-?} online and fits-deepswe check"
elif [ -z "$NODE" ]; then
  die "worker.yaml has no jenkins_agent.name"
else
  NODE_JSON="$(jenkins_api "/computer/${NODE}/api/json?tree=offline,assignedLabels[name]")" \
    || die "the controller at $JENKINS_URL has no node ${NODE}; run: $MAC_K3D_BIN config -c $WORKER_CONFIG"
  printf '%s' "$NODE_JSON" | python3 -c '
import json, sys
doc = json.load(sys.stdin)
labels = {l.get("name") for l in doc.get("assignedLabels", [])}
if doc.get("offline"):
    sys.exit("node is offline")
if "fits-deepswe" not in labels:
    sys.exit("node has no fits-deepswe label (its cpu_cores is below what a DeepSWE question declares): " + " ".join(sorted(l for l in labels if l)))
' || die "Jenkins node ${NODE} cannot run DeepSWE (see above)"
  pass "Jenkins node ${NODE} online with fits-deepswe"
fi

pass "03_check_worker complete"
