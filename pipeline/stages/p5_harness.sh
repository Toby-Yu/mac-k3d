#!/usr/bin/env bash
# P5 — Harbor + iCode for DeepSWE, LoLBench, and SWE-bench Pro.
set -euo pipefail
source "$(cd "$(dirname "$0")" && pwd)/_common.sh"
# shellcheck source=../lib/icode_input.sh
source "$PIPELINE_LIB/icode_input.sh"

write_harness_meta() {
  python3 - "$HARNESS_DIR" "$DEEPSEEK_MODEL" "$STARTED_AT" "$FINISHED_AT" "$DURATION" "$1" "$PIPELINE_LIB" <<'PY'
import json, re, sys
from pathlib import Path

harness, model, started, finished, duration, rc, lib = sys.argv[1:8]
sys.path.insert(0, lib)
from icode_usage import find_icode_usage

root = Path(harness)
scope = root
jobs_file = root / "harbor_jobs_dir.txt"
if jobs_file.is_file():
    raw = jobs_file.read_text(encoding="utf-8").strip()
    candidate = Path(raw)
    if candidate.is_dir():
        scope = candidate
usage = find_icode_usage(scope)
blob = ""
if usage is None:
    for p in scope.rglob("*"):
        if p.is_file() and p.suffix in {".log", ".json", ".jsonl", ".txt"} and p.stat().st_size < 2_000_000:
            try:
                blob += p.read_text(encoding="utf-8", errors="ignore") + "\n"
            except OSError:
                pass

def first_int(*pats):
    for pat in pats:
        m = re.search(pat, blob, re.I)
        if m:
            return int(m.group(1))
    return 0

if usage:
    prompt = int(usage.get("prompt") or 0)
    completion = int(usage.get("completion") or 0)
    total = int(usage.get("total") or 0)
else:
    prompt = first_int(
        r'"prompt_tokens"\s*:\s*(\d+)',
        r"\bprompt_tokens[=:\s]+(\d+)",
        r'"input_tokens"\s*:\s*(\d+)',
    )
    completion = first_int(
        r'"completion_tokens"\s*:\s*(\d+)',
        r"\bcompletion_tokens[=:\s]+(\d+)",
        r'"output_tokens"\s*:\s*(\d+)',
    )
    total = first_int(r'"total_tokens"\s*:\s*(\d+)', r"\btotal_tokens[=:\s]+(\d+)")
if total == 0:
    total = prompt + completion
served = None
m = re.search(r'"model"\s*:\s*"([^"]+)"', blob)
if m:
    served = m.group(1)
if usage and usage.get("model"):
    served = usage["model"]
meta = {
    "access_date_utc": started,
    "started_at": started,
    "finished_at": finished,
    "duration_seconds": int(duration),
    "llm_model_id": model,
    "llm_model_served": served,
    "exit_code": int(rc),
    "token_usage": {"prompt": prompt, "completion": completion, "total": total},
}
(root / "meta.json").write_text(json.dumps(meta, indent=2), encoding="utf-8")
PY
}

progress 55 "P5: Harbor+iCode harness arm (benchmark=${BENCHMARK:-deepswe} n=$N_TASKS)"
CANARY_MODE="$(canary_mode)" || die "CANARY must be official, on, only or off (got ${CANARY:-})"
if icode_official_enabled; then
  [ "$CANARY_MODE" != off ] || die "OFFICIAL=1 runs the isolation canary; remove CANARY=off"
  [ -z "${CANARY_ALLOW_HOST:-}" ] || die "CANARY_ALLOW_HOST breaks isolation on purpose; OFFICIAL=1 refuses it"
fi
CANARY_DIR="$WORKDIR/canary/jenkins-${BUILD_NUMBER:-local}"
TASKS_DIR="$(benchmark_tasks_dir)"
[ -d "$TASKS_DIR" ] || die "run P2 first (missing $TASKS_DIR)"
[ -n "${DEEPSEEK_API_KEY:-}" ] || die "$(missing_deepseek_key_hint)"
have harbor || die "harbor not on PATH (run P1)"
warn_docker_mtu
[ -f "$PIPELINE_LIB/icode_harbor_agent.py" ] || die "missing pipeline/lib/icode_harbor_agent.py"
[ -f "$PIPELINE_LIB/icode_capture.sh" ] || die "missing pipeline/lib/icode_capture.sh"
[ -f "$WORKDIR/icode_bin_path.txt" ] || bash "$(dirname "$0")/p3_icode.sh"
ICODE_BIN="$(cat "$WORKDIR/icode_bin_path.txt")"
[ -n "$ICODE_BIN" ] && [ -f "$ICODE_BIN" ] || die "P3 did not resolve an icode binary (needed for Harbor bind-mount)"
if [ -s "$WORKDIR/icode_host_root.txt" ]; then
  HOST_ICODE="$(cat "$WORKDIR/icode_host_root.txt")"
