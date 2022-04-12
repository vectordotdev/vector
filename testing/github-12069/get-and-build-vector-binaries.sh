#!/usr/bin/env bash

curl -q -o vector-v0.20.0.tar.gz -L https://github.com/vectordotdev/vector/releases/download/v0.20.0/vector-0.20.0-x86_64-unknown-linux-gnu.tar.gz
tar -xzf vector-v0.20.0.tar.gz
mv vector-x86_64-unknown-linux-gnu/bin/vector vector-v0.20.0
rm -rf vector-x86_64-unknown-linux-gnu
rm -f vector-v0.20.0.tar.gz

pushd ../..
cargo clean && cargo build --release --no-default-features --features sources-stdin,sinks-http
popd || exit
cp ../../target/release/vector vector-pr
