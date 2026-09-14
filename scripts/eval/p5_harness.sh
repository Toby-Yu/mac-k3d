#!/usr/bin/env bash
# P5 — one DeepSWE task through Pier + iCode agent
set -euo pipefail
source "$(cd "$(dirname "$0")" && pwd)/_common.sh"

progress 55 "P5: Pier+iCode harness arm (n=$N_TASKS)"

[ -d "$DEEPSWE_DIR/tasks" ] || die "run P2 first (missing deep-swe/tasks)"
[ -f "$WORKDIR/icode_bin_path.txt" ] || bash "$(dirname "$0")/p3_icode.sh"
[ -n "${DEEPSEEK_API_KEY:-}" ] || die "$(missing_deepseek_key_hint)"
[ -f "$MAC_K3D_ROOT/eval/icode_pier_agent.py" ] || die "missing eval/icode_pier_agent.py"
have pier || die "pier not on PATH (run P1)"

export ICODE_BIN
ICODE_BIN="$(cat "$WORKDIR/icode_bin_path.txt")"
export PIER_AGENTS_PATH="${PIER_AGENTS_PATH:-$PIER_AGENT_DIR}"
export DEEPSEEK_MODEL="${DEEPSEEK_MODEL:-deepseek-v4-pro}"
export PYTHONPATH="$MAC_K3D_ROOT/eval${PYTHONPATH:+:$PYTHONPATH}"
mkdir -p "$HARNESS_DIR"

ensure_selected_tasks
FIRST_TASK=""
while read -r tid; do
  [ -n "$tid" ] || continue
  FIRST_TASK="$DEEPSWE_DIR/tasks/$tid"
  break
done <"$WORKDIR/selected_tasks.txt"
[ -n "$FIRST_TASK" ] && [ -d "$FIRST_TASK" ] || die "no tasks under $DEEPSWE_DIR/tasks"

HOST_ICODE=""
ICODE_BIN_HOST_IN_SANDBOX=""
ICODE_BIN_IN_SANDBOX="icode"
if [ "${ICODE_MODE}" = "source" ]; then
  [ -d "$ICODE_SOURCE" ] || die "ICODE_SOURCE missing: $ICODE_SOURCE"
  HOST_ICODE="$(cd "$ICODE_SOURCE" && pwd)"
elif [ -n "${ICODE_RELEASE:-}" ] && [[ "$ICODE_RELEASE" == http://* || "$ICODE_RELEASE" == https://* ]]; then
  HOST_ICODE=""
elif [ -n "${ICODE_BIN:-}" ] && [ -f "$ICODE_BIN" ]; then
  HOST_ICODE="$(cd "$(dirname "$ICODE_BIN")" && pwd)"
  ICODE_BIN_HOST_IN_SANDBOX="/opt/icode-host/$(basename "$ICODE_BIN")"
else
  die "cannot resolve host iCode tree to bind-mount (set ICODE_SOURCE or a real ICODE_BIN path)"
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
  printf 'ICODE_SOURCE_HOST=%s\n' "/opt/icode-host"
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
  CMD+=(-p "$DEEPSWE_DIR/tasks" --n-tasks "$N_TASKS")
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
CMD+=(--ae "ICODE_SOURCE_HOST=/opt/icode-host")
CMD+=(--ae "ICODE_BIN=${ICODE_BIN_IN_SANDBOX}")
if [ -n "$ICODE_BIN_HOST_IN_SANDBOX" ]; then
  CMD+=(--ae "ICODE_BIN_HOST=${ICODE_BIN_HOST_IN_SANDBOX}")
fi

STARTED_AT="$(date -u +%Y-%m-%dT%H:%M:%SZ)"
SECONDS=0
set +e
"${CMD[@]}" 2>&1 | tee "$HARNESS_DIR/pier.log"
rc=${PIPESTATUS[0]:-1}
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
  python3 - "$MAC_K3D_ROOT/eval/pier_result.py" "$HARNESS_DIR" "$STARTED_AT" <<'PY'
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

python3 - "$HARNESS_DIR" "$DEEPSEEK_MODEL" "$STARTED_AT" "$FINISHED_AT" "$DURATION" "$rc" <<'PY'
import json, re, sys
from pathlib import Path

harness, model, started, finished, duration, rc = sys.argv[1:7]
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

prompt = first_int(r'"prompt_tokens"\s*:\s*(\d+)', r"\bprompt_tokens[=:\s]+(\d+)")
completion = first_int(r'"completion_tokens"\s*:\s*(\d+)', r"\bcompletion_tokens[=:\s]+(\d+)")
total = first_int(r'"total_tokens"\s*:\s*(\d+)', r"\btotal_tokens[=:\s]+(\d+)")
if total == 0:
    total = prompt + completion
served = None
m = re.search(r'"model"\s*:\s*"([^"]+)"', blob)
if m:
    served = m.group(1)
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

progress 70 "P5 complete (exit=$rc)"