else
  HOST_ICODE="$(cd "$(dirname "$ICODE_BIN")" && pwd)"
fi
[ -d "$HOST_ICODE" ] || die "icode host tree missing: $HOST_ICODE"
# Git clone has .venv/bin/icode. A release drop must keep its real binary —
# writing the git wrapper here overwrites it and Harbor then exits 127.
if [ -x "$HOST_ICODE/.venv/bin/icode" ]; then
  echo "P5 harbor: git wrapper for $HOST_ICODE"
  icode_embed_sandbox_cpython "$HOST_ICODE"
  icode_write_sandbox_pth "$HOST_ICODE"
  icode_sanitize_host_tree "$HOST_ICODE"
  icode_write_wrapper "$HOST_ICODE/icode"
  icode_probe_sandbox "$HOST_ICODE"
else
  echo "P5 harbor: keeping release binary at $HOST_ICODE/$(basename "${ICODE_BIN:-icode}") (no git wrapper)"
fi
# Clone tokens are for P3 on the host. Harbor hands its own environment to docker compose.
unset GITCODE_TOKEN MAC_K3D_GITCODE_PAT GITHUB_TOKEN MAC_K3D_GITHUB_PAT MAC_K3D_GIT_TOKEN

export DEEPSEEK_MODEL="${DEEPSEEK_MODEL:-deepseek-v4-pro}"
export ICODE_MODEL="${ICODE_MODEL:-$DEEPSEEK_MODEL}"
export ICODE_API_BASE="${ICODE_API_BASE:-https://api.deepseek.com/v1}"
if [ -z "${ICODE_PROVIDER:-}" ]; then
  case "$ICODE_API_BASE" in
    *deepseek.com*) export ICODE_PROVIDER=DeepSeek ;;
    *) export ICODE_PROVIDER=OpenAI ;;
  esac
fi
export ICODE_REASONING_EFFORT="${ICODE_REASONING_EFFORT:-high}"
mkdir -p "$HARNESS_DIR"
RESUME_RAW="$(printf '%s' "${RESUME:-0}" | tr '[:upper:]' '[:lower:]')"
case "$RESUME_RAW" in
  1|true|yes|on) RESUME=1 ;;
  *) RESUME=0 ;;
esac
if [ "$RESUME" = "1" ]; then
  echo "P5 harbor: RESUME=1 keeping prior harbor_runs; seeding completed units"
  rm -f "$HARNESS_DIR/container_mem_peak_gb" \
    "$HARNESS_DIR/container_mem_current.json" \
    "$HARNESS_DIR/active_question.txt" \
    "$HARNESS_DIR/skipped_questions.txt"
else
  rm -rf "$HARNESS_DIR/harbor_runs/jenkins-${BUILD_NUMBER:-local}"
  rm -f "$HARNESS_DIR/container_mem_peak_gb" \
    "$HARNESS_DIR/container_mem.jsonl" \
    "$HARNESS_DIR/container_mem_current.json" \
    "$HARNESS_DIR/active_question.txt" \
    "$HARNESS_DIR/skipped_questions.txt"
  echo "P5 harbor: cleared harbor_runs/jenkins-${BUILD_NUMBER:-local} for this run"
fi
ensure_selected_tasks
JOBS_DIR="$HARNESS_DIR/harbor_runs/jenkins-${BUILD_NUMBER:-local}"
case "${BENCHMARK:-deepswe}" in
  lolbench)
    PROV_REPO="$LOLBENCH_DIR"
    PROV_APPLIED=1
    ;;
  swebenchpro)
    PROV_REPO="$SWEBENCHPRO_DIR/src"
    PROV_APPLIED=0
    ;;
  *)
    PROV_REPO="$DEEPSWE_DIR"
    PROV_APPLIED=0
    ;;
