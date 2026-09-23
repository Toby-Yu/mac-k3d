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
TASKS_DIR="$(benchmark_tasks_dir)"
[ -d "$TASKS_DIR" ] || die "run P2 first (missing $TASKS_DIR)"
[ -n "${DEEPSEEK_API_KEY:-}" ] || die "$(missing_deepseek_key_hint)"
have harbor || die "harbor not on PATH (run P1)"
[ -f "$PIPELINE_LIB/icode_harbor_agent.py" ] || die "missing pipeline/lib/icode_harbor_agent.py"
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
  icode_write_wrapper "$HOST_ICODE/icode"
else
  echo "P5 harbor: keeping release binary at $HOST_ICODE/$(basename "${ICODE_BIN:-icode}") (no git wrapper)"
fi

export DEEPSEEK_MODEL="${DEEPSEEK_MODEL:-deepseek-v4-pro}"
export ICODE_MODEL="${ICODE_MODEL:-$DEEPSEEK_MODEL}"
mkdir -p "$HARNESS_DIR"
ensure_selected_tasks
TASK_ID=""
while read -r tid; do
  [ -n "$tid" ] || continue
  TASK_ID="$tid"
  break
done <"$WORKDIR/selected_tasks.txt"
[ -n "$TASK_ID" ] && [ -d "$TASKS_DIR/$TASK_ID" ] || die "no task under $TASKS_DIR (wanted ${TASK_ID:-<empty>})"

JOBS_DIR="$HARNESS_DIR/harbor_runs/jenkins-${BUILD_NUMBER:-local}/${TASK_ID}"
JOB_NAME="${TASK_ID}_icode_${BUILD_NUMBER:-local}"
mkdir -p "$JOBS_DIR"
printf '%s\n' "$JOBS_DIR" >"$HARNESS_DIR/harbor_jobs_dir.txt"

HARBOR_ENV="$WORKDIR/.harbor-env"
umask 077
{
  printf 'DEEPSEEK_API_KEY=%s\n' "${DEEPSEEK_API_KEY}"
  printf 'DEEPSEEK_MODEL=%s\n' "${DEEPSEEK_MODEL}"
  printf 'ICODE_MODEL=%s\n' "${ICODE_MODEL}"
  printf 'ICODE_API_BASE=%s\n' "https://api.deepseek.com"
  printf 'ICODE_PROVIDER=%s\n' "DeepSeek"
  printf 'PYTHONDONTWRITEBYTECODE=%s\n' "1"
  printf 'MAC_K3D_BENCHMARK=%s\n' "${BENCHMARK:-deepswe}"
  if [ -n "${GITCODE_TOKEN:-}" ]; then
    printf 'GITCODE_TOKEN=%s\n' "${GITCODE_TOKEN}"
  fi
} >"$HARBOR_ENV"
chmod 600 "$HARBOR_ENV"

RUN_DIR="$WORKDIR"
TASK_PATH="$TASKS_DIR/$TASK_ID"
if [ "${BENCHMARK:-deepswe}" = "lolbench" ]; then
  JOB_NAME="${TASK_ID}_icode_union_${BUILD_NUMBER:-local}"
  RUN_DIR="$LOLBENCH_DIR"
  TASK_PATH="harbor_tasks/${TASK_ID}"
  echo "P5 harbor: -p ${TASK_PATH} -a icode_harbor_agent:ICodeAgent -m ${DEEPSEEK_MODEL}"
  TASK_TOML="$TASKS_DIR/${TASK_ID}/task.toml"
  LOLBENCH_IMAGE="$(
    python3 - "$TASK_TOML" "$TASK_ID" <<'PY'
import re, sys
path, tid = sys.argv[1], sys.argv[2]
text = open(path, encoding="utf-8").read() if path else ""
m = re.search(r'^docker_image\s*=\s*"([^"]+)"', text, re.M)
print(m.group(1) if m else f"smartdub26/lolbench:{tid}-1.0.0")
PY
  )"
  ENV_DIR="$TASKS_DIR/${TASK_ID}/environment"
  if docker image inspect "$LOLBENCH_IMAGE" >/dev/null 2>&1; then
    echo "P5 harbor: using local image $LOLBENCH_IMAGE"
  else
    leftover="$(docker images --format '{{.Repository}}:{{.Tag}}' | grep -E "^${TASK_ID}__.*__env-main" | head -n 1 || true)"
    if [ -n "$leftover" ]; then
      echo "P5 harbor: retag $leftover -> $LOLBENCH_IMAGE (skip Harbor force build; it hangs after tagging)"
      docker tag "$leftover" "$LOLBENCH_IMAGE"
    else
      [ -f "$ENV_DIR/Dockerfile" ] || die "missing $ENV_DIR/Dockerfile and no local $LOLBENCH_IMAGE"
      echo "P5 harbor: Hub tag is arm64-only; docker build --progress=plain $LOLBENCH_IMAGE"
      docker build --progress=plain -t "$LOLBENCH_IMAGE" "$ENV_DIR"
    fi
  fi
else
  echo "P5 harbor: -p ${TASK_PATH} -a icode_harbor_agent:ICodeAgent -m ${DEEPSEEK_MODEL}"
fi
echo "P5 harbor: --jobs-dir $JOBS_DIR --job-name $JOB_NAME (key via env-file, not printed)"

