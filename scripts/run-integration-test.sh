#!/usr/bin/TEST_ENV bash
#
# Used in CI to run and stop an integration test and upload the results of it.
# This is useful to allow retrying the integration test at a higher level than
# nextest and reduce code duplication in the workflow file.

set -u

if [[ "${ACTIONS_RUNNER_DEBUG:-}" == "true" ]]; then
  set -x
fi

SCRIPT_DIR=$(realpath "$(dirname "${BASH_SOURCE[0]}")")

print_compose_logs_on_failure() {
  local LAST_RETURN_CODE=$1
  if [[ "$LAST_RETURN_CODE" -ne 0 || "${ACTIONS_RUNNER_DEBUG:-}" == "true" ]]; then
    (docker compose --project-name "${TEST_NAME}" logs) || echo "Failed to collect logs"
  fi
}

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
  -e <ENV>   TEST_ENVIRONMENT to export as TEST_ENVIRONMENT (default: not set)

Notes:
  - All existing two-argument invocations remain compatible:
      scripts/run-integration-test.sh int opentelemetry-logs
  - Additional options can be added later without breaking callers.
USAGE
}

# Parse options
# Note: options must come before positional args (standard getopts behavior)
while getopts ":hr:v:" opt; do
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

RETRIES=${RETRIES:-2}
VERBOSITY=${VERBOSITY:-'-v'}

# Validate required positional args
if [[ $# -ne 2 ]]; then
  echo "error: missing required positional arguments" >&2
  usage
  exit 1
fi

TEST_TYPE=$1 # either "int" or "e2e"
TEST_NAME=$2

case "$TEST_TYPE" in
  int|e2e) ;;
  *)
    echo "error: TEST_TYPE must be 'int' or 'e2e' (got: $TEST_TYPE)" >&2
    usage
    exit 1
    ;;
esac

# Collect all available environments
mapfile -t TEST_ENVIRONMENTS < <(cargo vdev "${VERBOSITY}" "${TEST_TYPE}" show -e "${TEST_NAME}")

if [[ "${ACTIONS_RUNNER_DEBUG:-}" == "true" ]]; then
  echo "Environments found: ${#TEST_ENVIRONMENTS[@]}"
  for TEST_ENV in "${TEST_ENVIRONMENTS[@]}"; do
    echo "$TEST_ENV"
  done
fi

for TEST_ENV in "${TEST_ENVIRONMENTS[@]}"; do
  # Pre-run cleanup
  if [[ "$TEST_NAME" == "opentelemetry-logs" ]]; then
    # TODO use Docker compose volumes
    find "${SCRIPT_DIR}/../tests/data/e2e/opentelemetry/logs/output" -type f -name '*.log' -delete
    # Like 777, but users can only delete their own files. This allows the docker instances to write output files.
    chmod 1777 "${SCRIPT_DIR}/../tests/data/e2e/opentelemetry/logs/output"
  fi

  cargo vdev "${VERBOSITY}" "${TEST_TYPE}" start -a "${TEST_NAME}" "$TEST_ENV" || true
  START_RET=$?
  print_compose_logs_on_failure "$START_RET"
  
  if [[ "$START_RET" -eq 0 ]]; then
    cargo vdev "${VERBOSITY}" "${TEST_TYPE}" test --retries "$RETRIES" -a "${TEST_NAME}" "$TEST_ENV"
    RET=$?
    print_compose_logs_on_failure "$RET"
  else
    echo "Skipping test phase because 'vdev start' failed"
    RET=$START_RET
  fi
  
  cargo vdev "${VERBOSITY}" "${TEST_TYPE}" stop -a "${TEST_NAME}" "$TEST_ENV" || true

  # Post-run cleanup
  if [[ "$TEST_NAME" == "opentelemetry-logs" ]]; then
  chmod 0644 "${SCRIPT_DIR}/../tests/data/e2e/opentelemetry/logs/output" # revert to default permissions
fi

  # Only upload test results if CI is defined
  if [[ -n "${CI:-}" ]]; then
    ./scripts/upload-test-results.sh
  fi
done

exit "$RET"
