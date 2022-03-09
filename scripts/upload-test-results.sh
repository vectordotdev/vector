#!/bin/bash; C:\Program\ Files\Git\bin\bash.exe
set -euo pipefail
IFS=$'\n\t'

# upload-test-restults.sh
#
# SUMMARY
#
#   Upload `cargo-nextest` JUnit output to Datadog

cd "$(dirname "${BASH_SOURCE[0]}")/.."
set -x

DD_ENV="${DD_ENV:-"local"}" datadog-ci junit upload \
  --service vector \
  target/nextest/default/junit.xml