esac

MOUNTS_JSON="$(python3 "$PIPELINE_LIB/agent_mounts.py" build --icode-root "$HOST_ICODE")"
python3 "$PIPELINE_LIB/agent_mounts.py" check \
  --mounts "$MOUNTS_JSON" \
  --icode-root "$HOST_ICODE" \
  --forbid "$DEEPSWE_DIR" \
  --forbid "$LOLBENCH_DIR" \
  --forbid "$SWEBENCHPRO_DIR" \
  --forbid "$TASKS_DIR" \
  || die "P5 refuses to mount $HOST_ICODE for the agent (see errors above)"

PROTOCOL_INPUTS="$WORKDIR/eval_protocol_inputs.json"
python3 "$PIPELINE_LIB/provenance.py" write-inputs \
  --out "$PROTOCOL_INPUTS" \
  --repo-dir "$PROV_REPO" \
  --tasks-dir "$TASKS_DIR" \
  --selected "$WORKDIR/selected_tasks.txt" \
  --overlay "$PIPELINE_LIB/lolbench_fix_rewards.py" \
  --applied "$PROV_APPLIED" \
  --workdir "$WORKDIR" \
  --pipeline-root "$MAC_K3D_ROOT" \
  --icode-root "$HOST_ICODE" \
  --mounts "$MOUNTS_JSON"
mapfile -t TASK_IDS < <(grep -v '^[[:space:]]*$' "$WORKDIR/selected_tasks.txt" || true)
[ "${#TASK_IDS[@]}" -gt 0 ] || die "no task under $TASKS_DIR (wanted ${TASK_ID:-<empty>})"

# Harbor applies each task.toml's cpus/memory_mb. This only decides how many
# trials it may run at once, and refuses a worker that cannot host one.
RESOURCE_PLAN="$WORKDIR/eval_resources.json"
plan_out="$(
  python3 "$PIPELINE_LIB/task_resources.py" plan \
    --tasks-dir "$TASKS_DIR" \
    --selected "$WORKDIR/selected_tasks.txt" \
    --cpu "${CPU_LOCK_QTY:-1}" \
    --n-rollouts "$N_ROLLOUTS" \
    --workdir "$WORKDIR" \
    --measured-peak-gb "$(cat "$HARNESS_DIR/container_mem_peak_gb" 2>/dev/null || echo 0)" \
    --out "$RESOURCE_PLAN"
)" || die "this worker cannot run the selected tasks as declared (see the error above)"
eval "$plan_out"
export EVAL_SLOTS DECLARED_CPUS DECLARED_MEMORY_MB DECLARED_STORAGE_MB
python3 "$PIPELINE_LIB/provenance.py" record-resources \
  --inputs "$PROTOCOL_INPUTS" --plan "$RESOURCE_PLAN" || true
STARTED_AT="$(date -u +%Y-%m-%dT%H:%M:%SZ)"
SECONDS=0
: >"$HARNESS_DIR/harbor.log"
LAST_RC=0

HARBOR_ENV="$WORKDIR/.harbor-env"
umask 077
{
  printf 'DEEPSEEK_API_KEY=%s\n' "${DEEPSEEK_API_KEY}"
  printf 'DEEPSEEK_MODEL=%s\n' "${DEEPSEEK_MODEL}"
  printf 'ICODE_MODEL=%s\n' "${ICODE_MODEL}"
  printf 'ICODE_API_BASE=%s\n' "${ICODE_API_BASE}"
  printf 'ICODE_PROVIDER=%s\n' "${ICODE_PROVIDER}"
  printf 'ICODE_REASONING_EFFORT=%s\n' "${ICODE_REASONING_EFFORT}"
  printf 'PYTHONDONTWRITEBYTECODE=%s\n' "1"
  printf 'MAC_K3D_BENCHMARK=%s\n' "${BENCHMARK:-deepswe}"
} >"$HARBOR_ENV"
chmod 600 "$HARBOR_ENV"

LEAKSCAN_OUT="$WORKDIR/anticheat_leakscan.json"
set +e
python3 "$PIPELINE_LIB/anticheat_leakscan.py" \
  --tree "$HOST_ICODE" \
  --tasks-dir "$TASKS_DIR" \
  --selected "$WORKDIR/selected_tasks.txt" \
  --out "$LEAKSCAN_OUT"
