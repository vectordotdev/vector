#!/usr/bin/env bash
set -euo pipefail

# release-prepare.sh
#
# SUMMARY
#
#   Update Kubernetes manifests from latest stable release and
#   create a new Cue file for the new release.

set -eu

cd "$(dirname "${BASH_SOURCE[0]}")/.."
set -x

cargo vdev generate manifests
cargo vdev generate release-cue
