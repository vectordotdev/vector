#!/usr/bin/env bash
set -eou pipefail

# prepare-target-dir.sh
#
# SUMMARY
#
#   A script to work around the issue with docker volume mounts having
#   incorrect permissions.
#
#   Implemenmt a trick: we create the all paths that we use as docker volume
#   mounts manually, so that when we use them as mounts they're already there,
#   and docker doesn't create them owned as uid 0.

# To update the list at this file, use the following command:
#
#     yq r docker-compose.yml "services.*.volumes" | grep -o '\./target/[^:]*' | sort | uniq
#

DIRS=(
  ./target/aarch64-unknown-linux-musl/cargo/git
  ./target/aarch64-unknown-linux-musl/cargo/registry
  ./target/aarch64-unknown-linux-musl/rustup/tmp
  ./target/armv7-unknown-linux-musleabihf/cargo/git
  ./target/armv7-unknown-linux-musleabihf/cargo/registry
  ./target/armv7-unknown-linux-musleabihf/rustup/tmp
  ./target/armv7-unknwon-linux-musleabihf/cargo/git
  ./target/armv7-unknwon-linux-musleabihf/rustup/tmp
  ./target/cargo/git
  ./target/cargo/registry
  ./target/rustup/tmp
  ./target/x86_64-unknown-linux-musl/cargo/git
  ./target/x86_64-unknown-linux-musl/cargo/registry
  ./target/x86_64-unknown-linux-musl/rustup/tmp
)

mkdir -p "${DIRS[@]}"
