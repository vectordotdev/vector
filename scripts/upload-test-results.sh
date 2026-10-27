#!/usr/bin/env bash
set -euo pipefail
IFS=$'\n\t'

# upload-test-results.sh
#
# SUMMARY
#
#   Upload JUnit output to Datadog (CI only)
#   Print test results location when running locally
#
# USAGE
#
#   upload-test-results.sh [JUNIT_FILE] [SERVICE]
#
#   JUNIT_FILE defaults to the `cargo-nextest` output: target/nextest/default/junit.xml
#   SERVICE defaults to `vector`
#
# NOTES
#
#   Only uploads in CI environments. Prints file location when called locally.

JUNIT_FILE="${1:-"$(dirname "${BASH_SOURCE[0]}")/../target/nextest/default/junit.xml"}"
SERVICE="${2:-vector}"

# Print location locally, upload in CI
if [[ -z "${CI:-}" ]]; then
  if [[ -f "$JUNIT_FILE" ]]; then
    echo "Test results available at: $JUNIT_FILE"
  fi
  exit 0
fi

set -x

_os_platform="$(uname -s)"
_os_architecture="$(uname -m)"

export DD_TAGS="os.platform:$_os_platform,os.architecture:$_os_architecture"
export DD_ENV="${DD_ENV:-"local"}"

# TODO: outside contributors don't have access to the CI secrets, so upload might fail.
datadog-ci junit upload --service "${SERVICE}" "${JUNIT_FILE}" || echo "Failed to upload results"
