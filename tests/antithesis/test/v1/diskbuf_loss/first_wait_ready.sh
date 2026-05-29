#!/usr/bin/env bash
# Runs once at the start of a timeline. Block until the self-driving lossfinder
# has delivered at least one record end-to-end through the disk_v2 buffer, i.e.
# the SUT is live. The exerciser itself emits setup_complete; this command does
# NOT do any lifecycle signaling.
set -euo pipefail
STATUS="${VDBUF_STATUS:-/tmp/vdbuf-status}"

for _ in $(seq 1 120); do
    if grep -qE 'delivered=[1-9][0-9]*' "$STATUS" 2>/dev/null; then
        echo "[first] lossfinder live: $(cat "$STATUS")"
        exit 0
    fi
    sleep 1
done

echo "[first] lossfinder never reported a delivery" >&2
exit 1
