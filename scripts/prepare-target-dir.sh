#!/usr/bin/env bash
set -eou pipefail

# prepare-target-dir.sh
#
# SUMMARY
#
#   A script to work around the issue with Docker volume mounts having
#   incorrect permissions.
#
#   Implement a trick: We create all the paths we use as Docker volume
#   mounts manually, so that when we use them as mounts, they're already there
#   and Docker doesn't create them owned as uid 0.

DIRS=$(grep -o '\./target/[^:]*:' < docker-compose.yml | sed 's/:$//' | sort | uniq)

for DIR in ${DIRS}
do
    mkdir -p "${DIR}"
done
