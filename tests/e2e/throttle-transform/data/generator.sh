#!/bin/sh
# Event generator for throttle transform E2E tests.
# Sends JSON events via HTTP to Vector's http source.
set -e

VECTOR_URL="${VECTOR_URL:-http://vector:9090}"
TOTAL="${TOTAL_EVENTS:-200}"
DELAY="${EVENT_DELAY_MS:-10}"

# Wait for Vector to be ready
echo "Waiting for Vector at $VECTOR_URL ..."
until wget -qO /dev/null "$VECTOR_URL" 2>/dev/null; do
  sleep 0.5
done
echo "Vector is ready."

i=0
while [ "$i" -lt "$TOTAL" ]; do
  svc_num=$((i % 5))
  msg_size=$((50 + i % 200))
  payload=$(printf '{"service":"svc-%d","id":%d,"level":"info","message":"%*s"}' \
    "$svc_num" "$i" "$msg_size" "" | tr ' ' 'x')

  wget -qO /dev/null --post-data="$payload" \
    --header="Content-Type: application/json" \
    "$VECTOR_URL" 2>/dev/null || true

  i=$((i + 1))

  # Throttle the generator slightly
  if [ "$DELAY" -gt 0 ]; then
    sleep "0.$(printf '%03d' "$DELAY")"
  fi
done

echo "Sent $TOTAL events."
# Give Vector time to process and flush
sleep 5
echo "Done."
