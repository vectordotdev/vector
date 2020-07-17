#! /usr/bin/env bash
set -e -o verbose

curl https://sh.rustup.rs -sSf | sh -s -- -y
source $HOME/.cargo/env

rustup target add wasm32-wasi
rustup toolchain install nightly --target x86_64-unknown-linux-musl
rustup toolchain install nightly --target armv7-unknown-linux-musleabihf
rustup toolchain install nightly --target aarch64-unknown-linux-musl
rustup component add rustfmt
rustup component add clippy
rustup default "$(cat rust-toolchain)"

cd scripts
bundle update --bundler
bundle install
cd ..

cd website
yarn install
cd ..
