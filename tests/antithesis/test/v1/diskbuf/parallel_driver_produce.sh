#!/usr/bin/env bash
# Continuously produce uniquely-IDed events into Vector (e2e acks) under fault
# injection, exercising the disk buffer's rotation / partial-write paths.
set -euo pipefail
exec /usr/bin/vdbuf-workload produce
