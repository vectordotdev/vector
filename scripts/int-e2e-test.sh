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

  if [[ $LAST_RETURN_CODE -ne 0 ]]; then
    case "$TEST_TYPE" in
      int) TYPE_DIR="integration" ;;
      e2e) TYPE_DIR="e2e" ;;
      *) TYPE_DIR="" ;;
    esac

    if [[ -n "$TYPE_DIR" ]]; then
      local COMPOSE_DIR="${SCRIPT_DIR}/${TYPE_DIR}/${TEST_NAME}"
      (docker compose -f "${COMPOSE_DIR}/compose.yaml" logs) || echo "Failed to collect logs"
    fi
  fi
}

if [[ "$TEST_NAME" == "opentelemetry-logs" ]]; then
  docker run --rm \
    -v opentelemetry-logs_otel-e2e-output:/data \
    alpine \
    sh -c 'find /data -type f -name "*.log" -delete'
fi

cargo vdev -v "${TEST_TYPE}" start -a "${TEST_NAME}"
START_RET=$?
print_compose_logs_on_failure $START_RET

# TODO this is arbitrary sleep. Investigate if it can be safely removed.
sleep 15

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
