#!/bin/bash

# test-behavior.sh
#
# SUMMARY
#
#   Run behaviorial tests

set -euo pipefail

$(find target -type f -executable -name vector | head -n1) test tests/behavior/**/*.toml
