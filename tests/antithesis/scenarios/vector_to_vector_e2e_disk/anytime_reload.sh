#!/usr/bin/env bash
# anytime_ command, baked into the Vector image and run IN a node container. It
# triggers a live config reload under load to exercise #24948 (silent loss when
# the disk-buffered sink is rebuilt on reload). Vector only rebuilds CHANGED
# components, so a bare SIGHUP can be a no-op; we swap the active config with a
# benign alternate first, forcing the sink to rebuild.
#
# Only node0 sets VECTOR_CONFIG_ALT, so this is a no-op on node1.
set -euo pipefail
[ -n "${VECTOR_CONFIG_ALT:-}" ] || exit 0
cfg="${VECTOR_CONFIG:?}"
alt="${VECTOR_CONFIG_ALT:?}"

tmp="$(mktemp)"
cp "$cfg" "$tmp"
cp "$alt" "$cfg"
cp "$tmp" "$alt"
rm -f "$tmp"

# Vector is PID 1 in the node container; SIGHUP triggers reload-from-disk.
kill -HUP 1
sleep 5
