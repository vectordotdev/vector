#! /usr/bin/env bash
set -e -o verbose

# We want to ensure we're building using "full" release capabilities when possible, which
# means full LTO and a single codegen unit.  This maximizes performance of the resulting
# code, but increases compilation time.  We only set this if we're in CI _and_ we haven't
# been instructed not to via OPTIMIZED_BUILD.
if [ "${CI-}" == "true" && "${PROFILE-}" != "debug" ] ; then
    echo  "RUSTFLAGS=\"${RUSTFLAGS} -C lto=true -C codegen-units=1\"" >> $GITHUB_ENV
fi