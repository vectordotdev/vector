#!/usr/bin/env bash
#
# Used in CI to run and stop an integration test and upload the results of it.
# This is useful to allow retrying the integration test at a higher level than
# nextest and reduce code duplication in the workflow file.

set -Eeuo pipefail

usage() {
  cat >&2 <<'USAGE'
Usage:
  scripts/run-integration-test.sh [OPTIONS] (int|e2e) TEST_NAME

Required positional arguments:
  TEST_TYPE   One of: int, e2e
  TEST_NAME   Name of the test/app (used as docker compose project name)

Options:
  -h         Show this help and exit
  -r <NUM>   Number of retries for the "test" phase (default: 2)
  -v         Increase verbosity; repeat for more (e.g. -vv or -vvv)
  -e <ENV>   Environment to export as TEST_ENV (default: not set)

Notes:
  - All existing two-argument invocations remain compatible:
      scripts/run-integration-test.sh int opentelemetry-logs
  - Additional options can be added later without breaking callers.
USAGE
}

# Defaults (tunable via options)
RETRIES=2
VERBOSITY=
TEST_ENV=

# Parse options
# Note: options must come before positional args (standard getopts behavior)
while getopts ":hr:ve:" opt; do
  case "$opt" in
    h)
      usage
      exit 0
      ;;
    r)
      RETRIES="$OPTARG"
      if ! [[ "$RETRIES" =~ ^[0-9]+$ ]] || [[ "$RETRIES" -lt 0 ]]; then
        echo "error: -r requires a non-negative integer (got: $RETRIES)" >&2
        exit 2
      fi
      ;;
    v)
      VERBOSITY+="v"
      ;;
    e)
      TEST_ENV="$OPTARG"
      ;;
    \?)
      echo "error: unknown option: -$OPTARG" >&2
      usage
      exit 2
      ;;
    :)
      echo "error: option -$OPTARG requires an argument" >&2
      usage
      exit 2
      ;;
  esac
done
shift $((OPTIND - 1))

# Validate required positional args
if [[ $# -ne 2 ]]; then
  echo "error: missing required positional arguments" >&2
  usage
  exit 1
fi

TEST_TYPE=$1   # either "int" or "e2e"
TEST_NAME=$2
VERBOSITY=${VERBOSITY:+-v}

case "$TEST_TYPE" in
  int|e2e) ;;
  *)
    echo "error: TEST_TYPE must be 'int' or 'e2e' (got: $TEST_TYPE)" >&2
    usage
    exit 1
    ;;
esac

set -x

SCRIPT_DIR=$(realpath "$(dirname "${BASH_SOURCE[0]}")")

print_compose_logs_on_failure() {
  local LAST_RETURN_CODE=$1
  if [[ "$LAST_RETURN_CODE" -ne 0 || "${ACTIONS_RUNNER_DEBUG:-}" == "true" ]]; then
    (docker compose --project-name "${TEST_NAME}" logs) || echo "Failed to collect logs"
  fi
}

if [[ "$TEST_NAME" == "opentelemetry-logs" ]]; then
  find "${SCRIPT_DIR}/../tests/data/e2e/opentelemetry/logs/output" -type f -name '*.log' -delete || true
  chmod -R 777 "${SCRIPT_DIR}/../tests/data/e2e/opentelemetry/logs/output" || true
fi

cargo vdev "${VERBOSITY}" "${TEST_TYPE}" start -a "${TEST_NAME}" || true
START_RET=$?
print_compose_logs_on_failure "$START_RET"

if [[ "$START_RET" -eq 0 ]]; then
  cargo vdev "${VERBOSITY}" "${TEST_TYPE}" test --retries "$RETRIES" -a "${TEST_NAME}"
  RET=$?
  print_compose_logs_on_failure "$RET"
else
  echo "Skipping test phase because 'vdev start' failed"
  RET=$START_RET
fi

cargo vdev "${VERBOSITY}" "${TEST_TYPE}" stop -a "${TEST_NAME}" || true

# Only upload test results if CI is defined
if [[ -n "${CI:-}" ]]; then
  ./scripts/upload-test-results.sh
fi

exit "$RET"
