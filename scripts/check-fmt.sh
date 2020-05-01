#!/bin/bash

# check-fmt.sh
#
# SUMMARY
#
#   Checks the format of Vector code

set -exuo pipefail

cargo fmt -- --check