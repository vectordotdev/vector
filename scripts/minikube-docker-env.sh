#!/usr/bin/env bash
set -euo pipefail

if ! COMMANDS="$(minikube docker-env)"; then
  echo "Unable to obtain docker env from minikube; is minikube started?" >&2
  exit 7
fi

eval "$COMMANDS"
