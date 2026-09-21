#!/usr/bin/env bash
# P5 — DeepSWE: Pier + iCode. LoLBench: Harbor + iCode.
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
blob = ""
for p in root.rglob("*"):
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
usage = find_icode_usage(root)
if usage:
    if not prompt:
        prompt = int(usage.get("prompt") or 0)
    if not completion:
        completion = int(usage.get("completion") or 0)
    if not total:
        total = int(usage.get("total") or 0)
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

if [ "${BENCHMARK:-deepswe}" = "lolbench" ]; then
  progress 55 "P5: Harbor+iCode harness arm (benchmark=lolbench n=$N_TASKS)"
  TASKS_DIR="$(benchmark_tasks_dir)"
  [ -d "$TASKS_DIR" ] || die "run P2 first (missing $TASKS_DIR)"
  [ -n "${DEEPSEEK_API_KEY:-}" ] || die "$(missing_deepseek_key_hint)"
  have harbor || die "harbor not on PATH (run P1)"
  [ -f "$PIPELINE_LIB/icode_harbor_agent.py" ] || die "missing pipeline/lib/icode_harbor_agent.py"
  [ -f "$WORKDIR/icode_bin_path.txt" ] || bash "$(dirname "$0")/p3_icode.sh"
  ICODE_BIN="$(cat "$WORKDIR/icode_bin_path.txt")"
  [ -n "$ICODE_BIN" ] && [ -f "$ICODE_BIN" ] || die "P3 did not resolve an icode binary (needed for Harbor bind-mount; GitCode clone is not used)"
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
  [ -n "$TASK_ID" ] && [ -d "$TASKS_DIR/$TASK_ID" ] || die "no LoLBench task under $TASKS_DIR"

  JOBS_DIR="$HARNESS_DIR/harbor_runs/jenkins-${BUILD_NUMBER:-local}/${TASK_ID}"
  JOB_NAME="${TASK_ID}_icode_union_${BUILD_NUMBER:-local}"
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
    if [ -n "${GITCODE_TOKEN:-}" ]; then
      printf 'GITCODE_TOKEN=%s\n' "${GITCODE_TOKEN}"
    fi
  } >"$HARBOR_ENV"
  chmod 600 "$HARBOR_ENV"

  echo "P5 harbor: -p harbor_tasks/${TASK_ID} -a icode_harbor_agent:ICodeAgent -m ${DEEPSEEK_MODEL}"
  echo "P5 harbor: --jobs-dir $JOBS_DIR --job-name $JOB_NAME (key via env-file, not printed)"

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

  CMD=(harbor run)
  CMD+=(-p "harbor_tasks/${TASK_ID}")
  CMD+=(-a "icode_harbor_agent:ICodeAgent")
  CMD+=(-m "${DEEPSEEK_MODEL}")
  CMD+=(--allow-agent-host api.deepseek.com)
  CMD+=(--allow-agent-host api.deepseek.ai)
  CMD+=(--agent-setup-timeout-multiplier 10)
  CMD+=(--job-name "$JOB_NAME")
  CMD+=(--jobs-dir "$JOBS_DIR")
  CMD+=(--no-delete)
  CMD+=(-n 1)
  CMD+=(-y)
  CMD+=(--env-file "$HARBOR_ENV")
  CMD+=(--ve "LOLBENCH_SUITE=union")
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
  if [ -n "${GITCODE_TOKEN:-}" ]; then
    CMD+=(--ae "GITCODE_TOKEN=${GITCODE_TOKEN}")
  fi

  STARTED_AT="$(date -u +%Y-%m-%dT%H:%M:%SZ)"
  SECONDS=0
  echo "P5 harbor: agent can take many minutes; heartbeats every 60s (Harbor itself prints little)"
  set +e
  : >"$HARNESS_DIR/harbor.log"
  (
    cd "$LOLBENCH_DIR"
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
  exit 0
fi

progress 55 "P5: Pier+iCode harness arm (benchmark=$BENCHMARK n=$N_TASKS)"

TASKS_DIR="$(benchmark_tasks_dir)"
[ -d "$TASKS_DIR" ] || die "run P2 first (missing $TASKS_DIR)"
[ -f "$WORKDIR/icode_bin_path.txt" ] || bash "$(dirname "$0")/p3_icode.sh"
[ -n "${DEEPSEEK_API_KEY:-}" ] || die "$(missing_deepseek_key_hint)"
[ -f "$PIPELINE_LIB/icode_pier_agent.py" ] || die "missing pipeline/lib/icode_pier_agent.py"
have pier || die "pier not on PATH (run P1)"

export ICODE_BIN
ICODE_BIN="$(cat "$WORKDIR/icode_bin_path.txt")"
export PIER_AGENTS_PATH="${PIER_AGENTS_PATH:-$PIER_AGENT_DIR}"
export DEEPSEEK_MODEL="${DEEPSEEK_MODEL:-deepseek-v4-pro}"
export PYTHONPATH="$PIPELINE_LIB${PYTHONPATH:+:$PYTHONPATH}"
mkdir -p "$HARNESS_DIR"

ensure_selected_tasks
FIRST_TASK=""
while read -r tid; do
  [ -n "$tid" ] || continue
  FIRST_TASK="$TASKS_DIR/$tid"
  break
done <"$WORKDIR/selected_tasks.txt"
[ -n "$FIRST_TASK" ] && [ -d "$FIRST_TASK" ] || die "no tasks under $TASKS_DIR"

HOST_ICODE=""
ICODE_BIN_HOST_IN_SANDBOX=""
ICODE_BIN_IN_SANDBOX="icode"
if [ -s "$WORKDIR/icode_host_root.txt" ]; then
  HOST_ICODE="$(cat "$WORKDIR/icode_host_root.txt")"
  [ -d "$HOST_ICODE" ] || die "icode host tree missing: $HOST_ICODE"
  ICODE_BIN_HOST_IN_SANDBOX="/opt/icode-host/$(basename "${ICODE_BIN:-icode}")"
  if [ -x "$HOST_ICODE/.venv/bin/icode" ]; then
    icode_embed_sandbox_cpython "$HOST_ICODE"
    icode_write_wrapper "$HOST_ICODE/icode"
  fi
elif [ -n "${ICODE_RELEASE:-}" ] && [[ "$ICODE_RELEASE" == http://* || "$ICODE_RELEASE" == https://* ]]; then
  HOST_ICODE=""
elif [ -n "${ICODE_BIN:-}" ] && [ -f "$ICODE_BIN" ]; then
  HOST_ICODE="$(cd "$(dirname "$ICODE_BIN")" && pwd)"
  ICODE_BIN_HOST_IN_SANDBOX="/opt/icode-host/$(basename "$ICODE_BIN")"
else
  die "cannot resolve host iCode tree to bind-mount (icode_host_root.txt or a real ICODE_BIN path)"
fi

PIER_HELP="$(pier run --help 2>&1 || true)"
echo "$PIER_HELP" | grep -q -- '--agent-import-path' || die "this pier has no --agent-import-path; need datacurve-pier 0.3.x"

MOUNTS_JSON="[]"
if [ -n "$HOST_ICODE" ]; then
  MOUNTS_JSON="$(
    python3 - "$HOST_ICODE" <<'PY'
import json, sys
print(json.dumps([{"type": "bind", "source": sys.argv[1], "target": "/opt/icode-host"}]))
PY
  )"
fi

# Secrets go in this gitignored workdir file — never on argv / --ae.
PIER_ENV_FILE="$WORKDIR/.pier-env"
umask 077
{
  printf 'DEEPSEEK_API_KEY=%s\n' "${DEEPSEEK_API_KEY}"
  printf 'DEEPSEEK_MODEL=%s\n' "${DEEPSEEK_MODEL}"
  printf 'ICODE_MODEL=%s\n' "${DEEPSEEK_MODEL}"
  printf 'ICODE_API_BASE=%s\n' "https://api.deepseek.com"
  printf 'ICODE_PROVIDER=%s\n' "DeepSeek"
  printf 'ICODE_SOURCE_HOST=%s\n' "/opt/icode-host"
  printf 'PYTHONDONTWRITEBYTECODE=%s\n' "1"
  printf 'ICODE_BIN=%s\n' "${ICODE_BIN_IN_SANDBOX}"
  if [ -n "$ICODE_BIN_HOST_IN_SANDBOX" ]; then
    printf 'ICODE_BIN_HOST=%s\n' "$ICODE_BIN_HOST_IN_SANDBOX"
  fi
  if [ -n "${ICODE_RELEASE:-}" ] && [[ "$ICODE_RELEASE" == http://* || "$ICODE_RELEASE" == https://* ]]; then
    printf 'ICODE_RELEASE_URL=%s\n' "$ICODE_RELEASE"
  fi
} >"$PIER_ENV_FILE"
chmod 600 "$PIER_ENV_FILE"

CMD=(pier run)
if echo "$PIER_HELP" | grep -q -- '--n-tasks'; then
  CMD+=(-p "$TASKS_DIR" --n-tasks "$N_TASKS")
  if echo "$PIER_HELP" | grep -q -- '--include-task-name'; then
    while read -r tid; do
      [ -n "$tid" ] || continue
      CMD+=(--include-task-name "$tid")
    done <"$WORKDIR/selected_tasks.txt"
  fi
else
  CMD+=(-p "$FIRST_TASK")
fi
CMD+=(--agent-import-path "icode_pier_agent:ICodeAgent")
CMD+=(--model "${DEEPSEEK_MODEL}")
CMD+=(-o "$HARNESS_DIR")
if echo "$PIER_HELP" | grep -q -- '--yes'; then
  CMD+=(-y)
fi
if echo "$PIER_HELP" | grep -q -- '--env-file'; then
  CMD+=(--env-file "$PIER_ENV_FILE")
fi
if echo "$PIER_HELP" | grep -q -- '--agent-dir'; then
  CMD+=(--agent-dir "$PIER_AGENT_DIR")
fi
if echo "$PIER_HELP" | grep -q -- '--mounts-json' && [ "$MOUNTS_JSON" != "[]" ]; then
  CMD+=(--mounts-json "$MOUNTS_JSON")
fi
# Non-secret agent env only. API key is in --env-file (pier process → adapter os.environ).
# ICODE_API_* match Harbor P5 so the binary can reach DeepSeek.
CMD+=(--ae "ICODE_SOURCE_HOST=/opt/icode-host")
CMD+=(--ae "PYTHONDONTWRITEBYTECODE=1")
CMD+=(--ae "ICODE_BIN=${ICODE_BIN_IN_SANDBOX}")
CMD+=(--ae "ICODE_API_BASE=https://api.deepseek.com")
CMD+=(--ae "ICODE_PROVIDER=DeepSeek")
CMD+=(--ae "ICODE_MODEL=${DEEPSEEK_MODEL}")
if [ -n "$ICODE_BIN_HOST_IN_SANDBOX" ]; then
  CMD+=(--ae "ICODE_BIN_HOST=${ICODE_BIN_HOST_IN_SANDBOX}")
fi

STARTED_AT="$(date -u +%Y-%m-%dT%H:%M:%SZ)"
SECONDS=0
set +e
"${CMD[@]}" 2>&1 | tee "$HARNESS_DIR/pier.log"
rc=${PIPESTATUS[0]:-1}
icode_reclaim_host_tree "$HOST_ICODE"
set -e
DURATION="$SECONDS"
FINISHED_AT="$(date -u +%Y-%m-%dT%H:%M:%SZ)"

if grep -Eiq 'No such option|unexpected argument' "$HARNESS_DIR/pier.log"; then
  die "pier CLI rejected flags (see $HARNESS_DIR/pier.log). Not recording as a successful stage."
fi

if [ "$rc" -ne 0 ]; then
  echo "WARNING: pier run exited $rc (see $HARNESS_DIR/pier.log). Stage still recorded."
fi
echo "$rc" >"$HARNESS_DIR/exit_code.txt"

HOLLOW="$(
  python3 - "$PIPELINE_LIB/pier_result.py" "$HARNESS_DIR" "$STARTED_AT" <<'PY'
import json, sys
from datetime import datetime, timezone
from pathlib import Path

mod = Path(sys.argv[1])
root = Path(sys.argv[2])
started = sys.argv[3]
sys.path.insert(0, str(mod.parent))
from pier_result import hollow_job_reason

try:
    start_ts = datetime.strptime(started, "%Y-%m-%dT%H:%M:%SZ").replace(tzinfo=timezone.utc).timestamp()
except ValueError:
    start_ts = 0.0
newest = None
newest_mtime = -1.0
for p in root.rglob("result.json"):
    try:
        m = p.stat().st_mtime
    except OSError:
        continue
    if m + 2 < start_ts:
        continue
    if m >= newest_mtime:
        newest_mtime = m
        newest = p
if newest is None:
    print("no pier result.json from this P5 run")
    raise SystemExit(0)
doc = json.loads(newest.read_text(encoding="utf-8"))
reason = hollow_job_reason(doc)
if reason:
    print(f"{newest}: {reason}")
PY
)"
if [ -n "$HOLLOW" ]; then
  die "P5 hollow job: $HOLLOW"
fi

write_harness_meta "$rc"
progress 70 "P5 complete (exit=$rc)"
