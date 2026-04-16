#!/usr/bin/env bash

# Used in CI to run and stop an integration test and upload the results of it.
# This is useful to allow retrying the integration test at a higher level than
# nextest and reduce code duplication in the workflow file.

set -u

if [[ "${ACTIONS_RUNNER_DEBUG:-}" == "true" ]]; then
  set -x
fi

vdev_cmd="${VDEV:-cargo vdev}"

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
  -e <ENV>   One or more environments to run (repeatable or comma-separated).
             If provided, these are used as TEST_ENVIRONMENTS instead of auto-discovery.
  -c         Collect code coverage (outputs target/coverage/lcov.info)

Notes:
  - All existing two-argument invocations remain compatible:
      scripts/run-integration-test.sh int opentelemetry-logs
  - Additional options can be added later without breaking callers.
USAGE
}

# Parse options
# Note: options must come before positional args (standard getopts behavior)
TEST_ENV=""
COVERAGE=false
while getopts ":hr:v:e:c" opt; do
  case "$opt" in
    h)
      usage
      exit 0
      ;;
    r)
      RETRIES="$OPTARG"
      if ! [[ "$RETRIES" =~ ^[0-9]+$ ]] || [[ "$RETRIES" -lt 0 ]]; then
        echo "ERROR: -r requires a non-negative integer (got: $RETRIES)" >&2
        exit 2
      fi
      ;;
    v)
      VERBOSITY+="v"
      ;;
    e)
      TEST_ENV="$OPTARG"
      ;;
    c)
      COVERAGE=true
      ;;
    \?)
      echo "ERROR: unknown option: -$OPTARG" >&2
      usage
      exit 2
      ;;
    :)
      echo "ERROR: option -$OPTARG requires an argument" >&2
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
  echo "ERROR: missing required positional arguments" >&2
  usage
  exit 1
fi

TEST_TYPE=$1 # either "int" or "e2e"
TEST_NAME=$2

case "$TEST_TYPE" in
  int|e2e) ;;
  *)
    echo "ERROR: TEST_TYPE must be 'int' or 'e2e' (got: $TEST_TYPE)" >&2
    usage
    exit 1
    ;;
esac

# Determine environments to run
if [[ ${#TEST_ENV} -gt 0 ]]; then
  # Use the environments supplied via -e
  TEST_ENVIRONMENTS="${TEST_ENV}"
else
  # Collect all available environments via auto-discovery
  mapfile -t TEST_ENVIRONMENTS < <($vdev_cmd "${VERBOSITY}" "${TEST_TYPE}" show -e "${TEST_NAME}")
  if [[ ${#TEST_ENVIRONMENTS[@]} -eq 0 ]]; then
    echo "ERROR: no environments found for ${TEST_TYPE} test '${TEST_NAME}'" >&2
    exit 1
  fi
fi

# Remove stale combined coverage from a previous (possibly failed) attempt so
# retries via nick-fields/retry don't append to leftover data.
rm -f target/coverage/lcov-combined.info

for TEST_ENV in "${TEST_ENVIRONMENTS[@]}"; do
  # Execution flow for each environment:
  # 1. Clean up previous test output
  # 2. Start environment
  # 3. If start succeeded:
  #    - Run tests
  #    - Upload results to Datadog CI
  # 4. If start failed:
  #    - Skip test phase
  #    - Exit with error code
  # 5. Stop environment (always, best effort)
  # 6. Exit if there was a failure

  docker run --rm -v vector_target:/output/"${TEST_NAME}" alpine:3.20 \
    sh -c "rm -rf /output/${TEST_NAME}/*"

  $vdev_cmd "${VERBOSITY}" "${TEST_TYPE}" start "${TEST_NAME}" "${TEST_ENV}"
  START_RET=$?
  print_compose_logs_on_failure "$START_RET"

  if [[ "$START_RET" -eq 0 ]]; then
    COVERAGE_FLAG=""
    [[ "$COVERAGE" == "true" ]] && COVERAGE_FLAG="--coverage"

    $vdev_cmd "${VERBOSITY}" "${TEST_TYPE}" test --retries "$RETRIES" ${COVERAGE_FLAG} "${TEST_NAME}" "${TEST_ENV}"
    RET=$?
    print_compose_logs_on_failure "$RET"

    # Normalize source paths in coverage report so they are relative to the repo root.
    # The test runner container mounts source at /home/vector; strip that prefix so
    # Datadog can resolve files against the repository root (e.g. SF:src/foo.rs).
    # Append each environment's coverage to a combined file so multi-env services
    # preserve all coverage data (not just the last environment).
    if [[ "$COVERAGE" == "true" && "$RET" -eq 0 ]]; then
      LCOV_FILE="target/coverage/lcov.info"
      if [[ -f "$LCOV_FILE" ]]; then
        sed -i 's|SF:/home/vector/|SF:|g' "$LCOV_FILE"
        cat "$LCOV_FILE" >> target/coverage/lcov-combined.info
        rm "$LCOV_FILE"
      fi
    fi

    # Upload test results only if the vdev test step ran
    ./scripts/upload-test-results.sh
  else
    echo "Skipping test phase because 'vdev start' failed"
    RET=$START_RET
  fi

  # Always stop the environment (best effort cleanup)
  $vdev_cmd "${VERBOSITY}" "${TEST_TYPE}" stop "${TEST_NAME}" || true

  # Exit early on first failure
  if [[ "$RET" -ne 0 ]]; then
    exit "$RET"
  fi
done

# Promote combined coverage file to the expected output path
if [[ "$COVERAGE" == "true" && -f target/coverage/lcov-combined.info ]]; then
  mv target/coverage/lcov-combined.info target/coverage/lcov.info
fi

exit 0
