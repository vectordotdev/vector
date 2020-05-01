#!/usr/bin/env bash
set -eou pipefail

# prepare-target-dir.sh
#
# SUMMARY
#
#   A script to work around the issue with docker volume mounts having
#   incorrect permissions.
#
#   Implemenmt a trick: we create the all paths that we use as docker volume
#   mounts manually, so that when we use them as mounts they're already there,
#   and docker doesn't create them owned as uid 0.

mapfile -t DIRS < <(grep -o '\./target/[^:]*:' < docker-compose.yml | sed 's/:$//' | sort | uniq)
mkdir -p "${DIRS[@]}"
