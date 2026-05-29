#!/usr/bin/env bash
# Liveness: the writer/reader keep making progress. Antithesis runs this
# repeatedly and the property holds once delivery has advanced well past the
# initial round-trip. A buffer that has deadlocked (e.g. the #21683 wrap making
# is_buffer_full() true forever) would stall delivery and fail this property.
set -euo pipefail
STATUS="${VDBUF_STATUS:-/tmp/vdbuf-status}"

read_field() { grep -oE "$1=[0-9]+" "$STATUS" 2>/dev/null | cut -d= -f2; }

delivered="$(read_field delivered)"
if [[ -n "${delivered:-}" && "${delivered}" -gt 100 ]]; then
    echo "[eventually] sustained progress: delivered=${delivered}"
    exit 0
fi

echo "[eventually] not enough progress yet: delivered=${delivered:-none}" >&2
exit 1
