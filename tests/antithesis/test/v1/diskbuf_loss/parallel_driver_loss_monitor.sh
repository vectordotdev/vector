#!/usr/bin/env bash
# Active workload presence + a continuous mirror of the SUT-side loss assertion.
# The lossfinder self-drives the buffer; this command runs in parallel for the
# timeline, continuously checking the externally-visible loss counter. The
# authoritative detector is the SUT-side assert_always! inside the exerciser;
# this is belt-and-suspenders and gives Antithesis an active command to schedule.
set -uo pipefail
STATUS="${VDBUF_STATUS:-/tmp/vdbuf-status}"

read_field() { grep -oE "$1=[0-9]+" "$STATUS" 2>/dev/null | cut -d= -f2; }

# Bounded loop so the command terminates within a timeline rather than running
# truly forever.
for _ in $(seq 1 600); do
    silent_loss="$(read_field silent_loss)"
    if [[ -n "${silent_loss:-}" ]] && (( silent_loss > 0 )); then
        echo "[loss] VIOLATION: silent_loss=${silent_loss} ($(cat "$STATUS"))" >&2
        exit 1
    fi
    sleep 1
done

echo "[loss] no silent data loss observed over the window"
exit 0
