#! /usr/bin/env bash
set -e -o verbose

PATH=$PATH:$HOME/.cargo/bin

rustup toolchain install "$(cat rust-toolchain)"
rustup default "$(cat rust-toolchain)"
rustup component add rustfmt
rustup component add clippy
rustup target add wasm32-wasi
rustup run stable cargo install cargo-deb --version 1.29.2
rustup run stable cargo install cross --version 0.2.1

cd scripts
bundle update --bundler
bundle install
cd ..

sudo npm -g install markdownlint-cli

pip3 install jsonschema==3.2.0
pip3 install remarshal==0.11.2
