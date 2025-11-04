#!/bin/bash
# Seed the /home/target volume with precompiled test binaries from the image.
# This script runs as the container entrypoint to populate the persistent volume
# with test binaries that were compiled during the Docker image build.
#
# Why this is needed:
# - Test binaries are compiled at /home/target during image build
# - At runtime, /home/target is mounted as a Docker volume
# - Docker volumes hide/mask the image contents at the mount point
# - So we copy binaries from /precompiled-target (not masked) to /home/target (volume)
# - This only runs once when the volume is empty

if [ -d /precompiled-target/debug ]; then
    # Count files in /home/target/debug (excluding . and ..)
    file_count=$(find /home/target/debug -type f 2>/dev/null | wc -l || echo "0")
    if [ "$file_count" -eq 0 ]; then
        echo "Seeding /home/target with precompiled binaries..."
        mkdir -p /home/target/debug
        cp -r /precompiled-target/debug/* /home/target/debug/
        echo "Seeded successfully"
    fi
fi

# Execute the command passed to the container (e.g., /bin/sleep infinity)
exec "$@"
