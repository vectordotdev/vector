#!/bin/bash
# Performance E2E benchmarks for the throttle transform.
#
# Usage:
#   ./run_perf.sh [--release]
#
# Builds vector (debug or release) then runs each perf config variant,
# capturing timing and memory stats.
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
PROJECT_ROOT="$(cd "$SCRIPT_DIR/../../../../.." && pwd)"

BUILD_MODE="debug"
if [[ "${1:-}" == "--release" ]]; then
    BUILD_MODE="release"
    BUILD_FLAG="--release"
else
    BUILD_FLAG=""
fi

echo "=== Throttle Transform Performance Benchmarks ==="
echo "Build mode: $BUILD_MODE"
echo ""

# Build vector
echo "Building vector ($BUILD_MODE)..."
cargo build -p vector $BUILD_FLAG --features "sources-demo_logs,sinks-blackhole,transforms-throttle,transforms-remap" 2>&1 | tail -3

VECTOR_BIN="$PROJECT_ROOT/target/$BUILD_MODE/vector"

if [[ ! -x "$VECTOR_BIN" ]]; then
    echo "ERROR: Vector binary not found at $VECTOR_BIN"
    exit 1
fi

TMPDIR=$(mktemp -d)
trap "rm -rf $TMPDIR" EXIT

CONFIGS=(
    "vector_perf_events_only.yaml:Events only"
    "vector_perf_bytes_only.yaml:JSON bytes only"
    "vector_perf_multi.yaml:Events + bytes"
    "vector_perf_metrics_off.yaml:Metrics OFF (baseline)"
    "vector_perf_metrics_detailed.yaml:Metrics detailed ON"
    "vector_perf_metrics_both.yaml:Metrics both ON"
)

printf "%-35s %12s %12s\n" "Config" "Duration(s)" "Events/sec"
printf "%s\n" "$(printf '=%.0s' {1..60})"

for entry in "${CONFIGS[@]}"; do
    IFS=':' read -r config_file label <<< "$entry"
    config_path="$SCRIPT_DIR/$config_file"

    if [[ ! -f "$config_path" ]]; then
        echo "SKIP: $config_file not found"
        continue
    fi

    DATA_DIR="$TMPDIR/data_$(date +%s%N)"
    mkdir -p "$DATA_DIR"

    START_TIME=$(date +%s%N)

    "$VECTOR_BIN" -c "$config_path" \
        --quiet \
        2>/dev/null || true

    END_TIME=$(date +%s%N)
    ELAPSED_NS=$((END_TIME - START_TIME))
    ELAPSED_S=$(echo "scale=3; $ELAPSED_NS / 1000000000" | bc)

    EVENTS=100000
    EPS=$(echo "scale=0; $EVENTS / $ELAPSED_S" | bc 2>/dev/null || echo "N/A")

    printf "%-35s %12s %12s\n" "$label" "${ELAPSED_S}s" "$EPS"
done

echo ""
echo "Done. For more accurate results, use --release and run multiple iterations."
