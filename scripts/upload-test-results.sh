#!/usr/bin/env bash
set -euo pipefail
IFS=$'\n\t'

# upload-test-results.sh
#
# SUMMARY
#
#   Upload `cargo-nextest` JUnit output to Datadog

cd "$(dirname "${BASH_SOURCE[0]}")/.."
set -x

_os_platform="$(uname -s)"
_os_architecture="$(uname -m)"

export DD_TAGS="os.platform:$_os_platform,os.architecture:$_os_architecture"
export DD_ENV="${DD_ENV:-"local"}"

# TODO: outside contributors don't have access to the
# CI secrets, so allowing this to fail for now
datadog-ci junit upload \
  --service vector \
  target/nextest/default/junit.xml || echo "Failed to upload results"
