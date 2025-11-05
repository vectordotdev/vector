#!/bin/bash
# ============================================================================
# Volume Seeding Script for Vector Test Runner
# ============================================================================
# Docker volumes mask image contents at mount points. This script copies
# precompiled binaries from /precompiled-target to /home/target volume.
#
# - DEVELOPMENT MODE: Seeds volume on first run (when source is mounted)
# - PRECOMPILED MODE: No-op (no volumes mounted)
# ============================================================================

# Check if precompiled binaries exist in the image
if [ -d /precompiled-target/debug ] && [ "$(ls -A /precompiled-target/debug 2>/dev/null)" ]; then
    # Count how many files exist in the target volume
    file_count=$(find /home/target/debug -type f 2>/dev/null | wc -l || echo "0")

    # Only seed if the volume is empty (first run)
    if [ "$file_count" -eq 0 ]; then
        echo "==> Seeding /home/target/debug with precompiled test binaries..."
        mkdir -p /home/target/debug/deps
        cp /precompiled-target/debug/* /home/target/debug/deps/
        echo "==> Seeding complete! Tests will start from precompiled binaries."
    else
        echo "==> /home/target already populated (skipping seed)"
    fi
else
    echo "==> No precompiled binaries found (PRECOMPILE=false mode)"
fi

# Execute the command passed to the container (e.g., /bin/sleep infinity)
exec "$@"
