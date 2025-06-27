#!/bin/bash

# Used in CI to run and stop an integration test and upload the results of it.
# This is useful to allow retrying the integration test at a higher level than
# the nextest and reduce code duplication in the workflow file.

set -u

if [ $# -ne 2 ]
then
  echo "usage: $0 [int|e2e] TEST_NAME"
  exit 1
fi

set -x

TEST_TYPE=$1 # either "int" or "e2e"
TEST_NAME=$2

cargo vdev -v "${TEST_TYPE}" start -a "${TEST_NAME}"
sleep 15
cargo vdev -v "${TEST_TYPE}" test --retries 2 -a "${TEST_NAME}"
RET=$?

# Output docker compose logs on failure
if [[ $RET -ne 0 ]]; then
  SCRIPT_DIR=$(realpath "$(dirname "${BASH_SOURCE[0]}")")

  case "$TEST_TYPE" in
    int) TYPE_DIR="integration" ;;
    e2e) TYPE_DIR="e2e" ;;
    *) TYPE_DIR="" ;;
  esac

  if [[ -n "$TYPE_DIR" ]]; then
    COMPOSE_DIR="${SCRIPT_DIR}/${TYPE_DIR}/${TEST_NAME}"
    (cd "${COMPOSE_DIR}" && docker compose logs)
  fi
fi

cargo vdev -v "${TEST_TYPE}" stop -a "${TEST_NAME}"

# Only upload test results if CI is defined
if [[ -n "${CI:-}" ]]; then
  ./scripts/upload-test-results.sh
fi

exit $RET
