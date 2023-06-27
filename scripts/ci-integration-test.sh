#!/bin/bash

# Used in CI to run and stop an integration test and upload the results of it.
# This is useful to allow retrying the integration test at a higher level than
# the nextest and reduce code duplication in the workflow file.

set -u

if [[ -z "${CI:-}" ]]; then
  echo "Aborted: this script is for use in CI." >&2
  exit 1
fi

if [ $# -ne 1 ]
then
  echo "usage: $0 INTEGRATION"
  exit 1
fi

set -x

INTEGRATION=$1

cargo vdev -v int start "${INTEGRATION}"
sleep 15
cargo vdev -v int test --retries 2 -a "${INTEGRATION}"
RET=$?
cargo vdev -v int stop "${INTEGRATION}"
./scripts/upload-test-results.sh
exit $RET
