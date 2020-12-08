#!/bin/bash
set -euo pipefail

# test-behavior.sh
#
# SUMMARY
#
#   Run behavioral tests

$(find target -type f -executable -name vector | head -n1) test tests/behavior/**/*.toml