leak_rc=$?
set -e
if [ "$leak_rc" = 0 ] || [ "$leak_rc" = 2 ]; then
  python3 "$PIPELINE_LIB/provenance.py" record-leakscan --inputs "$PROTOCOL_INPUTS" --report "$LEAKSCAN_OUT"
fi
case "$leak_rc" in
  0) ;;
  2)
    if icode_official_enabled; then
      die "leak scan found task gold in the mounted iCode tree (see $LEAKSCAN_OUT); OFFICIAL=1 stops here"
    fi
    echo "WARNING: leak scan found task gold in the mounted iCode tree (see $LEAKSCAN_OUT). Smoke run continues; OFFICIAL=1 would stop."
    ;;
  *) die "leak scanner failed (exit $leak_rc)" ;;
esac

declare -A TASK_READY=()
NEEDED=$(( ${#TASK_IDS[@]} * N_ROLLOUTS ))

# Harbor owns the trial loop, so progress is "how many rewards exist so far".
write_progress() {
  local done_units
  done_units="$(find "$JOBS_DIR" -name reward.json -type f 2>/dev/null | wc -l | tr -d ' ')"
  python3 "$PIPELINE_LIB/eval_progress.py" write \
    --out "$HARNESS_DIR/progress.json" \
    --done "${done_units:-0}" \
    --needed "$NEEDED" \
    --inflight "$EVAL_SLOTS" \
    --slots "${EVAL_SLOTS:-1}" \
    --mean "" \
    --started-at "$STARTED_AT"
}

ensure_task_ready() {
  local tid="$1"
  [ -n "${TASK_READY[$tid]:-}" ] && return 0
  [ -d "$TASKS_DIR/$tid" ] || die "no task under $TASKS_DIR (wanted $tid)"
  if [ "${BENCHMARK:-deepswe}" = "lolbench" ]; then
    local task_toml="$TASKS_DIR/${tid}/task.toml"
    local image
    image="$(
      python3 - "$task_toml" "$tid" <<'PY'
import re, sys
path, tid = sys.argv[1], sys.argv[2]
text = open(path, encoding="utf-8").read() if path else ""
m = re.search(r'^docker_image\s*=\s*"([^"]+)"', text, re.M)
print(m.group(1) if m else f"smartdub26/lolbench:{tid}-1.0.0")
PY
    )"
    ensure_task_image "$tid" "$image" "$TASKS_DIR/${tid}/environment"
  fi
  TASK_READY[$tid]=1
}

# Sets cmd, unit_run_dir, unit_task_path, unit_jobs and unit_log.
declare -a cmd=()
unit_jobs=""
unit_run_dir=""
unit_task_path=""
unit_log=""

# LoLBench runs Harbor from its checkout with a relative dataset path.
# Where to run harbor from, and the dataset directory to hand it. LoLBench's
# tasks only resolve relative to its checkout.
unit_location() {
  unit_run_dir="$WORKDIR"
  unit_task_path="$TASKS_DIR"
  if [ "${BENCHMARK:-deepswe}" = "lolbench" ]; then
    unit_run_dir="$LOLBENCH_DIR"
    unit_task_path="harbor_tasks"
  fi
}

# One Harbor job for the whole build: Harbor expands the selected tasks into
# n_attempts trials each and runs EVAL_SLOTS of them at a time.
build_run_cmd() {
  local tid
  unit_location
  unit_log="$JOBS_DIR/harbor.log"
  cmd=(harbor run)
  cmd+=(-p "$unit_task_path")
  for tid in "${TASK_IDS[@]}"; do
    cmd+=(-i "$tid")
  done
  cmd+=(-a "icode_harbor_agent:ICodeAgent")
  cmd+=(--job-name "icode_${BENCHMARK:-deepswe}_${BUILD_NUMBER:-local}")
  cmd+=(--jobs-dir "$JOBS_DIR")
  cmd+=(--no-delete)
  cmd+=(-k "$N_ROLLOUTS")
  cmd+=(-r "${HARBOR_MAX_RETRIES:-1}")
  append_agent_flags
}

# The canary (P0.6) runs in iCode's sandbox: only the agent, job location, its
# spec and the skipped verifier differ. CANARY_ALLOW_HOST opens one more host for
# the canary alone, to prove that it notices.
build_canary_cmd() {
  local tid="$1" spec="$2"
  # Same -p/-i resolution as the real run, so the canary proves isolation on the
  # path the rollouts actually take.
  unit_location
  unit_jobs="$CANARY_DIR/$tid"
  unit_log="$unit_jobs/harbor.log"
  cmd=(harbor run)
  cmd+=(-p "$unit_task_path")
  cmd+=(-i "$tid")
  cmd+=(-a "canary_harbor_agent:CanaryAgent")
  cmd+=(--job-name "${tid}_canary_${BUILD_NUMBER:-local}")
  cmd+=(--jobs-dir "$unit_jobs")
  cmd+=(--no-delete)
  append_agent_flags
  cmd+=(--ak "spec=$spec")
  cmd+=(--disable-verification)
  if [ -n "${CANARY_ALLOW_HOST:-}" ]; then
    cmd+=(--allow-agent-host "$CANARY_ALLOW_HOST")
  fi
}

# Everything the agent sees: model, egress allowlist, resources, env file,
# verifier env, mounts and agent env. Shared by iCode and the canary.
append_agent_flags() {
  cmd+=(-m "${DEEPSEEK_MODEL}")
  cmd+=(--allow-agent-host api.deepseek.com)
  cmd+=(--allow-agent-host api.deepseek.ai)
  cmd+=(--agent-setup-timeout-multiplier 10)
  cmd+=(-n "$EVAL_SLOTS")
  # No --override-cpus / --override-memory-mb: Harbor then applies what each
  # task.toml declares. EVAL_SLOTS already divides the lock by those numbers.
  if [ -n "${EVAL_OVERRIDE_CPUS:-}" ]; then
    cmd+=(--override-cpus "$EVAL_OVERRIDE_CPUS")
    echo "P5 harbor: override-cpus=$EVAL_OVERRIDE_CPUS (EVAL_OVERRIDE_CPUS set; task.toml ignored)"
  fi
  if [ -n "${EVAL_OVERRIDE_MEMORY_MB:-}" ]; then
    cmd+=(--override-memory-mb "$EVAL_OVERRIDE_MEMORY_MB")
    echo "P5 harbor: override-memory-mb=$EVAL_OVERRIDE_MEMORY_MB (EVAL_OVERRIDE_MEMORY_MB set; task.toml ignored)"
  fi
  cmd+=(-y)
  cmd+=(--env-file "$HARBOR_ENV")
  if [ "${BENCHMARK:-deepswe}" = "lolbench" ]; then
    cmd+=(--ve "LOLBENCH_SUITE=union")
  fi
  cmd+=(--mounts "$MOUNTS_JSON")
  cmd+=(--ae "ICODE_MODEL=${ICODE_MODEL}")
  cmd+=(--ae "ICODE_API_BASE=${ICODE_API_BASE}")
  cmd+=(--ae "ICODE_PROVIDER=${ICODE_PROVIDER}")
  cmd+=(--ae "ICODE_REASONING_EFFORT=${ICODE_REASONING_EFFORT}")
  cmd+=(--ae "PYTHONDONTWRITEBYTECODE=1")
  cmd+=(--ae "DEEPSEEK_MODEL=${DEEPSEEK_MODEL}")
  cmd+=(--ae "DEEPSEEK_API_KEY=${DEEPSEEK_API_KEY}")
  cmd+=(--ae "MAC_K3D_BENCHMARK=${BENCHMARK:-deepswe}")
  if [ -n "${REPO_CANDIDATES:-}" ]; then
    cmd+=(--ae "MAC_K3D_REPO_CANDIDATES=$REPO_CANDIDATES")
  fi
}

# Every selected task's declared repo, colon separated and deduplicated. One
# Harbor job covers them all, so the trial picks the one its own image has.
# The declared base commit is checked host-side in P7 instead, where the task id
# of each trial is known.
declared_repo_candidates() {
  local tid declared repo_decl base_decl out=""
  for tid in "${TASK_IDS[@]}"; do
    declared="$(python3 "$PIPELINE_LIB/capture_receipt.py" declared-repo \
      --task-dir "$TASKS_DIR/$tid" --benchmark "${BENCHMARK:-deepswe}")" || declared=""
    { read -r repo_decl; read -r base_decl; } <<<"$declared" || true
    [ -n "$repo_decl" ] || continue
    case ":${out}:" in
      *":${repo_decl}:"*) continue ;;
    esac
    if [ -n "$out" ]; then out="${out}:${repo_decl}"; else out="$repo_decl"; fi
  done
  printf '%s' "$out"
}

# One Harbor job for every selected task x rollout. Harbor schedules them.
run_harbor() {
  local tid rc rewards
  for tid in "${TASK_IDS[@]}"; do
    ensure_task_ready "$tid"
  done
  REPO_CANDIDATES="$(declared_repo_candidates)"
  mkdir -p "$JOBS_DIR"
  printf '%s\n' "$JOBS_DIR" >"$HARNESS_DIR/harbor_jobs_dir.txt"
  build_run_cmd
  echo "P5 harbor: -p ${unit_task_path} tasks=${#TASK_IDS[@]} -k ${N_ROLLOUTS} -n ${EVAL_SLOTS} (${NEEDED} trials)"
  echo "P5 harbor: --jobs-dir $JOBS_DIR log=$unit_log"
  [ -z "$REPO_CANDIDATES" ] || echo "P5 harbor: declared repos $REPO_CANDIDATES"
  write_progress
  (
    while true; do
      sleep 60
      python3 "$PIPELINE_LIB/eval_progress.py" heartbeat \
        --progress "$HARNESS_DIR/progress.json" --elapsed "$SECONDS" || true
      python3 "$PIPELINE_LIB/task_resources.py" sample \
        --harness-dir "$HARNESS_DIR" --slots "$EVAL_SLOTS" || true
      write_progress || true
    done
  ) &
  heartbeat_pid=$!
  set +e
  (
    cd "$unit_run_dir"
    export PYTHONPATH="$PIPELINE_LIB${PYTHONPATH:+:$PYTHONPATH}"
    export PYTHONUNBUFFERED=1
    if command -v stdbuf >/dev/null 2>&1; then
      stdbuf -oL -eL "${cmd[@]}"
    else
      "${cmd[@]}"
    fi
  ) 2>&1 | tee -a "$unit_log"
  rc="${PIPESTATUS[0]}"
  set -e
  kill "$heartbeat_pid" 2>/dev/null || true
  wait "$heartbeat_pid" 2>/dev/null || true
  cat "$unit_log" >>"$HARNESS_DIR/harbor.log" || true
  if grep -Eiq 'No such option|unexpected argument|unrecognized arguments' "$unit_log"; then
    die "harbor CLI rejected flags (see $unit_log). Not recording as a successful stage."
  fi
  echo "$rc" >"$HARNESS_DIR/exit_code.txt"
  LAST_RC="$rc"
  rewards="$(find "$JOBS_DIR" -name reward.json -type f 2>/dev/null | wc -l | tr -d ' ')"
  echo "P5 harbor: ${rewards:-0}/${NEEDED} trials scored (exit $rc)"
  if [ "${rewards:-0}" -eq 0 ]; then
    echo "WARNING: no reward.json under $JOBS_DIR (unscored / no-response; see $unit_log)"
  elif [ "$rc" -ne 0 ]; then
    echo "WARNING: harbor run exited $rc. Rewards are present; treating them as scores."
  fi
  if grep -Eiq \
    'out of memory|Cannot allocate memory|oom-kill|oom_kill|exit(ed)?[[:space:]]+(with[[:space:]]+)?(status|code)[[:space:]]*137([^0-9]|$)|ExitCode[=:[:space:]]*137([^0-9]|$)' \
    "$unit_log"; then
    echo "WARNING: a trial hit the memory ceiling. EVAL_SLOTS=$EVAL_SLOTS came from CPU_LOCK_QTY and the declared memory; lower CPU_LOCK_QTY or give the worker more RAM."
  fi
  for tid in "${TASK_IDS[@]}"; do
    python3 "$PIPELINE_LIB/provenance.py" record-image \
      --inputs "$WORKDIR/eval_protocol_inputs.json" \
      --task-id "$tid" \
      --task-toml "$TASKS_DIR/${tid}/task.toml" || true
  done
  write_progress
}

masked_cmd() {
  local arg
  local -a masked=()
  for arg in "${cmd[@]}"; do
    case "$arg" in
      *_API_KEY=* | *_TOKEN=* | *_PAT=*) arg="${arg%%=*}=***" ;;
    esac
    masked+=("$arg")
  done
  printf '%s' "${masked[*]}"
}

