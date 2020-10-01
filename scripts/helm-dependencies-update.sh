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

ORDER=(
  # Lowest level.
  vector-shared

  # Intermediate level.
  vector-daemonset

  # Highest level.
  vector
)

for CHART in "${ORDER[@]}"; do
  helm dependency update "distribution/helm/$CHART" --skip-refresh "$@"
done
