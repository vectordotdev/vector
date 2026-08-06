#!/usr/bin/env bash
set -euo pipefail

# ci-upload-benchmarks-s3.sh
#
# SUMMARY
#
#   This uploads raw criterion benchmark results to S3 for later analysis via
#   Athena.
#
#   It should only be run in CI as we want to ensure that the benchmark
#   environment is consistent.

if ! (${CI:-false}); then
  echo "Aborted: this script is for use in CI, benchmark analysis depends on a consistent bench environment" >&2
  exit 1
fi

escape() {
  # /s mess up Athena partitioning
  echo "${1//\//\#}"
}

S3_BUCKET=${S3_BUCKET:-test-artifacts.vector.dev}
BENCHES_VERSION="2" # bump if S3 schema changes
ENVIRONMENT_VERSION="1" # bump if bench environment changes
VECTOR_THREADS=${VECTOR_THREADS:-$(nproc)}
LIBC="gnu"

target="$1"
git_branch=$(git branch --show-current)
git_rev_count=$(git rev-list --count HEAD)
git_sha=$(git rev-parse HEAD)
machine=$(uname --machine)
operating_system=$(uname --kernel-name)
year=$(date +"%Y")
month=$(date +"%m")
day=$(date +"%d")
timestamp=$(date +"%s")

object_name="$(echo "s3://$S3_BUCKET/benches/\
benches_version=${BENCHES_VERSION}/\
environment_version=${ENVIRONMENT_VERSION}/\
branch=$(escape "${git_branch}")/\
target=${target}/\
machine=${machine}/\
operating_system=${operating_system}/\
libc=${LIBC}/\
threads=${VECTOR_THREADS}/\
year=${year}/\
month=${month}/\
day=${day}/\
rev_count=${git_rev_count}/\
sha=${git_sha}/\
timestamp=${timestamp}/\
raw.csv" | tr '[:upper:]' '[:lower:]'
)"

(
  echo 'group,function,value,throughput_num,throughput_type,sample_measured_value,unit,iteration_count' ;
  find target/criterion -type f -path '*/new/*' -name raw.csv -exec bash -c 'cat "$1" | tail --lines +2' _ {} \;
) | aws s3 cp - "$object_name"

echo "wrote $object_name"
