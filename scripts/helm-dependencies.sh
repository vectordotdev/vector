#!/usr/bin/env bash
set -euo pipefail

# helm-dependencies.sh
#
# SUMMARY
#
#   Update Helm chart dependencies in the proper order to propagate
#   the changes.
#
#   This script implements our custom compatible dependency update mechanism,
#   rather than relying on the official `helm dependency update`.
#   We'd be happy to, however, the official mechanism doesn't produce
#   reproducible results.
#   See https://github.com/helm/helm/issues/8850

cd "$(dirname "${BASH_SOURCE[0]}")/.."

# Read the shared scripting config.
source "distribution/helm/scripting-config.sh"

list-chart-dependencies() {
  local CHART="$1"
  helm dependency list "$CHART" | tail -n +2 | sed '/^$/d' | awk '{ gsub("file://", "", $3); print $1, $3 }'
}

update() {
  for CHART in "${DEPENDENCY_UPDATE_ORDER[@]}"; do
    echo "=> $CHART"

    CHART_PATH="distribution/helm/$CHART"

    DEPENDENCIES_STRING="$(list-chart-dependencies "$CHART_PATH")"
    if [[ -z "$DEPENDENCIES_STRING" ]]; then
      echo "No dependencies"
      continue;
    fi

    CHART_VENDORED_DEPENDENCIES_PATH="$CHART_PATH/charts"
    rm -rf "$CHART_VENDORED_DEPENDENCIES_PATH"
    mkdir -p "$CHART_VENDORED_DEPENDENCIES_PATH"

    DEPENDENCIES=( "$DEPENDENCIES_STRING" )
    for DEPENDENCY_PAIR in "${DEPENDENCIES[@]}"; do
      read -ra KV <<<"$DEPENDENCY_PAIR"
      DEPENDENCY_NAME="${KV[0]}"
      DEPENDENCY_PATH="${KV[1]}"

      COPY_FROM="$CHART_PATH/$DEPENDENCY_PATH"
      COPY_TO="$CHART_VENDORED_DEPENDENCIES_PATH/$DEPENDENCY_NAME"

      echo "Copying \"$DEPENDENCY_NAME\" from \"$COPY_FROM\" to \"$COPY_TO\"..."
      cp -r -T "$COPY_FROM" "$COPY_TO"
    done
  done
}

list() {
  for CHART in "${DEPENDENCY_UPDATE_ORDER[@]}"; do
    echo "=> $CHART"
    list-chart-dependencies "distribution/helm/$CHART"
  done
}

validate() {
  for CHART in "${DEPENDENCY_UPDATE_ORDER[@]}"; do
    echo "=> $CHART"

    CHART_PATH="distribution/helm/$CHART"

    DEPENDENCIES_STRING="$(list-chart-dependencies "$CHART_PATH")"
    if [[ -z "$DEPENDENCIES_STRING" ]]; then
      echo "No dependencies"
      continue;
    fi

    CHART_VENDORED_DEPENDENCIES_PATH="$CHART_PATH/charts"
    DEPENDENCIES=( "$DEPENDENCIES_STRING" )
    for DEPENDENCY_PAIR in "${DEPENDENCIES[@]}"; do
      read -ra KV <<<"$DEPENDENCY_PAIR"
      DEPENDENCY_NAME="${KV[0]}"
      DEPENDENCY_PATH="${KV[1]}"

      VENDORED_PATH="$CHART_PATH/$DEPENDENCY_PATH"
      UPSTREAM_PATH="$CHART_VENDORED_DEPENDENCIES_PATH/$DEPENDENCY_NAME"

      echo "Validating \"$DEPENDENCY_NAME\" at \"$VENDORED_PATH\" against \"$UPSTREAM_PATH\"..."
      diff -qr "$VENDORED_PATH" "$UPSTREAM_PATH"
    done
  done
}

usage() {
  cat >&2 <<-EOF
Usage: $0 MODE

Modes:
  update   - update Helm chart dependencies and vendor them to the respective
             charts/ dir of each chart.
  list     - list the dependencies for each Helm chart.
  validate - check that vendored Helm chart dependencies are up-to-date with
             with their upstream counterparts.
EOF
  exit 1
}

MODE="${1:-}"
case "$MODE" in
  update|list|validate)
    "$MODE"
    ;;
  *)
    usage
    ;;
esac
