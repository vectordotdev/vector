#!/usr/bin/env bash

set -euo pipefail
[ -n "${VECTOR_CONFIG_ALT:-}" ] || exit 0
cfg="${VECTOR_CONFIG:?}"
alt="${VECTOR_CONFIG_ALT:?}"

tmp="$(mktemp)"
cp "$cfg" "$tmp"
cp "$alt" "$cfg"
cp "$tmp" "$alt"
rm -f "$tmp"

# Vector is PID 1 in the node container. SIGHUP triggers reload-from-disk.
kill -HUP 1
sleep 5
