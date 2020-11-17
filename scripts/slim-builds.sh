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

cat <<-EOF >> ./Cargo.toml
[profile.dev]
# See defaults https://doc.rust-lang.org/cargo/reference/profiles.html#dev
opt-level = 0
debug = true
debug-assertions = true
overflow-checks = true
lto = false
panic = 'unwind'
# Disabled, see build.incremental
# incremental = true
codegen-units = 256
rpath = false

[profile.release]
# See defaults https://doc.rust-lang.org/cargo/reference/profiles.html#release
opt-level = 3
debug = false
debug-assertions = false
overflow-checks = false
lto = false
panic = 'unwind'
# Disabled, see build.incremental
# incremental = false
codegen-units = 1
rpath = false
EOF
