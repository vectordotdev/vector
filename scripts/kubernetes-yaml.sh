#!/usr/bin/env bash
set -euo pipefail

# kubernetes-yaml.sh
#
# SUMMARY
#
#   Manages the Kubernetes distribution YAML configs.
#   See usage function in the code or run without arguments.

cd "$(dirname "${BASH_SOURCE[0]}")/.."

CONFIGURATIONS_DIR="scripts/kubernetes-yaml"

generate() {
  local RELEASE_NAME="$1"
  local CHART="$2"
  local VALUES_FILE="$3"

  # Print header.
  cat <<EOF
# This file is generated from the Helm Chart by "scripts/kubernetes-yaml.sh".
# You might want to use the Helm Chart, see "$CHART" or the
# documentation on our website at https://vector.dev/docs.
# If you copied this file into you local setup - feel free to change it however
# you please.
# If you want to create a PR to the Vector's Kubernetes config - please do not
# edit this file directly. Instead, apply your changes to the Helm Chart.
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
    (
      # shellcheck disable=SC1090
      source "$CONFIG_FILE"
      generate "$RELEASE_NAME" "$CHART" "$VALUES_FILE" > "$TARGET_FILE"
    )
  done
}

check() {
  for CONFIG_FILE in "$CONFIGURATIONS_DIR"/*/config.sh; do
    VALUES_FILE="$(dirname "$CONFIG_FILE")/values.yaml"
    (
      # shellcheck disable=SC1090
      source "$CONFIG_FILE"
      GENERATED="$(generate "$RELEASE_NAME" "$CHART" "$VALUES_FILE")"
      FILE="$(cat "$TARGET_FILE")"

      if [[ "$GENERATED" != "$FILE" ]]; then
        echo "Error: Kubernetes YAML config ($TARGET_FILE) does not match the generated version" >&2
        exit 1
      fi
    )
  done
}

usage() {
  cat >&2 <<-EOF
Usage: $0 MODE

Modes:
  check  - compare the current file contents and the generated config and
           exit with non-zero exit code if they don't match
  update - update the file with the generated config
EOF
  exit 1
}

MODE="${1:-}"
case "$MODE" in
  update|check)
    "$MODE"
    ;;
  *)
    usage
    ;;
esac