# Host lists and gold file names for one task; never gold content.
canary_spec() {
  local tid="$1" spec="$CANARY_DIR/$1/canary_spec.json"
  mkdir -p "$CANARY_DIR/$tid"
  python3 "$PIPELINE_LIB/canary_verdict.py" spec \
    --task-dir "$TASKS_DIR/$tid" \
    --benchmark "${BENCHMARK:-deepswe}" \
    --allow-host "${CANARY_ALLOW_HOST:-}" \
    --out "$spec" >&2 || die "canary spec failed for $tid"
  printf '%s\n' "$spec"
}

# One canary trial per target task, one after another. A failure stops the run.
run_canary() {
  local tid spec rc
  local -a targets=() expect=()
  if [ "$CANARY_MODE" = only ]; then
    targets=("${TASK_IDS[@]}")
  else
    targets=("${TASK_IDS[0]}")
  fi
  rm -rf "$CANARY_DIR"
  mkdir -p "$CANARY_DIR"
  if [ -n "${CANARY_ALLOW_HOST:-}" ]; then
    echo "WARNING: CANARY_ALLOW_HOST=$CANARY_ALLOW_HOST opens that host for the canary only; the canary must fail"
  fi
  for tid in "${targets[@]}"; do
    ensure_task_ready "$tid"
    spec="$(canary_spec "$tid")"
    build_canary_cmd "$tid" "$spec"
    echo "P5 canary: task=$tid -a canary_harbor_agent:CanaryAgent (iCode's flags, mounts and env) log=$unit_log"
    set +e
    (
      cd "$unit_run_dir"
      export PYTHONPATH="$PIPELINE_LIB${PYTHONPATH:+:$PYTHONPATH}"
      export PYTHONUNBUFFERED=1
      "${cmd[@]}"
    ) >"$unit_log" 2>&1
    rc=$?
    set -e
    if [ "$rc" -ne 0 ]; then
      echo "WARNING: canary harbor run exited $rc for $tid (see $unit_log)"
    fi
    expect+=(--task "$tid")
  done
  set +e
  python3 "$PIPELINE_LIB/canary_verdict.py" summarize \
    --jobs-dir "$CANARY_DIR" \
    --out-dir "$CANARY_DIR" \
    "${expect[@]}"
  rc=$?
  set -e
  case "$rc" in
    0 | 2) ;;
    *) die "canary verdict failed (exit $rc)" ;;
  esac
  python3 "$PIPELINE_LIB/provenance.py" record-canary --inputs "$PROTOCOL_INPUTS" --summary "$CANARY_DIR/summary.json"
  [ "$rc" = 0 ] || die "isolation canary failed (see $CANARY_DIR/report.md); the run stops here"
  echo "P5 canary: pass (${#targets[@]} tasks, report $CANARY_DIR/report.md)"
}

# MAC_K3D_P5_DRY_RUN=1: print the harbor command with secret values masked; run nothing.
if [ "${MAC_K3D_P5_DRY_RUN:-0}" = 1 ]; then
  REPO_CANDIDATES="$(declared_repo_candidates)"
  build_run_cmd
  echo "P5 harbor dry-run (cwd $unit_run_dir): $(masked_cmd)"
  if [ "$CANARY_MODE" != off ]; then
    build_canary_cmd "${TASK_IDS[0]}" "$(canary_spec "${TASK_IDS[0]}")"
    echo "P5 canary dry-run (cwd $unit_run_dir): $(masked_cmd)"
  fi
  exit 0
fi

if [ "$CANARY_MODE" != off ]; then
  run_canary
fi
if [ "$CANARY_MODE" = only ]; then
  progress 70 "P5 canary only: pass; no rollouts (report $CANARY_DIR/report.md)"
  exit 0
fi

run_harbor
icode_reclaim_host_tree "$HOST_ICODE"

DURATION="$SECONDS"
FINISHED_AT="$(date -u +%Y-%m-%dT%H:%M:%SZ)"
write_harness_meta "$LAST_RC"
progress 70 "P5 complete (exit=$LAST_RC)"
