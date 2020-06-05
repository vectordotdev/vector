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
# focus on fast, lean builds
[build]
incremental = false
EOF

cat <<-EOF >> ./Cargo.toml
# focus on fast, lean builds
[profile.dev]
debug = false
opt-level = "s" # Binary size
lto = false # Don't LTO on CI
EOF
