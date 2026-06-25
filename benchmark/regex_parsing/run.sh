#!/usr/bin/env bash
#
# Profiles Vector's regex parsing under load and produces a flamegraph.
#
# Drives Vector's http_server source with lading's apache_common HTTP payload,
# samples the running Vector process, and emits a flamegraph plus a remap-only
# CPU breakdown. Compare two runs by passing different LABELs.
#
# Usage:
#   run.sh                           # default label = timestamp
#   run.sh baseline                  # named run
#   VECTOR_BIN=/path/to/vector run.sh baseline
#
# Prerequisites (macOS):
#   - lading             (cargo install lading)
#   - inferno            (cargo install inferno)
#   - sample             (ships with macOS)
#
# Vector must be built with debug symbols. Build with:
#   cargo build --profile bench --no-default-features \
#       --features "sources-http_server,transforms-remap,sinks-http,vrl/stdlib"
#
# Note: macOS-only. On Linux, swap `sample` for `perf record` and
# `inferno-collapse-sample` for `inferno-collapse-perf`.

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/../.." && pwd)"

# Configurable via env vars
VECTOR_BIN="${VECTOR_BIN:-$REPO_ROOT/target/release/vector}"
VECTOR_CONFIG="${VECTOR_CONFIG:-$SCRIPT_DIR/vector.yaml}"
LADING_CONFIG="${LADING_CONFIG:-$SCRIPT_DIR/lading.yaml}"
OUT_DIR="${OUT_DIR:-/tmp/vector-regex-bench}"
SAMPLE_SECONDS="${SAMPLE_SECONDS:-30}"
WARMUP_SECONDS="${WARMUP_SECONDS:-12}"
EXPERIMENT_SECONDS="${EXPERIMENT_SECONDS:-60}"

LABEL="${1:-$(date +%Y%m%d-%H%M%S)}"
RUN_DIR="$OUT_DIR/$LABEL"

# Sanity checks
[[ -x "$VECTOR_BIN" ]]      || { echo "Vector binary not found: $VECTOR_BIN" >&2; exit 1; }
[[ -f "$VECTOR_CONFIG" ]]   || { echo "Vector config not found: $VECTOR_CONFIG" >&2; exit 1; }
[[ -f "$LADING_CONFIG" ]]   || { echo "Lading config not found: $LADING_CONFIG" >&2; exit 1; }
for tool in lading sample inferno-collapse-sample inferno-flamegraph; do
    command -v "$tool" >/dev/null || { echo "Required tool not on PATH: $tool" >&2; exit 1; }
done

mkdir -p "$RUN_DIR"

echo "==> $LABEL"
echo "  Vector:  $VECTOR_BIN"
echo "  Config:  $VECTOR_CONFIG"
echo "  Output:  $RUN_DIR"
echo

VECTOR_PID=""
LADING_PID=""
cleanup() {
    local pids=()
    [[ -n "$VECTOR_PID" ]] && { kill "$VECTOR_PID" 2>/dev/null; pids+=("$VECTOR_PID"); }
    [[ -n "$LADING_PID" ]] && { kill "$LADING_PID" 2>/dev/null; pids+=("$LADING_PID"); }
    [[ ${#pids[@]} -gt 0 ]] && wait "${pids[@]}" 2>/dev/null || true
}
trap cleanup EXIT INT TERM

# Kill anything leftover from prior runs (lading or Vector on our ports)
pkill -f "$(basename "$VECTOR_BIN") --config $VECTOR_CONFIG" 2>/dev/null || true
pkill -f "lading --config-path $LADING_CONFIG" 2>/dev/null || true
sleep 1

echo "==> Starting Vector"
"$VECTOR_BIN" --config "$VECTOR_CONFIG" > "$RUN_DIR/vector.stdout" 2>&1 &
VECTOR_PID=$!
sleep 3
if ! kill -0 "$VECTOR_PID" 2>/dev/null; then
    echo "Vector crashed at startup:"
    tail -20 "$RUN_DIR/vector.stdout"
    exit 1
fi
echo "  PID $VECTOR_PID"

echo "==> Starting lading (${EXPERIMENT_SECONDS}s experiment)"
lading \
    --config-path "$LADING_CONFIG" \
    --no-target \
    --capture-path "$RUN_DIR/lading.captures" \
    --experiment-duration-seconds "$EXPERIMENT_SECONDS" \
    --warmup-duration-seconds 5 \
    > "$RUN_DIR/lading.stdout" 2>&1 &
LADING_PID=$!
echo "  PID $LADING_PID"

echo "==> Warming up ${WARMUP_SECONDS}s"
sleep "$WARMUP_SECONDS"

echo "  CPU at sample-start:"
ps -p "$VECTOR_PID" -o pcpu= -o pmem= | awk '{printf "    %.0f%% CPU, %.1f%% RSS\n", $1, $2}'

echo "==> Sampling for ${SAMPLE_SECONDS}s"
sample "$VECTOR_PID" "$SAMPLE_SECONDS" -file "$RUN_DIR/sample.txt" > /dev/null

echo "==> Generating flamegraph"
inferno-collapse-sample "$RUN_DIR/sample.txt" > "$RUN_DIR/sample.folded"
inferno-flamegraph --title "Vector regex parsing ($LABEL)" \
    "$RUN_DIR/sample.folded" > "$RUN_DIR/flamegraph.svg"

# Stop both processes and wait for them to exit before continuing.
# Must wait explicitly here — bash stalls at script exit until all tracked
# background jobs change state, causing a silent hang if we skip the wait.
kill "$LADING_PID" "$VECTOR_PID" 2>/dev/null || true
wait "$LADING_PID" "$VECTOR_PID" 2>/dev/null || true
VECTOR_PID=""
LADING_PID=""

echo
echo "==> Analysis"
python3 "$SCRIPT_DIR"/analysis.py "$RUN_DIR"

echo
echo "==> Outputs in $RUN_DIR"
echo "  flamegraph.svg     open with: open $RUN_DIR/flamegraph.svg"
echo "  sample.txt         raw macOS sample output"
echo "  sample.folded      collapsed stacks (inferno format)"
echo "  lading.captures    lading metrics (JSONL)"
echo "  vector.stdout      Vector logs"
