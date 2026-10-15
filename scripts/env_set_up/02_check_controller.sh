#!/usr/bin/env bash
# Verify Jenkins controller (k3d + UI + jobs) is healthy. With JENKINS_USER and
# JENKINS_API_TOKEN, also the DeepSWE harness choices, the Active Choices
# plugin and the credentials an eval reads.
set -euo pipefail
# shellcheck source=./_common.sh
source "$(cd "$(dirname "$0")" && pwd)/_common.sh"

require_mac_k3d

if [ -f "$CONTROLLER_CONFIG" ]; then
  STATUS_OUT="$("$MAC_K3D_BIN" status -c "$CONTROLLER_CONFIG" 2>&1)" || true
  echo "$STATUS_OUT"

  echo "$STATUS_OUT" | grep -qi 'Docker Engine:.*running' || die "Docker Engine not running per status"
  echo "$STATUS_OUT" | grep -qi 'k3d cluster:.*running' || die "k3d cluster not running"
  echo "$STATUS_OUT" | grep -qi 'Jenkins:.*Running' || die "Jenkins pod not Running"
  pass "controller status healthy"
  JOBS_FIX="run: $MAC_K3D_BIN config -c $CONTROLLER_CONFIG"
elif [ -f "$WORKER_CONFIG" ]; then
  note "no controller on this machine; checking the one at $JENKINS_URL (jenkins_agent.controller_url)"
  JOBS_FIX="run \`mac-k3d config -c ~/.config/mac-k3d/config.yaml\` on the controller"
else
  die "missing controller config: $CONTROLLER_CONFIG and worker config: $WORKER_CONFIG (run: mac-k3d setup)"
fi

CODE="$(jenkins_login_code)"
[ "$CODE" = "200" ] || die "Jenkins UI ${JENKINS_URL}/login returned HTTP $CODE (want 200)"
pass "Jenkins UI ${JENKINS_URL}/login HTTP 200"

HIDDEN=0
for bench in deepswe lolbench swebenchpro; do
  for shape in one_task some_task full_suite_task; do
    rc=0
    job_state "${bench}_${shape}" || rc=$?
    case "$rc" in
      0) pass "job ${bench}_${shape} present" ;;
      2) HIDDEN=1 ;;
      *) die "job ${bench}_${shape} not found at ${JENKINS_URL}; $JOBS_FIX" ;;
    esac
  done
done
for retired in build_jiuwenswarm_headless eval_aggregate; do
  rc=0
  job_state "$retired" || rc=$?
  case "$rc" in
    0) die "retired job $retired is still on the controller; $JOBS_FIX removes it" ;;
    2) HIDDEN=1 ;;
  esac
done
if [ "$HIDDEN" = 1 ]; then
  note "${JENKINS_URL} answers 403 for every job without a login; set JENKINS_USER and JENKINS_API_TOKEN to check the jobs"
else
  pass "no retired jobs (build_jiuwenswarm_headless, eval_aggregate)"
fi

if ! have_jenkins_token; then
  note "JENKINS_USER / JENKINS_API_TOKEN unset — skipping the harness choices, plugin and credential checks"
  pass "02_check_controller complete"
  exit 0
fi

# Every DeepSWE job offers both harnesses; the page shows the picked one's inputs.
for shape in one_task some_task full_suite_task; do
  xml="$(jenkins_api "/job/deepswe_${shape}/config.xml")" || die "cannot read deepswe_${shape}/config.xml as $JENKINS_USER"
  choices="$(printf '%s' "$xml" | python3 -c '
import sys, xml.etree.ElementTree as ET
root = ET.fromstring(sys.stdin.read())
for param in root.iter("hudson.model.ChoiceParameterDefinition"):
    if (param.findtext("name") or "") == "HARNESS":
        print(" ".join(s.text or "" for s in param.iter("string")))
')"
  case " $choices " in
    *" icode "*" jiuwenswarm "* | *" jiuwenswarm "*" icode "*) ;;
    *) die "deepswe_${shape} offers HARNESS '${choices:-none}', not icode and jiuwenswarm; $JOBS_FIX" ;;
  esac
  pass "deepswe_${shape} HARNESS: $choices"
done

plugins="$(jenkins_api "/pluginManager/api/json?depth=1&tree=plugins[shortName,active]")" \
  || die "cannot list the controller's plugins as $JENKINS_USER"
printf '%s' "$plugins" | python3 -c '
import json, sys
doc = json.load(sys.stdin)
sys.exit(0 if any(p.get("shortName") == "uno-choice" and p.get("active") for p in doc.get("plugins", [])) else 1)
' || die "the Active Choices plugin (uno-choice) is not active: the DeepSWE pages cannot switch between iCode and JiuwenSwarm inputs; $JOBS_FIX"
pass "Active Choices plugin (uno-choice) active"

credential() {
  jenkins_api "/credentials/store/system/domain/_/credential/$1/api/json?tree=id" >/dev/null 2>&1
}
credential deepseek-api-key || die "no Jenkins credential deepseek-api-key: every eval reads its model key from it (Manage Jenkins > Credentials > System > Global, Secret text)"
pass "credential deepseek-api-key present"
if credential gitcode-pat; then
  pass "credential gitcode-pat present"
else
  note "no credential gitcode-pat: a private gitcode.com repo (iCode or JiuwenSwarm git) cannot be cloned"
fi

pass "02_check_controller complete"
