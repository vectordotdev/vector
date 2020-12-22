#!/usr/bin/env bash
set -euo pipefail

# helm-template-snapshot.sh
#
# SUMMARY
#
#   Manages the Helm template snapshots.
#   See usage function in the code or run without arguments.

cd "$(dirname "${BASH_SOURCE[0]}")/.."

CONFIGURATIONS_DIR="tests/helm-snapshots"

generate() {
  local RELEASE_NAME="$1"
  local CHART="$2"
  local VALUES_FILE="$3"

  # Print header.
  cat <<EOF
# Do not edit!
# This file is generated
# - by "scripts/helm-snapshot-tests.sh"
# - for the chart at "$CHART"
# - with the values from "$VALUES_FILE"
EOF

  # Generate template.
  # TODO: use app-version when https://github.com/helm/helm/issues/8670 is solved
  helm template \
    "$RELEASE_NAME" \
    "$CHART" \
    --namespace vector \
    --create-namespace \
    --values "$VALUES_FILE" \
    --version master
}

update() {
  for CONFIG_FILE in "$CONFIGURATIONS_DIR"/*/config.sh; do
    VALUES_FILE="$(dirname "$CONFIG_FILE")/values.yaml"
    TARGET_FILE="$(dirname "$CONFIG_FILE")/snapshot.yaml"
    (
      # shellcheck disable=SC1090
      source "$CONFIG_FILE"
      generate "$RELEASE_NAME" "$CHART" "$VALUES_FILE" >"$TARGET_FILE"
    )
  done
}

check() {
  for CONFIG_FILE in "$CONFIGURATIONS_DIR"/*/config.sh; do
    VALUES_FILE="$(dirname "$CONFIG_FILE")/values.yaml"
    TARGET_FILE="$(dirname "$CONFIG_FILE")/snapshot.yaml"
    (
      # shellcheck disable=SC1090
      source "$CONFIG_FILE"
      GENERATED="$(generate "$RELEASE_NAME" "$CHART" "$VALUES_FILE")"
      FILE="$(cat "$TARGET_FILE")"

      if [[ "$GENERATED" != "$FILE" ]]; then
        echo "Error: snapshot ($TARGET_FILE) does not match the generated version" >&2
        exit 1
      fi
    )
  done
}

usage() {
  cat >&2 <<-EOF
Usage: $0 MODE

Modes:
  check  - run the tests, compare the generated outputs with snapshots and
           exit with non-zero exit code if the outputs do not match
  update - run the tests and update the snapshots from the generated output
EOF
  exit 1
}

MODE="${1:-}"
case "$MODE" in
update | check)
  "$MODE"
  ;;
*)
  usage
  ;;
esac
