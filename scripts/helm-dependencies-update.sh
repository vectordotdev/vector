#!/usr/bin/env bash
set -euo pipefail

# helm-dependencies-update.sh
#
# SUMMARY
#
#   Update Helm chart dependencies in the proper order to propagate
#   the changes.

cd "$(dirname "${BASH_SOURCE[0]}")/.."
set -x

# Read the shared scripting config.
source "distribution/helm/scripting-config.sh"

for CHART in "${DEPENDENCY_UPDATE_ORDER[@]}"; do
  helm dependency update --skip-refresh "distribution/helm/$CHART" "$@"
done
