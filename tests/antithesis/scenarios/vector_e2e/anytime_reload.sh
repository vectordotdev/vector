#!/usr/bin/env bash

set -euo pipefail
[ -n "${VECTOR_CONFIG_ALT:-}" ] || exit 0
cfg="${VECTOR_CONFIG:?}"
alt="${VECTOR_CONFIG_ALT:?}"

# Vector only ever reads $cfg, so reload alternates $cfg between two immutable
# sources rather than swapping two live files. The alternate $alt is never
# written, and the baseline (the original $cfg) is snapshotted once, so the only
# mutable file is $cfg and the only writes to it are a single rename of a fully
# written temp. The node-termination fault can therefore interrupt this script at
# any point and leave $cfg as one complete config or the other, never half-written
# and never collapsed so both sources hold the same content. Alternation always
# resumes on the next invocation.
base="$cfg.orig"
if [ ! -f "$base" ]; then
    cp "$cfg" "$base.tmp"
    mv "$base.tmp" "$base"
fi

# Pick whichever source is not currently live. cksum reads from stdin so its
# output is the checksum alone, with no filename to differ on.
if [ "$(cksum <"$cfg")" = "$(cksum <"$alt")" ]; then
    next="$base"
else
    next="$alt"
fi
cp "$next" "$cfg.tmp"
mv "$cfg.tmp" "$cfg"

# Vector is PID 1 in the node container. SIGHUP triggers reload-from-disk.
kill -HUP 1
sleep 5
