#!/bin/bash
# Generate REMUR difftest reference trace from a Spike run.
#
# Usage:
#   scripts/gen_spike_ref.sh <elf> [output.ref] [-- spike-extra-args...]
#
# Environment:
#   SPIKE_BIN               spike executable path (default: spike)
#   REMUR_DIFFTEST_REF_OUT  fallback output path when [output.ref] omitted

set -euo pipefail

if [ "$#" -lt 1 ]; then
    echo "Usage: $0 <elf> [output.ref] [-- spike-extra-args...]" >&2
    exit 1
fi

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
ELF="$1"
shift

OUT=""
if [ "$#" -gt 0 ] && [ "$1" != "--" ]; then
    OUT="$1"
    shift
else
    OUT="${REMUR_DIFFTEST_REF_OUT:-}"
fi

if [ -z "$OUT" ]; then
    echo "Output path not provided. Pass [output.ref] or set REMUR_DIFFTEST_REF_OUT." >&2
    exit 1
fi

if [ "$#" -gt 0 ] && [ "$1" = "--" ]; then
    shift
fi

SPIKE_BIN="${SPIKE_BIN:-spike}"
LOG_PATH="target/trace/spike.log"
mkdir -p "$(dirname "$OUT")" "$(dirname "$LOG_PATH")"

"$SPIKE_BIN" -l "$@" "$ELF" > "$LOG_PATH" 2>&1
python3 "$SCRIPT_DIR/spike_to_remur_trace.py" --input "$LOG_PATH" --output "$OUT"

echo "Generated REMUR difftest reference: $OUT"
