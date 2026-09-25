#!/usr/bin/env bash
# EVAL_SLOTS = min(CPU_LOCK_QTY, RAM ceiling). Low free disk forces one slot.
eval_parallel_degree() {
  N_ROLLOUTS="${N_ROLLOUTS:-1}"
  CPU_LOCK_QTY="${CPU_LOCK_QTY:-1}"
  case "$N_ROLLOUTS" in
    ''|*[!0-9]*) echo "N_ROLLOUTS must be an integer >= 1" >&2; return 1 ;;
  esac
  case "$CPU_LOCK_QTY" in
    ''|*[!0-9]*) echo "CPU_LOCK_QTY must be an integer >= 1" >&2; return 1 ;;
  esac
  if [ "$N_ROLLOUTS" -lt 1 ] || [ "$CPU_LOCK_QTY" -lt 1 ]; then
    echo "N_ROLLOUTS and CPU_LOCK_QTY must be integers >= 1" >&2
    return 1
  fi
  local lib="${PIPELINE_LIB:-$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)}"
  local out
  if ! out="$(
    python3 "$lib/eval_slots.py" slots \
      --cpu "$CPU_LOCK_QTY" \
      --benchmark "${BENCHMARK:-deepswe}" \
      --workdir "${WORKDIR:-.}"
  )"; then
    echo "eval_slots.py failed" >&2
    return 1
  fi
  eval "$out"
  if [ -z "${EVAL_SLOTS:-}" ]; then
    EVAL_SLOTS="$CPU_LOCK_QTY"
  fi
  if [ -z "${EVAL_CPUS_EACH:-}" ]; then
    EVAL_CPUS_EACH=$((CPU_LOCK_QTY / EVAL_SLOTS))
  fi
  if [ "${EVAL_FITS:-1}" != 0 ] && [ "$EVAL_SLOTS" -lt 1 ]; then
    EVAL_SLOTS=1
  fi
  if [ "$EVAL_CPUS_EACH" -lt 1 ]; then
    EVAL_CPUS_EACH=1
  fi
  EVAL_PARALLEL="$EVAL_SLOTS"
  export N_ROLLOUTS CPU_LOCK_QTY EVAL_SLOTS EVAL_PARALLEL EVAL_CPUS_EACH EVAL_FITS EVAL_MEMORY_MB
}
