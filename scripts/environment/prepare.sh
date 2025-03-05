#! /usr/bin/env bash
set -e -o verbose

git config --global --add safe.directory /git/vectordotdev/vector

rustup show active-toolchain || rustup toolchain install
rustup show
if [[ "$(cargo-deb --version)" != "2.0.2" ]] ; then
  rustup run stable cargo install cargo-deb --version 2.0.0 --force --locked
fi
if [[ "$(cross --version | grep cross)" != "cross 0.2.5" ]] ; then
  rustup run stable cargo install cross --version 0.2.5 --force --locked
fi
if [[ "$(cargo-nextest --version)" != "cargo-nextest 0.9.72" ]] ; then
  rustup run stable cargo install cargo-nextest --version 0.9.72 --force --locked
fi
if [[ "$(cargo-deny --version)" != "cargo-deny 0.16.1" ]] ; then
  rustup run stable cargo install cargo-deny --version 0.16.1 --force --locked
fi
if ! dd-rust-license-tool --help >& /dev/null ; then
  rustup run stable cargo install dd-rust-license-tool --version 1.0.2 --force --locked
fi

if [[ "$(wasm-pack --version)" != "wasm-pack 0.13.1" ]] ; then
    echo "wasm-pack version 0.13.1 is not installed"
    cargo install --force --locked --version 0.13.1 wasm-pack
else
    echo "wasm-pack version 0.13.1 is installed already"
fi

# Currently fixing this to version 0.30 since version 0.31 has introduced
# a change that means it only works with versions of node > 10.
# https://github.com/igorshubovych/markdownlint-cli/issues/258
# ubuntu 20.04 gives us version 10.19. We can revert once we update the
# ci image to install a newer version of node.
sudo npm -g install markdownlint-cli@0.30
sudo npm -g install @datadog/datadog-ci

# Make sure our release build settings are present.
. scripts/environment/release-flags.sh