CMD=(harbor run)
CMD+=(-p "$TASK_PATH")
CMD+=(-a "icode_harbor_agent:ICodeAgent")
CMD+=(-m "${DEEPSEEK_MODEL}")
CMD+=(--allow-agent-host api.deepseek.com)
CMD+=(--allow-agent-host api.deepseek.ai)
CMD+=(--agent-setup-timeout-multiplier 10)
CMD+=(--job-name "$JOB_NAME")
CMD+=(--jobs-dir "$JOBS_DIR")
CMD+=(--no-delete)
CMD+=(-n 1)
N_ROLLOUTS="${N_ROLLOUTS:-1}"
case "$N_ROLLOUTS" in
  ''|*[!0-9]*) die "N_ROLLOUTS must be an integer >= 1" ;;
esac
if [ "$N_ROLLOUTS" -lt 1 ]; then
  die "N_ROLLOUTS must be an integer >= 1"
fi
CMD+=(-k "$N_ROLLOUTS")
echo "P5 harbor: n_rollouts=$N_ROLLOUTS (sequential; -n 1)"
CMD+=(-y)
CMD+=(--env-file "$HARBOR_ENV")
if [ "${BENCHMARK:-deepswe}" = "lolbench" ]; then
  CMD+=(--ve "LOLBENCH_SUITE=union")
fi
MOUNTS_JSON="$(
  python3 - "$HOST_ICODE" <<'PY'
import json, sys
print(json.dumps([{"type": "bind", "source": sys.argv[1], "target": "/opt/icode-host"}]))
PY
)"
CMD+=(--mounts "$MOUNTS_JSON")
CMD+=(--ae "ICODE_MODEL=${ICODE_MODEL}")
CMD+=(--ae "ICODE_API_BASE=https://api.deepseek.com")
CMD+=(--ae "ICODE_PROVIDER=DeepSeek")
CMD+=(--ae "PYTHONDONTWRITEBYTECODE=1")
CMD+=(--ae "DEEPSEEK_MODEL=${DEEPSEEK_MODEL}")
CMD+=(--ae "DEEPSEEK_API_KEY=${DEEPSEEK_API_KEY}")
CMD+=(--ae "MAC_K3D_BENCHMARK=${BENCHMARK:-deepswe}")
if [ -n "${GITCODE_TOKEN:-}" ]; then
  CMD+=(--ae "GITCODE_TOKEN=${GITCODE_TOKEN}")
fi

STARTED_AT="$(date -u +%Y-%m-%dT%H:%M:%SZ)"
SECONDS=0
echo "P5 harbor: agent can take many minutes; heartbeats every 60s (Harbor itself prints little)"
set +e
: >"$HARNESS_DIR/harbor.log"
(
  cd "$RUN_DIR"
  export PYTHONPATH="$PIPELINE_LIB${PYTHONPATH:+:$PYTHONPATH}"
  export PYTHONUNBUFFERED=1
  if command -v stdbuf >/dev/null 2>&1; then
    stdbuf -oL -eL "${CMD[@]}"
  else
    "${CMD[@]}"
  fi
) >>"$HARNESS_DIR/harbor.log" 2>&1 &
harbor_pid=$!
(
  while kill -0 "$harbor_pid" 2>/dev/null; do
    sleep 60
    kill -0 "$harbor_pid" 2>/dev/null || break
    n_events=0
    ev="$(find "$JOBS_DIR" -name events.jsonl -type f 2>/dev/null | head -n 1 || true)"
    if [ -n "$ev" ]; then
      n_events="$(wc -l <"$ev" | tr -d ' ')"
    fi
    echo "P5 harbor heartbeat ${SECONDS}s events=${n_events} $(date -u +%H:%M:%SZ)"
  done
) &
heartbeat_pid=$!
wait "$harbor_pid"
rc=$?
icode_reclaim_host_tree "$HOST_ICODE"
kill "$heartbeat_pid" 2>/dev/null || true
wait "$heartbeat_pid" 2>/dev/null || true
if [ -s "$HARNESS_DIR/harbor.log" ]; then
  echo "P5 harbor: last 40 lines of harbor.log"
  tail -n 40 "$HARNESS_DIR/harbor.log" || true
fi
set -e
DURATION="$SECONDS"
FINISHED_AT="$(date -u +%Y-%m-%dT%H:%M:%SZ)"
echo "$rc" >"$HARNESS_DIR/exit_code.txt"

if grep -Eiq 'No such option|unexpected argument|unrecognized arguments' "$HARNESS_DIR/harbor.log"; then
  die "harbor CLI rejected flags (see $HARNESS_DIR/harbor.log). Not recording as a successful stage."
fi

REWARD="$(find "$JOBS_DIR" -name reward.json -type f 2>/dev/null | head -n 1 || true)"
if [ -z "$REWARD" ]; then
  die "P5 Harbor produced no reward.json under $JOBS_DIR (infra fail; reward 0 would still write the file). See $HARNESS_DIR/harbor.log"
fi
echo "OK harbor reward.json=$REWARD"
if [ "$rc" -ne 0 ]; then
  echo "WARNING: harbor run exited $rc (see $HARNESS_DIR/harbor.log). reward.json present; treating as a score."
fi

write_harness_meta "$rc"
progress 70 "P5 complete (exit=$rc)"
