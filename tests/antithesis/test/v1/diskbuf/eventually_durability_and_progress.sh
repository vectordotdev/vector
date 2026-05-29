#!/usr/bin/env bash
# Faults paused: verify durability (every acked event reached the collector)
# and writer progress (a fresh post-recovery write is delivered -> no #21683
# permanent deadlock).
set -euo pipefail
exec /usr/bin/vdbuf-workload check
