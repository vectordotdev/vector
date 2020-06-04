#! /usr/bin/env bash
set -e

rustup default "$(cat rust-toolchain)"
rustup component add rustfmt

cd scripts
bundle update --bundler
bundle install
cd ..

cd website
yarn
cd ..
