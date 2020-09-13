#! /usr/bin/env bash
set -e -o verbose

rustup default "$(cat rust-toolchain)"
rustup component add rustfmt
rustup component add clippy
rustup target add wasm32-wasi

cd scripts
bundle update --bundler
bundle install
cd ..

cd scripts
npm install markdownlint-cli
cd ..

pip3 install jsonschema==3.2.0
pip3 install remarshal==0.11.2
