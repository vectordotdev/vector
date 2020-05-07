#!/usr/bin/env bash
set -euo pipefail

# prepare-target-dir.sh
#
# SUMMARY
#
#   A script to work around the issue with Docker volume mounts having
#   incorrect permissions.
#
#   Implements a trick: we create all the paths we use as Docker volume mounts
#   manually, so that when we use them as mounts, they're already there
#   and Docker doesn't create them owned as uid 0.

list-target-mounts() {
  grep -o '\./target/[^:]*:' < docker-compose.yml | sed 's/:$//' | sort | uniq
}

DIRS=()
while IFS='' read -r LINE; do DIRS+=("$LINE"); done < <(list-target-mounts)

mkdir -p "${DIRS[@]}"
