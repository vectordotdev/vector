#!/bin/bash

# Used in CI to run and stop an integration test and upload the results of it.
# This is useful to allow retrying the integration test at a higher level than
# the nextest and reduce code duplication in the workflow file.

set -u

if [[ -z "${CI:-}" ]]; then
  echo "Aborted: this script is for use in CI." >&2
  exit 1
fi

if [ $# -ne 2 ]
then
  echo "usage: $0 [int|e2e] TEST_NAME"
  exit 1
fi

set -x

TEST_TYPE=$1 # either "int" or "e2e"
TEST_NAME=$2

cargo vdev -v "${TEST_TYPE}" start -a "${TEST_NAME}"
sleep 30
cargo vdev -v "${TEST_TYPE}" test --retries 2 -a "${TEST_NAME}"
RET=$?
cargo vdev -v "${TEST_TYPE}" stop -a "${TEST_NAME}"
./scripts/upload-test-results.sh
exit $RET
