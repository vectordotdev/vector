#!/usr/bin/env bash
# Run Vector with hotpath profiling and watch it live in the hotpath TUI.
#
# Usage: ./run.sh [config.yaml]
#
# Without a config, runs a continuous demo_logs (json decode) -> remap -> console (json encode)
# pipeline. Quit the TUI with `q`; Vector is then stopped and its final report is written to
# $REPORT.
#
# Environment:
#   HOTPATH_FEATURE       hotpath (timing) or hotpath-alloc (timing + allocations, default)
#   EXTRA_FEATURES        extra comma-separated cargo features
#   HOTPATH_METRICS_PORT  port shared by Vector and the TUI (default 6770)
#   LOG                   Vector log file (default /tmp/vector-hotpath.log)
#   REPORT                final hotpath report file (default /tmp/vector-hotpath-report.txt)
set -euo pipefail
cd "$(dirname "$0")"

HOTPATH_FEATURE="${HOTPATH_FEATURE:-hotpath-alloc}"
LOG="${LOG:-/tmp/vector-hotpath.log}"
REPORT="${REPORT:-/tmp/vector-hotpath-report.txt}"
export HOTPATH_METRICS_PORT="${HOTPATH_METRICS_PORT:-6770}"

CONFIG="${1:-}"
if [[ -z "$CONFIG" ]]; then
  CONFIG=/tmp/vector-hotpath.yaml
  cat >"$CONFIG" <<'YAML'
sources:
  demo:
    type: demo_logs
    format: json
    interval: 0.0
    decoding:
      codec: json

transforms:
  parse:
    type: remap
    inputs: [demo]
    source: |
      .processed = true
      .host = upcase(string!(.host))

sinks:
  out:
    type: console
    inputs: [parse]
    encoding:
      codec: json
YAML
fi

if ! command -v hotpath >/dev/null 2>&1; then
  echo "==> Installing the hotpath TUI (cargo install hotpath --features=tui)"
  cargo install hotpath --features=tui --version '^0.30' --locked
fi

FEATURES="$(cargo vdev features "$CONFIG" | paste -sd, -),tikv-jemallocator,${HOTPATH_FEATURE}"
[[ -n "${EXTRA_FEATURES:-}" ]] && FEATURES="${FEATURES},${EXTRA_FEATURES}"

echo "==> Building Vector with features: ${FEATURES}"
cargo build --release --bin vector --no-default-features --features "$FEATURES"

echo "==> Starting Vector (config: ${CONFIG}, log: ${LOG})"
HOTPATH_OUTPUT_PATH="$REPORT" ./target/release/vector --config "$CONFIG" >/dev/null 2>"$LOG" &
VECTOR_PID=$!

stop_vector() {
  if kill -0 "$VECTOR_PID" 2>/dev/null; then
    echo "==> Stopping Vector"
    kill -INT "$VECTOR_PID" 2>/dev/null || true
    wait "$VECTOR_PID" 2>/dev/null || true
  fi
  [[ -s "$REPORT" ]] && echo "==> Final hotpath report: ${REPORT}"
}
trap stop_vector EXIT

# Give the metrics server a moment to bind before the TUI connects.
sleep 2
if ! kill -0 "$VECTOR_PID" 2>/dev/null; then
  echo "Vector exited early; see ${LOG}" >&2
  exit 1
fi

hotpath console
