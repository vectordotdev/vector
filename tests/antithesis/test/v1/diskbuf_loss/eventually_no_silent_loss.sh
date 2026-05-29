#!/usr/bin/env bash
# Safety property: the disk_v2 buffer never silently loses a record that was
# accepted and durably flushed. The lossfinder maintains an oracle and increments
# `silent_loss` whenever a flushed-but-unresolved record is detected after a full
# drain. This command surfaces any detected loss as an Antithesis property
# failure: exit 0 only if silent_loss=0, else exit 1.
set -uo pipefail
STATUS="${VDBUF_STATUS:-/tmp/vdbuf-status}"

read_field() { grep -oE "$1=[0-9]+" "$STATUS" 2>/dev/null | cut -d= -f2; }

silent_loss="$(read_field silent_loss)"
if [[ -z "${silent_loss:-}" ]]; then
    echo "[eventually] status not ready yet" >&2
    exit 1
fi

if (( silent_loss > 0 )); then
    echo "[eventually] SILENT DATA LOSS detected: silent_loss=${silent_loss} ($(cat "$STATUS"))" >&2
    exit 1
fi

echo "[eventually] no silent data loss observed: $(cat "$STATUS")"
exit 0
