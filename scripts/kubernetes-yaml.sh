#!/usr/bin/env bash
set -euo pipefail

# kubernetes-yaml.sh
#
# SUMMARY
#
#   Manages the Kubernetes distribution YAML configs.
#   See usage function in the code or run without arguments.

cd "$(dirname "${BASH_SOURCE[0]}")/.."

TARGET_FILE="distribution/kubernetes/vector.yaml"

generate() {
  # Print header.
  cat <<EOF
# This file is generated from the Helm Chart by "scripts/kubernetes-yaml.sh".
# You might want to use the Helm Chart, see "distribution/helm/vector" or the
# documentation on our website at https://vector.dev/docs.
# If you copied this file into you local setup - feel free to change it however
# you please.
# If you want to create a PR to the Vector's Kubernetes config - please do not
# edit this file directly. Instead, apply your changes to the Helm Chart.
EOF

  # Generate template.
  helm template \
    vector \
    distribution/helm/vector \
    --namespace vector \
    --create-namespace \
    --values scripts/kubernetes-yaml/values.yaml \
    --version master
}

update() {
  generate > "$TARGET_FILE"
}

check() {
  GENERATED="$(generate)"
  FILE="$(cat "$TARGET_FILE")"

  if [[ "$GENERATED" != "$FILE" ]]; then
    echo "Error: Kubernetes YAML config ($TARGET_FILE) does not match the generated version" >&2
    exit 1
  fi
}

usage() {
  cat >&2 <<-EOF
Usage: $0 MODE

Modes:
  check     - compare the current file contents and the generated config and
              exit with non-zero exit code if they don't match
  update    - update the file with the generated config
  generate  - generate the config and print it to stdout
EOF
  exit 1
}

MODE="${1:-}"
case "$MODE" in
  update|check|generate)
    "$MODE"
    ;;
  *)
    usage
    ;;
esac
