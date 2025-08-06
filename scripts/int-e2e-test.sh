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

SCRIPT_DIR=$(realpath "$(dirname "${BASH_SOURCE[0]}")")
TEST_TYPE=$1 # either "int" or "e2e"
TEST_NAME=$2

print_compose_logs_on_failure() {
  local LAST_RETURN_CODE=$1

  if [[ $LAST_RETURN_CODE -ne 0 || "${ACTIONS_RUNNER_DEBUG:-}" == "true" ]]; then
      (docker compose --project-name "${TEST_NAME}" logs) || echo "Failed to collect logs"
  fi
}

if [[ "$TEST_NAME" == "opentelemetry-logs" ]]; then
  find "${SCRIPT_DIR}/../tests/data/e2e/opentelemetry/logs/output" -type f -name '*.log' -delete
  chmod -R 777 "${SCRIPT_DIR}/../tests/data/e2e/opentelemetry/logs/output"
fi

cargo vdev -v "${TEST_TYPE}" start -a "${TEST_NAME}"
START_RET=$?
print_compose_logs_on_failure $START_RET

if [[ $START_RET -eq 0 ]]; then
  cargo vdev -v "${TEST_TYPE}" test --retries 2 -a "${TEST_NAME}"
  RET=$?
  print_compose_logs_on_failure $RET
else
  echo "Skipping test phase because 'vdev start' failed"
  RET=$START_RET
fi

cargo vdev -v "${TEST_TYPE}" stop -a "${TEST_NAME}"

# Only upload test results if CI is defined
if [[ -n "${CI:-}" ]]; then
  ./scripts/upload-test-results.sh
fi

exit $RET
