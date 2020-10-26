#!/bin/bash
set -euo pipefail
cd "$(dirname "${BASH_SOURCE[0]}")/.."

BASE_URL="${1:-"${BASE_URL}"}"
COMPONENT_NAME="${2:-"${COMPONENT_NAME}"}"
URL="${BASE_URL}/metrics"

STATS_STREAM_SLEEP_SECS="${STATS_STREAM_SLEEP_SECS:-"10"}"

cat <<EOF
Current settings:
  - url:                         $URL
  - delay between stat ticks:    $STATS_STREAM_SLEEP_SECS secs

Columns:
  events total, events since last tick, events per second since last tick, tick timestamp

EOF

CURRENT_PROCESSED_EVENTS=0
while true; do
  LAST_PROCESSED_EVENTS="$CURRENT_PROCESSED_EVENTS"
  CURRENT_TIME="$(date --rfc-3339=seconds)"
  CURRENT_PROCESSED_EVENTS="$(curl -s "$URL" | { grep 'events_processed' || true; } | { grep -E 'component_name="?('"${COMPONENT_NAME}"')"?' || true; } | awk '{ print $NF }' | awk '{ sum+=$1 } END { print sum }')"
  if [[ -z "$CURRENT_PROCESSED_EVENTS" ]]; then
    CURRENT_PROCESSED_EVENTS="$LAST_PROCESSED_EVENTS"
  fi
  PROCESSED_SINCE_LAST_TICK="$((CURRENT_PROCESSED_EVENTS - LAST_PROCESSED_EVENTS))"
  PROCESSED_PER_SECOND="$((PROCESSED_SINCE_LAST_TICK / STATS_STREAM_SLEEP_SECS))"
  printf "%10s\t%10s\t%10s\t%s\n" "$CURRENT_PROCESSED_EVENTS" "$PROCESSED_SINCE_LAST_TICK" "$PROCESSED_PER_SECOND" "$CURRENT_TIME"
  sleep "$STATS_STREAM_SLEEP_SECS"
done
