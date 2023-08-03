#!/bin/bash
# shellcheck source=/dev/null
source scl_source enable llvm-toolset-7
exec "$@"
