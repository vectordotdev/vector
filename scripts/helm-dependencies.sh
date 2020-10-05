#!/usr/bin/env bash
set -euo pipefail

# helm-dependencies.sh
#
# SUMMARY
#
#   Update Helm chart dependencies in the proper order to propagate
#   the changes.

cd "$(dirname "${BASH_SOURCE[0]}")/.."

# Read the shared scripting config.
source "distribution/helm/scripting-config.sh"

update() {
  for CHART in "${DEPENDENCY_UPDATE_ORDER[@]}"; do
    set -x
    helm dependency update --skip-refresh "distribution/helm/$CHART" "$@"
    { set +x; } &> /dev/null
  done
}

usage() {
  cat >&2 <<-EOF
Usage: $0 MODE

Modes:
  update - update Helm chart dependencies and vendor them to the respective
           charts/ dir of each chart.
EOF
  exit 1
}

MODE="${1:-}"
case "$MODE" in
  update)
    "$MODE"
    ;;
  *)
    usage
    ;;
esac
