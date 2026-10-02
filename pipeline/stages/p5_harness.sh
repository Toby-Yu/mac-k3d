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
eval_parallel_degree || die "N_ROLLOUTS and CPU_LOCK_QTY must be integers >= 1"
echo "P5 harbor: n_rollouts=$N_ROLLOUTS slots=$EVAL_SLOTS cpus_each=$EVAL_CPUS_EACH"
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
python3 "$PIPELINE_LIB/eval_slots.py" report \
  --tasks-file "$WORKDIR/selected_tasks.txt" \
  --n-rollouts "$N_ROLLOUTS" \
  --cpu "$CPU_LOCK_QTY" \
  --benchmark "${BENCHMARK:-deepswe}" \
  --workdir "$WORKDIR"
mapfile -t TASK_IDS < <(grep -v '^[[:space:]]*$' "$WORKDIR/selected_tasks.txt" || true)
[ "${#TASK_IDS[@]}" -gt 0 ] || die "no task under $TASKS_DIR (wanted ${TASK_ID:-<empty>})"
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
ASSIGNED=""
INFLIGHT=""
declare -A UNIT_SPEC=()
declare -A UNIT_JOBS=()
declare -A UNIT_LOG=()
declare -A UNIT_START=()
declare -A QUESTION_START=()
NEEDED=$(( ${#TASK_IDS[@]} * N_ROLLOUTS ))
STARTED_UNITS=0
DONE_UNITS=0
DUR_SUM=0
DUR_N=0

if [ "$RESUME" = "1" ]; then
  seed="$(
    python3 "$PIPELINE_LIB/eval_slots.py" resume-seed \
      --tasks-file "$WORKDIR/selected_tasks.txt" \
      --n-rollouts "$N_ROLLOUTS" \
      --workdir "$WORKDIR" \
      --build "${RESUME_FROM:-}"
  )" || die "eval_slots.py resume-seed failed"
  eval "$seed"
  ASSIGNED="${ASSIGNED:-}"
  RESUME_DONE="${RESUME_DONE:-0}"
  RESUME_REMAINING="${RESUME_REMAINING:-}"
  RESUME_BUILD="${RESUME_BUILD:-}"
  STARTED_UNITS="$RESUME_DONE"
  DONE_UNITS="$RESUME_DONE"
  echo "P5 harbor: resume from ${RESUME_BUILD:-auto} seeded done=$RESUME_DONE/$NEEDED remaining=${RESUME_REMAINING:--}"
fi

write_progress() {
  local inflight_n="${#UNIT_SPEC[@]}"
  local mean=""
  if [ "${DUR_N:-0}" -gt 0 ]; then
    mean="$(python3 -c "print($DUR_SUM / $DUR_N)")"
  fi
  python3 "$PIPELINE_LIB/eval_progress.py" write \
    --out "$HARNESS_DIR/progress.json" \
    --done "$DONE_UNITS" \
    --needed "$NEEDED" \
    --inflight "$inflight_n" \
    --slots "${EVAL_SLOTS:-1}" \
    --mean "$mean" \
    --started-at "$STARTED_AT"
}

ensure_task_ready() {
  local tid="$1"
  [ -n "${TASK_READY[$tid]:-}" ] && return 0
  [ -d "$TASKS_DIR/$tid" ] || die "no task under $TASKS_DIR (wanted $tid)"
  local jobs="$HARNESS_DIR/harbor_runs/jenkins-${BUILD_NUMBER:-local}/${tid}"
  mkdir -p "$jobs"
  printf '%s\n' "$jobs" >"$HARNESS_DIR/harbor_jobs_dir.txt"
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

live_slots() {
  local out question=""
  if [ -n "${INFLIGHT:-}" ]; then
    question="${INFLIGHT%%:*}"
  else
    # Next wave's question (history peak / fallback planning before launch).
    question="$(next_work_unit 2>/dev/null | awk '{print $1}' || true)"
  fi
  out="$(
    python3 "$PIPELINE_LIB/eval_slots.py" slots \
      --cpu "$CPU_LOCK_QTY" \
      --benchmark "${BENCHMARK:-deepswe}" \
      --workdir "$WORKDIR" \
      --question "$question"
  )" || return 1
  eval "$out"
  EVAL_PARALLEL="$EVAL_SLOTS"
  export EVAL_SLOTS EVAL_PARALLEL EVAL_CPUS_EACH EVAL_FITS EVAL_MEMORY_MB
}

next_work_unit() {
  python3 "$PIPELINE_LIB/eval_slots.py" next \
    --tasks-file "$WORKDIR/selected_tasks.txt" \
    --n-rollouts "$N_ROLLOUTS" \
    --assigned "$ASSIGNED" \
    --inflight "$INFLIGHT"
}

# Sets cmd, unit_jobs, unit_run_dir, unit_task_path and unit_log for one work unit.
declare -a cmd=()
unit_jobs=""
unit_run_dir=""
unit_task_path=""
unit_log=""

# LoLBench runs Harbor from its checkout with a relative task path.
unit_location() {
  unit_run_dir="$WORKDIR"
  unit_task_path="$TASKS_DIR/$1"
  if [ "${BENCHMARK:-deepswe}" = "lolbench" ]; then
    unit_run_dir="$LOLBENCH_DIR"
    unit_task_path="harbor_tasks/$1"
  fi
}

build_unit_cmd() {
  local tid="$1" attempt="$2" job_name att_tag
  unit_jobs="$HARNESS_DIR/harbor_runs/jenkins-${BUILD_NUMBER:-local}/${tid}"
  att_tag="$(printf '%02d' "$attempt")"
  job_name="${tid}_icode_${BUILD_NUMBER:-local}_a${att_tag}"
  if [ "${BENCHMARK:-deepswe}" = "lolbench" ]; then
    job_name="${tid}_icode_union_${BUILD_NUMBER:-local}_a${att_tag}"
  fi
  unit_location "$tid"
  unit_log="$unit_jobs/harbor-a${att_tag}.log"
  cmd=(harbor run)
  cmd+=(-p "$unit_task_path")
  cmd+=(-a "icode_harbor_agent:ICodeAgent")
  cmd+=(--job-name "$job_name")
  cmd+=(--jobs-dir "$unit_jobs")
  cmd+=(--no-delete)
  append_agent_flags "$tid"
}

# The canary (P0.6) runs in iCode's sandbox: only the agent, job location, its
# spec and the skipped verifier differ. CANARY_ALLOW_HOST opens one more host for
# the canary alone, to prove that it notices.
build_canary_cmd() {
  local tid="$1" spec="$2"
  unit_location "$tid"
  unit_jobs="$CANARY_DIR/$tid"
  unit_log="$unit_jobs/harbor.log"
  cmd=(harbor run)
  cmd+=(-p "$unit_task_path")
  cmd+=(-a "canary_harbor_agent:CanaryAgent")
  cmd+=(--job-name "${tid}_canary_${BUILD_NUMBER:-local}")
  cmd+=(--jobs-dir "$unit_jobs")
  cmd+=(--no-delete)
  append_agent_flags "$tid"
  cmd+=(--ak "spec=$spec")
  cmd+=(--disable-verification)
  if [ -n "${CANARY_ALLOW_HOST:-}" ]; then
    cmd+=(--allow-agent-host "$CANARY_ALLOW_HOST")
  fi
}

# Everything the agent sees: model, egress allowlist, resources, env file,
# verifier env, mounts and agent env. Shared by iCode and the canary.
append_agent_flags() {
  local tid="$1"
  cmd+=(-m "${DEEPSEEK_MODEL}")
  cmd+=(--allow-agent-host api.deepseek.com)
  cmd+=(--allow-agent-host api.deepseek.ai)
  cmd+=(--agent-setup-timeout-multiplier 10)
  cmd+=(-n 1)
  cmd+=(-k 1)
  cmd+=(--override-cpus "$EVAL_CPUS_EACH")
  if [ "${EVAL_MEMORY_MB:-0}" -gt 0 ] 2>/dev/null; then
    cmd+=(--override-memory-mb "$EVAL_MEMORY_MB")
    echo "P5 harbor: override-memory-mb=$EVAL_MEMORY_MB slots=$EVAL_SLOTS cpus_each=$EVAL_CPUS_EACH"
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
  capture_declared_ae "$TASKS_DIR/$tid"
}

# Appends the repo and base the task declares; icode_capture.sh checks them.
capture_declared_ae() {
  local declared repo_decl="" base_decl=""
  declared="$(python3 "$PIPELINE_LIB/capture_receipt.py" declared-repo \
    --task-dir "$1" --benchmark "${BENCHMARK:-deepswe}")" || declared=""
  { read -r repo_decl; read -r base_decl; } <<<"$declared" || true
  if [ -n "$repo_decl" ]; then cmd+=(--ae "MAC_K3D_REPO=$repo_decl"); fi
  if [ -n "$base_decl" ]; then cmd+=(--ae "MAC_K3D_BASE_COMMIT=$base_decl"); fi
}

start_unit() {
  local tid="$1" attempt="$2"
  ensure_task_ready "$tid"
  build_unit_cmd "$tid" "$attempt"
  local jobs="$unit_jobs" run_dir="$unit_run_dir" log="$unit_log"
  if [ -z "${QUESTION_START[$tid]:-}" ]; then
    QUESTION_START[$tid]="$SECONDS"
    printf '%s\n' "$tid" >"$HARNESS_DIR/active_question.txt"
    echo "P5 harbor: -p ${unit_task_path} -a icode_harbor_agent:ICodeAgent -m ${DEEPSEEK_MODEL}"
    echo "P5 harbor: --jobs-dir $jobs (attempts 1-${N_ROLLOUTS})"
  fi
  echo "P5 harbor: task=$tid attempt=$attempt slots=$EVAL_SLOTS cpus_each=$EVAL_CPUS_EACH"
  (
    cd "$run_dir"
    export PYTHONPATH="$PIPELINE_LIB${PYTHONPATH:+:$PYTHONPATH}"
    export PYTHONUNBUFFERED=1
    if command -v stdbuf >/dev/null 2>&1; then
      stdbuf -oL -eL "${cmd[@]}"
    else
      "${cmd[@]}"
    fi
  ) >>"$log" 2>&1 &
  local pid=$!
  UNIT_SPEC[$pid]="${tid}:${attempt}"
  UNIT_JOBS[$pid]="$jobs"
  UNIT_LOG[$pid]="$log"
  if [ -n "$ASSIGNED" ]; then
    ASSIGNED="${ASSIGNED},${tid}:${attempt}"
  else
    ASSIGNED="${tid}:${attempt}"
  fi
  if [ -n "$INFLIGHT" ]; then
    INFLIGHT="${INFLIGHT},${tid}:${attempt}"
  else
    INFLIGHT="${tid}:${attempt}"
  fi
  UNIT_START[$pid]="$SECONDS"
  STARTED_UNITS=$((STARTED_UNITS + 1))
}

drop_inflight() {
  local spec="$1"
  local out="" part
  IFS=',' read -r -a parts <<<"$INFLIGHT"
  for part in "${parts[@]}"; do
    [ "$part" = "$spec" ] && continue
    [ -z "$part" ] && continue
    if [ -n "$out" ]; then
      out="${out},${part}"
    else
      out="$part"
    fi
  done
  INFLIGHT="$out"
}

reap_finished() {
  local pid spec jobs log rc reward waited=0
  for pid in "${!UNIT_SPEC[@]}"; do
    if kill -0 "$pid" 2>/dev/null; then
      continue
    fi
    waited=1
    set +e
    wait "$pid"
    rc=$?
    set -e
    spec="${UNIT_SPEC[$pid]}"
    jobs="${UNIT_JOBS[$pid]}"
    log="${UNIT_LOG[$pid]}"
    start_s="${UNIT_START[$pid]:-0}"
    unset 'UNIT_SPEC[$pid]'
    unset 'UNIT_JOBS[$pid]'
    unset 'UNIT_LOG[$pid]'
    unset 'UNIT_START[$pid]'
    drop_inflight "$spec"
    DONE_UNITS=$((DONE_UNITS + 1))
    if [ "$SECONDS" -ge "$start_s" ]; then
      DUR_SUM=$((DUR_SUM + SECONDS - start_s))
      DUR_N=$((DUR_N + 1))
    fi
    cat "$log" >>"$HARNESS_DIR/harbor.log" || true
    if grep -Eiq 'No such option|unexpected argument|unrecognized arguments' "$log"; then
      die "harbor CLI rejected flags (see $log). Not recording as a successful stage."
    fi
    echo "$rc" >"$HARNESS_DIR/exit_code.txt"
    if [ "$rc" -ne 0 ]; then
      LAST_RC="$rc"
    fi
    tid="${spec%%:*}"
    attempt="${spec##*:}"
    att_tag="$(printf '%02d' "$attempt")"
    job_name="${tid}_icode_${BUILD_NUMBER:-local}_a${att_tag}"
    if [ "${BENCHMARK:-deepswe}" = "lolbench" ]; then
      job_name="${tid}_icode_union_${BUILD_NUMBER:-local}_a${att_tag}"
    fi
    reward="$(find "$jobs/$job_name" -name reward.json -type f -print -quit 2>/dev/null || true)"
    if [ -z "$reward" ]; then
      echo "WARNING: no reward.json for $spec (unscored / no-response; see $log)"
    else
      echo "OK unit=$spec"
      if [ "$rc" -ne 0 ]; then
        echo "WARNING: harbor run exited $rc for $spec. reward.json present; treating as a score."
      fi
    fi
    # OOM detect: require exit/status context for 137 so Harbor f2p totals like
    # "137 passed" / table cell "137" are not false positives. Only skip remaining
    # attempts when this unit has no reward.json (real unscored OOM).
    if [ -z "$reward" ] && grep -Eiq \
      'out of memory|Cannot allocate memory|oom-kill|oom_kill|exit(ed)?[[:space:]]+(with[[:space:]]+)?(status|code)[[:space:]]*137([^0-9]|$)|ExitCode[=:[:space:]]*137([^0-9]|$)' \
      "$log"; then
      echo "P5 harbor: skip question=$tid (out of memory); continuing"
      if ! grep -qx "$tid" "$HARNESS_DIR/skipped_questions.txt" 2>/dev/null; then
        echo "$tid" >>"$HARNESS_DIR/skipped_questions.txt"
      fi
      local rest
      for rest in $(seq 1 "$N_ROLLOUTS"); do
        case ",${ASSIGNED}," in
          *",${tid}:${rest},"*) continue ;;
        esac
        if [ -n "$ASSIGNED" ]; then
          ASSIGNED="${ASSIGNED},${tid}:${rest}"
        else
          ASSIGNED="${tid}:${rest}"
        fi
        STARTED_UNITS=$((STARTED_UNITS + 1))
        DONE_UNITS=$((DONE_UNITS + 1))
      done
    fi
    if [ -z "$INFLIGHT" ]; then
      start_q="${QUESTION_START[$tid]:-$SECONDS}"
      elapsed=$((SECONDS - start_q))
      if [ "$elapsed" -lt 0 ]; then
        elapsed=0
      fi
      span="$((elapsed % 60))s"
      if [ "$elapsed" -ge 60 ]; then
        span="$((elapsed / 60))m $((elapsed % 60))s"
      fi
      echo "P5 harbor: question=$tid rollouts=${N_ROLLOUTS}/${N_ROLLOUTS} time=$span"
      python3 "$PIPELINE_LIB/provenance.py" record-image \
        --inputs "$WORKDIR/eval_protocol_inputs.json" \
        --task-id "$tid" \
        --task-toml "$TASKS_DIR/${tid}/task.toml" || true
      python3 "$PIPELINE_LIB/eval_slots.py" flush \
        --question "${spec%%:*}" \
        --slots "${EVAL_SLOTS:-0}" \
        --memory-mb "${EVAL_MEMORY_MB:-0}" \
        --benchmark "${BENCHMARK:-deepswe}" \
        --workdir "$WORKDIR" || true
      rm -f "$HARNESS_DIR/active_question.txt"
    fi
  done
  return "$waited"
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
  live_slots || die "eval_slots.py failed"
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

# MAC_K3D_P5_DRY_RUN=1: print the first unit's harbor command with secret values masked; run nothing.
if [ "${MAC_K3D_P5_DRY_RUN:-0}" = 1 ]; then
  live_slots || die "eval_slots.py failed"
  build_unit_cmd "${TASK_IDS[0]}" 1
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

echo "P5 harbor: one question's rollouts together; next question after they exit; heartbeats every 60s"
write_progress
(
  while true; do
    sleep 60
    python3 "$PIPELINE_LIB/eval_progress.py" heartbeat --progress "$HARNESS_DIR/progress.json" --elapsed "$SECONDS" || true
    python3 "$PIPELINE_LIB/eval_slots.py" sample \
      --workdir "$WORKDIR" \
      --slots "${EVAL_SLOTS:-0}" \
      --memory-mb "${EVAL_MEMORY_MB:-0}" \
      --benchmark "${BENCHMARK:-deepswe}" || true
  done
) &
heartbeat_pid=$!

while [ "$STARTED_UNITS" -lt "$NEEDED" ] || [ "${#UNIT_SPEC[@]}" -gt 0 ]; do
  live_slots || die "eval_slots.py failed"
  write_progress
  while [ "${#UNIT_SPEC[@]}" -lt "$EVAL_SLOTS" ] && [ "$STARTED_UNITS" -lt "$NEEDED" ]; do
    unit="$(next_work_unit || true)"
    [ -n "$unit" ] || break
    start_unit $unit
    write_progress
  done
  if [ "${#UNIT_SPEC[@]}" -eq 0 ]; then
    break
  fi
  set +e
  wait -n
  set -e
  reap_finished || true
  write_progress
done

kill "$heartbeat_pid" 2>/dev/null || true
wait "$heartbeat_pid" 2>/dev/null || true
icode_reclaim_host_tree "$HOST_ICODE"

DURATION="$SECONDS"
FINISHED_AT="$(date -u +%Y-%m-%dT%H:%M:%SZ)"
write_harness_meta "$LAST_RC"
progress 70 "P5 complete (exit=$LAST_RC)"
