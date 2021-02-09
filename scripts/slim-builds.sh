#!/usr/bin/env bash
set -euo pipefail

# slim-builds.sh
#
# SUMMARY
#
#   Sets various settings that produce disk optimized builds.
#   This is useful for CI where we currently only have 12gb
#   of disk space. We have routinely run into disk space errors
#   and this solves that.
#

mkdir -p .cargo/

cat <<-EOF >> ./.cargo/config
[build]
# On the CI, where this script runs, we won't be caching build artifacts.
# so we don't need to keep these around.
incremental = false
EOF
