#!/usr/bin/env bash
set -euo pipefail

# check-helm-lint.sh
#
# SUMMARY
#
#   Checks that Helm charts pass `helm lint`.

helm lint distribution/helm/*/
