#!/usr/bin/env bash
# Active workload presence + a coarse safety mirror of the SUT-side assertions.
# The exerciser self-drives the buffer; this command runs in parallel for the
# timeline, continuously checking the cheap externally-visible safety invariant:
# the buffer can never hand the reader more records than were ever produced.
# A get_total_records / accounting underflow would surface as handled > produced.
# The authoritative detector is the SUT-side assert_always! inside vector-buffers;
# this is belt-and-suspenders and gives Antithesis an active command to schedule.
set -uo pipefail
STATUS="${VDBUF_STATUS:-/tmp/vdbuf-status}"

read_field() { grep -oE "$1=[0-9]+" "$STATUS" 2>/dev/null | cut -d= -f2; }

# Bounded loop so the command terminates within a timeline rather than running
# truly forever.
for _ in $(seq 1 600); do
    produced="$(read_field produced)"
    handled="$(read_field handled)"
    if [[ -n "${produced:-}" && -n "${handled:-}" ]]; then
        if (( handled > produced )); then
            echo "[safety] VIOLATION: handled=${handled} > produced=${produced}" >&2
            exit 1
        fi
    fi
    sleep 1
done

echo "[safety] no violation observed over the window"
exit 0
