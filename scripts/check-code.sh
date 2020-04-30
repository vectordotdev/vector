#!/bin/bash

# check-code.sh
#
# SUMMARY
#
#   Checks all Vector code

set -exuo pipefail

cargo check --all --all-targets