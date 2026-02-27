#!/usr/bin/env bash
set -e -o verbose

# We want to ensure we're building using "full" release capabilities when possible, which
# means full LTO.  This maximizes performance of the resulting code, but increases
# compilation time.  We only set this if we're in CI _and_ we haven't been instructed to
# use the debug profile (via PROFILE environment variable).
# Note: codegen-units=1 is set directly in Cargo.toml's [profile.release].
if [[ "${CI-}" == "true" && "${PROFILE-}" != "debug" ]]; then
  {
    echo "CARGO_PROFILE_RELEASE_LTO=fat";
    echo "CARGO_PROFILE_RELEASE_DEBUG=false";
  } >> "${GITHUB_ENV}"
fi
